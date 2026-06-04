#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
use serde_json::{json, Value};
use std::rc::Rc;

fn doc_with(blocks: Vec<EditorBlock>) -> EditorDoc {
    EditorDoc {
        blocks,
        front_matter: lopress_core::FrontMatter::default(),
    }
}

fn paragraph_with_id(text: &str) -> (BlockId, EditorBlock) {
    let b = EditorBlock::paragraph(vec![InlineRun::plain(text)]);
    let id = b.id;
    (id, b)
}

fn run_text(block: &EditorBlock) -> String {
    match &block.body {
        BlockBody::Inline(runs) => runs.iter().map(|r| r.text.clone()).collect(),
        BlockBody::Code(t) => t.clone(),
        _ => String::new(),
    }
}

#[test]
fn change_type_on_opaque_block_is_noop_to_prevent_data_loss() {
    // An Opaque (unknown-plugin) block has no sensible conversion to another
    // kind. Changing only its kind yields {kind: X, body: Opaque}, which
    // `to_core` cannot serialize — the block is silently lost on save. So
    // ChangeType on an Opaque block must be a no-op, leaving it intact and
    // round-trippable; recovery for these blocks is via Delete only.
    let block = EditorBlock::opaque(
        "lopress:video".to_string(),
        json!({ "type": "lopress:video", "attrs": { "src": "x.mp4" } }),
    );
    let id = block.id;
    let mut doc = doc_with(vec![block]);

    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Paragraph,
        },
    );

    assert_eq!(doc.blocks.len(), 1, "block must not be dropped");
    assert!(
        matches!(&doc.blocks[0].kind, BlockKind::Opaque { type_name } if type_name.as_ref() == "lopress:video"),
        "kind must stay Opaque, got {:?}",
        doc.blocks[0].kind
    );
    assert!(
        matches!(doc.blocks[0].body, BlockBody::Opaque(_)),
        "body must stay Opaque so it round-trips through to_core"
    );
}

#[test]
fn split_paragraph_at_middle() {
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 5,
            new_block_id: None,
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(run_text(&doc.blocks[0]), "hello");
    assert_eq!(run_text(&doc.blocks[1]), " world");
}

#[test]
fn split_at_end_creates_empty_trailing_block() {
    let (id, block) = paragraph_with_id("hi");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 2,
            new_block_id: None,
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(run_text(&doc.blocks[0]), "hi");
    assert_eq!(run_text(&doc.blocks[1]), "");
}

#[test]
fn stale_inline_commit_after_change_to_code_keeps_body_renderable() {
    // Regression repro of the toolbar "Code" button bug. The button's
    // PointerDown handler emits a pre-commit EditBlockBody{Inline} then
    // ChangeType{Code}. ChangeType triggers a current_doc.update() that
    // rebuilds the editor pane, unmounting the old paragraph inline editor,
    // which fires FocusLost and emits a *stray* EditBlockBody{Inline} for the
    // now-Code block. Before the fix, that stray commit replaced the Code body
    // with an Inline one, leaving {kind: Code, body: Inline} — a pair no render
    // arm matches, so the block drew as an empty, uneditable, unselectable gap.
    // The model must coerce the body to the block's kind so it stays renderable.
    let (id, block) = paragraph_with_id("let x = 1;");
    let mut doc = doc_with(vec![block]);

    // 1. Toolbar pre-commit (block still Paragraph — Inline shape matches).
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("let x = 1;")])),
            built_in: false,
        },
    );
    // 2. Change type to Code.
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code { lang: Rc::from("") },
        },
    );
    // 3. Stray FocusLost commit from the unmounted paragraph editor.
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("let x = 1;")])),
            built_in: false,
        },
    );

    let block = &doc.blocks[0];
    assert!(
        matches!(block.kind, BlockKind::Code { .. }),
        "kind should remain Code, got {:?}",
        block.kind
    );
    assert!(
        matches!(block.body, BlockBody::Code(_)),
        "body must stay Code-shaped so the block renders; got {:?}",
        block.body
    );
    assert_eq!(run_text(block), "let x = 1;");
}

#[test]
fn stale_builtin_list_commit_after_change_to_heading_coerces_without_panicking() {
    // Regression repro of the toolbar "H2"-on-a-list crash. Clicking a kind
    // button on a focused list dispatches ChangeType{Heading}, which flattens
    // the list body to Inline. The list editor — unmounted by the ensuing
    // editor-pane rebuild — then fires its FocusLost flush: a *built-in*
    // EditBlockBody{List} for the now-Heading block. Unlike the inline editor,
    // the list editor is never pre-committed by the toolbar (the toolbar has no
    // handle to the list buffer), so this flush is the *only* path that carries
    // freshly-typed list text into the model — coerce_body_to_kind flattens it
    // to the heading's Inline shape. A previous pre-coercion debug_assert
    // panicked on this legitimate built-in commit; the model must instead coerce
    // it and keep the text.
    let item = |t: &str| ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain(t)],
    };
    let block = EditorBlock::list(false, vec![item("alpha"), item("beta")]);
    let id = block.id;
    let mut doc = doc_with(vec![block]);

    // 1. Toolbar dispatches ChangeType to Heading 2 (no pre-commit for lists).
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    // 2. Stale built-in FocusLost flush from the unmounted list editor, carrying
    //    a freshly-typed third item that only ever lived in the editor buffer.
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::List(vec![
                item("alpha"),
                item("beta"),
                item("gamma"),
            ])),
            built_in: true,
        },
    );

    let block = &doc.blocks[0];
    assert!(
        matches!(block.kind, BlockKind::Heading(2)),
        "kind should stay Heading(2), got {:?}",
        block.kind
    );
    assert!(
        matches!(block.body, BlockBody::Inline(_)),
        "body must be coerced to Inline so the block renders, got {:?}",
        block.body
    );
    assert!(
        run_text(block).contains("gamma"),
        "the flush's freshly-typed list text must survive coercion, got {:?}",
        run_text(block)
    );
}

#[test]
fn split_heading_keeps_level() {
    let block = EditorBlock::heading(2, vec![InlineRun::plain("Title goes here")]);
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 5,
            new_block_id: None,
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading(2)));
    assert!(matches!(doc.blocks[1].kind, BlockKind::Heading(2)));
}

#[test]
fn split_unknown_block_is_noop() {
    let (_id, block) = paragraph_with_id("hello");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: BlockId::new(),
            byte_offset: 1,
            new_block_id: None,
        },
    );
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "hello");
}

#[test]
fn merge_appends_runs_to_prev() {
    let (id_a, a) = paragraph_with_id("hello ");
    let (id_b, b) = paragraph_with_id("world");
    let mut doc = doc_with(vec![a, b]);
    apply(&mut doc, BlockAction::MergeWithPrev { block_id: id_b });
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].id, id_a);
    assert_eq!(run_text(&doc.blocks[0]), "hello world");
}

#[test]
fn merge_first_block_is_noop() {
    let (id_a, a) = paragraph_with_id("first");
    let mut doc = doc_with(vec![a]);
    apply(&mut doc, BlockAction::MergeWithPrev { block_id: id_a });
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "first");
}

#[test]
fn insert_after_places_correctly() {
    let (id, a) = paragraph_with_id("anchor");
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::InsertAfter {
            anchor: id,
            new_block: Box::new(EditorBlock::heading(1, vec![InlineRun::plain("Title")])),
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[1].kind, BlockKind::Heading(1)));
}

#[test]
fn insert_after_unknown_anchor_appends() {
    let (_id, a) = paragraph_with_id("x");
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::InsertAfter {
            anchor: BlockId::new(),
            new_block: Box::new(EditorBlock::paragraph(vec![InlineRun::plain("y")])),
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(run_text(&doc.blocks[1]), "y");
}

#[test]
fn delete_last_block_inserts_empty_paragraph() {
    let (id, a) = paragraph_with_id("only");
    let mut doc = doc_with(vec![a]);
    apply(&mut doc, BlockAction::Delete { block_id: id });
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0].kind, BlockKind::Paragraph));
    assert_eq!(run_text(&doc.blocks[0]), "");
}

#[test]
fn delete_middle_block() {
    let (_, a) = paragraph_with_id("a");
    let (id_b, b) = paragraph_with_id("b");
    let (_, c) = paragraph_with_id("c");
    let mut doc = doc_with(vec![a, b, c]);
    apply(&mut doc, BlockAction::Delete { block_id: id_b });
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(run_text(&doc.blocks[0]), "a");
    assert_eq!(run_text(&doc.blocks[1]), "c");
}

#[test]
fn move_forward_one_position() {
    let (id_a, a) = paragraph_with_id("a");
    let (id_b, b) = paragraph_with_id("b");
    let (id_c, c) = paragraph_with_id("c");
    let mut doc = doc_with(vec![a, b, c]);
    apply(
        &mut doc,
        BlockAction::Move {
            block_id: id_a,
            to_index: 2,
        },
    );
    assert_eq!(doc.blocks[0].id, id_b);
    assert_eq!(doc.blocks[1].id, id_a);
    assert_eq!(doc.blocks[2].id, id_c);
}

#[test]
fn move_backward() {
    let (id_a, a) = paragraph_with_id("a");
    let (id_b, b) = paragraph_with_id("b");
    let (id_c, c) = paragraph_with_id("c");
    let mut doc = doc_with(vec![a, b, c]);
    apply(
        &mut doc,
        BlockAction::Move {
            block_id: id_c,
            to_index: 0,
        },
    );
    assert_eq!(doc.blocks[0].id, id_c);
    assert_eq!(doc.blocks[1].id, id_a);
    assert_eq!(doc.blocks[2].id, id_b);
}

#[test]
fn move_first_to_end_gap() {
    let (id_a, a) = paragraph_with_id("a");
    let (id_b, b) = paragraph_with_id("b");
    let (id_c, c) = paragraph_with_id("c");
    let mut doc = doc_with(vec![a, b, c]);
    apply(
        &mut doc,
        BlockAction::Move {
            block_id: id_a,
            to_index: 3,
        },
    );
    assert_eq!(doc.blocks[0].id, id_b);
    assert_eq!(doc.blocks[1].id, id_c);
    assert_eq!(doc.blocks[2].id, id_a);
}

#[test]
fn move_to_self_adjacent_gap_is_noop() {
    let (id_a, a) = paragraph_with_id("a");
    let (id_b, b) = paragraph_with_id("b");
    let (id_c, c) = paragraph_with_id("c");
    let mut doc = doc_with(vec![a, b, c]);
    // gap immediately after id_b: that's where id_b already lives.
    apply(
        &mut doc,
        BlockAction::Move {
            block_id: id_b,
            to_index: 2,
        },
    );
    assert_eq!(doc.blocks[0].id, id_a);
    assert_eq!(doc.blocks[1].id, id_b);
    assert_eq!(doc.blocks[2].id, id_c);
}

#[test]
fn change_paragraph_to_heading() {
    let (id, a) = paragraph_with_id("title");
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading(2)));
    assert_eq!(run_text(&doc.blocks[0]), "title");
}

#[test]
fn change_paragraph_to_code_flattens_runs() {
    let block = EditorBlock::paragraph(vec![
        InlineRun::plain("hello "),
        InlineRun {
            text: "world".into(),
            bold: true,
            ..Default::default()
        },
    ]);
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code {
                lang: Rc::from("rust"),
            },
        },
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::Code { .. }));
    assert_eq!(run_text(&doc.blocks[0]), "hello world");
}

#[test]
fn edit_block_body_inline_replaces_runs() {
    let (id, a) = paragraph_with_id("old");
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("new")])),
            built_in: false,
        },
    );
    assert_eq!(run_text(&doc.blocks[0]), "new");
}

#[test]
fn edit_block_body_code_replaces_text() {
    let block = EditorBlock::code("rust".into(), "old".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Code("new".into())),
            built_in: false,
        },
    );
    assert_eq!(run_text(&doc.blocks[0]), "new");
}

#[test]
fn split_code_inserts_newline() {
    let block = EditorBlock::code("rust".into(), "fn main() {}".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 8,
            new_block_id: None,
        },
    );
    // Code block does not split; a newline is inserted at offset.
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "fn main(\n) {}");
}

#[test]
fn change_type_to_list_stamps_plugin_meta() {
    // A list created in-editor (toolbar / slash menu emit `ChangeType`) must
    // carry list `PluginMeta`, exactly like a list loaded via `from_core` —
    // otherwise it takes neither the plugin render path nor native
    // serialization and renders/serializes as nothing.
    let (id, block) = paragraph_with_id("an item");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: false },
        },
    );
    let block = &doc.blocks[0];
    assert!(matches!(block.kind, BlockKind::List { ordered: false }));
    assert!(matches!(block.body, BlockBody::List(_)));
    let meta = block
        .plugin
        .as_ref()
        .expect("a list block created via ChangeType must carry PluginMeta");
    assert_eq!(meta.editor.as_deref(), Some("list"));
    assert_eq!(meta.native.as_deref(), Some("list"));
    assert!(meta.builtin);
    assert_eq!(meta.block_type_name.as_ref(), "list");
}

#[test]
fn change_type_to_list_serializes_as_native_list() {
    // The plugin meta from `ChangeType` must drive `to_core`'s native branch,
    // so the new list serializes as a bare `list` core block.
    let (id, block) = paragraph_with_id("an item");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: true },
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].children[0].r#type, "list_item");
}

#[test]
fn split_with_new_block_id_uses_provided_id() {
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);
    let target_id = BlockId::new();
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 5,
            new_block_id: Some(target_id),
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[1].id, target_id);
}

#[test]
fn split_with_new_block_id_none_mints_fresh_id() {
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            byte_offset: 5,
            new_block_id: None,
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert_ne!(doc.blocks[1].id, doc.blocks[0].id);
}
/// For every recordable action, `apply` must return the inverse action that
/// would restore the doc. Applying that inverse to the post-state must
/// reproduce the original pre-state.
mod inverse_symmetry {
    use super::*;

    #[test]
    fn split_round_trip_id_stable() {
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let before_len = doc.blocks.len();
        let before_text = run_text(&doc.blocks[0]);
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 5,
                new_block_id: None,
            },
        )
        .expect("Split must record an inverse");

        // Canonical must carry the minted id.
        let minted_id = match &canonical {
            BlockAction::Split {
                new_block_id: Some(nid),
                ..
            } => *nid,
            _ => panic!("canonical Split must have a concrete new_block_id"),
        };
        assert_eq!(doc.blocks[1].id, minted_id);

        // Apply the inverse; doc must restore the pre-state content. The
        // run *structure* may differ (split+merge of plain runs leaves two
        // adjacent runs instead of one consolidated run) — compare flat
        // text, which is what "restored to pre-state" means semantically.
        let _ = apply(&mut doc, inverse).expect("inverse must record");
        assert_eq!(doc.blocks.len(), before_len);
        assert_eq!(run_text(&doc.blocks[0]), before_text);
    }

    #[test]
    fn split_redo_uses_same_id() {
        // Apply Split -> undo (apply the inverse) -> re-apply the canonical
        // Split -> the new block must have the SAME id as the first time,
        // because canonical carries it.
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 5,
                new_block_id: None,
            },
        )
        .expect("Split must record");
        let original_new_id = doc.blocks[1].id;

        // Undo.
        let _ = apply(&mut doc, inverse).expect("inverse must record");
        assert_eq!(doc.blocks.len(), 1);

        // Redo the canonical form.
        let _ = apply(&mut doc, canonical).expect("canonical re-apply must record");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(
            doc.blocks[1].id, original_new_id,
            "redo must reuse the original new_block_id"
        );
    }

    #[test]
    fn open_slash_menu_returns_none() {
        let (id, block) = paragraph_with_id("anything");
        let mut doc = doc_with(vec![block]);
        let result = apply(&mut doc, BlockAction::OpenSlashMenu { block_id: id });
        assert!(result.is_none(), "OpenSlashMenu is UI-only, unrecorded");
    }

    /// Snapshot a doc's block content as a list of (id, flat-text) tuples.
    /// Stable across inverse round-trips for the non-lossy variants — runs
    /// may consolidate differently, but the text and id sequence must match.
    fn snapshot(doc: &EditorDoc) -> Vec<(BlockId, String, &'static str)> {
        doc.blocks
            .iter()
            .map(|b| {
                let (text, tag) = match &b.body {
                    BlockBody::Inline(runs) => {
                        (runs.iter().map(|r| r.text.as_str()).collect(), "inline")
                    }
                    BlockBody::Code(t) => (t.clone(), "code"),
                    BlockBody::List(items) => (
                        items
                            .iter()
                            .map(|it| -> String {
                                it.runs.iter().map(|r| r.text.as_str()).collect()
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                        "list",
                    ),
                    BlockBody::Opaque(_) => (String::new(), "opaque"),
                    BlockBody::Table(data) => (
                        lopress_editor::actions::body_to_flat_text(&BlockBody::Table(data.clone())),
                        "table",
                    ),
                };
                (b.id, text, tag)
            })
            .collect()
    }

    /// Apply `action` then its returned inverse, asserting the doc returns
    /// to its pre-state snapshot. The shape of the inverse is also checked
    /// to be non-trivial (apply must have recorded something).
    fn assert_round_trip(doc: &mut EditorDoc, action: BlockAction) {
        let before = snapshot(doc);
        let (_canonical, inverse) = apply(doc, action).expect("action must record");
        let _ = apply(doc, inverse).expect("inverse must record");
        assert_eq!(snapshot(doc), before);
    }

    #[test]
    fn merge_with_prev_round_trip() {
        let (_id_a, a) = paragraph_with_id("hello ");
        let (id_b, b) = paragraph_with_id("world");
        let mut doc = doc_with(vec![a, b]);
        assert_round_trip(&mut doc, BlockAction::MergeWithPrev { block_id: id_b });
    }

    #[test]
    fn delete_round_trip() {
        let (_id_a, a) = paragraph_with_id("anchor");
        let (id_b, b) = paragraph_with_id("victim");
        let mut doc = doc_with(vec![a, b]);
        assert_round_trip(&mut doc, BlockAction::Delete { block_id: id_b });
    }

    #[test]
    fn insert_after_round_trip() {
        let (id_a, a) = paragraph_with_id("anchor");
        let mut doc = doc_with(vec![a]);
        let new_block = Box::new(EditorBlock::paragraph(vec![InlineRun::plain("inserted")]));
        assert_round_trip(
            &mut doc,
            BlockAction::InsertAfter {
                anchor: id_a,
                new_block,
            },
        );
    }

    #[test]
    fn move_round_trip_forward() {
        let (id_a, a) = paragraph_with_id("a");
        let (_id_b, b) = paragraph_with_id("b");
        let (_id_c, c) = paragraph_with_id("c");
        let mut doc = doc_with(vec![a, b, c]);
        assert_round_trip(
            &mut doc,
            BlockAction::Move {
                block_id: id_a,
                to_index: 2,
            },
        );
    }

    #[test]
    fn move_round_trip_backward() {
        let (_id_a, a) = paragraph_with_id("a");
        let (_id_b, b) = paragraph_with_id("b");
        let (id_c, c) = paragraph_with_id("c");
        let mut doc = doc_with(vec![a, b, c]);
        assert_round_trip(
            &mut doc,
            BlockAction::Move {
                block_id: id_c,
                to_index: 0,
            },
        );
    }

    #[test]
    fn change_type_paragraph_heading_round_trip() {
        // Paragraph↔Heading conversions are body-preserving (both Inline),
        // so this round-trip should be lossless.
        let (id, block) = paragraph_with_id("title");
        let mut doc = doc_with(vec![block]);
        assert_round_trip(
            &mut doc,
            BlockAction::ChangeType {
                block_id: id,
                new_kind: BlockKind::Heading(2),
            },
        );
    }

    #[test]
    fn edit_block_body_inline_round_trip() {
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let new_body = Box::new(BlockBody::Inline(vec![InlineRun::plain(
            "entirely different content",
        )]));
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
                built_in: false,
            },
        );
    }

    #[test]
    fn edit_block_body_code_round_trip() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("fn main() {}".to_string());
        block.kind = BlockKind::Code { lang: Rc::from("") };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        let new_body = Box::new(BlockBody::Code("fn other() { /* ... */ }".to_string()));
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
                built_in: false,
            },
        );
    }

    #[test]
    fn edit_block_body_list_round_trip() {
        use lopress_editor::model::types::ListItem;
        let it0 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("first")],
        };
        let it1 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("second")],
        };
        let list = EditorBlock::list(false, vec![it0, it1]);
        let id = list.id;
        let mut doc = doc_with(vec![list]);
        let new_body = Box::new(BlockBody::List(vec![
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("entirely")],
            },
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("different")],
            },
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("items")],
            },
        ]));
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
                built_in: false,
            },
        );
    }

    #[test]
    fn split_code_is_now_recordable() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("foobar".to_string());
        block.kind = BlockKind::Code { lang: Rc::from("") };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        assert_round_trip(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 3,
                new_block_id: None,
            },
        );
        // After undo, the Code body should be restored to "foobar".
        match &doc.blocks[0].body {
            BlockBody::Code(text) => assert_eq!(text, "foobar"),
            _ => panic!("expected Code body"),
        }
    }

    #[test]
    fn split_list_block_is_now_recordable() {
        use lopress_editor::model::types::ListItem;
        let it0 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("ab")],
        };
        let it1 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("cd")],
        };
        let original_item_ids = vec![it0.id, it1.id];
        let list = EditorBlock::list(false, vec![it0, it1]);
        let block_id = list.id;
        let mut doc = doc_with(vec![list]);
        // Top-level Split on the list at flat-offset 4: item 0 has 2 chars
        // + 1 newline = cumulative 3, so offset 4 lands inside item 1 at
        // local-offset 1 (between 'c' and 'd').
        assert_round_trip(
            &mut doc,
            BlockAction::Split {
                block_id,
                byte_offset: 4,
                new_block_id: None,
            },
        );
        // After undo, the list should have its original two items with their
        // original ids restored.
        match &doc.blocks[0].body {
            BlockBody::List(items) => {
                let ids: Vec<_> = items.iter().map(|it| it.id).collect();
                assert_eq!(
                    ids, original_item_ids,
                    "undo must restore the original item ids"
                );
            }
            _ => panic!("expected List body"),
        }
    }
}

// ============================================================================
// ChangeType conversion-arm tests — covering all 12 directions.
// The Inline→Inline cases (Paragraph↔Heading) are already tested above.
// The Inline→Code and Inline→List cases are tested above.
// The remaining 8 directions were broken (body/plugin mismatch) and are
// exercised here.
// ============================================================================

#[test]
fn change_type_code_to_paragraph_converts_body_to_inline() {
    let block = EditorBlock::code("rust".into(), "fn main() {}".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Paragraph,
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::Paragraph));
    assert!(
        matches!(&b.body, BlockBody::Inline(runs) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == "fn main() {}"),
        "body must be Inline with the original code text"
    );
    assert!(b.plugin.is_none(), "plugin must be cleared for Paragraph");
}

#[test]
fn change_type_code_to_heading_converts_body_to_inline() {
    let block = EditorBlock::code("python".into(), "print('hello')".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::Heading(2)));
    assert!(
        matches!(&b.body, BlockBody::Inline(runs) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == "print('hello')"),
        "body must be Inline with the original code text"
    );
    assert!(b.plugin.is_none(), "plugin must be cleared for Heading");
}

#[test]
fn change_type_code_to_list_converts_body_to_list() {
    let block = EditorBlock::code("rust".into(), "fn main() {}".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: false },
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::List { ordered: false }));
    match &b.body {
        BlockBody::List(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0]
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>(),
                "fn main() {}"
            );
        }
        _ => panic!("body must be List"),
    }
    let meta = b
        .plugin
        .as_ref()
        .expect("a list block must carry PluginMeta");
    assert_eq!(meta.block_type_name.as_ref(), "list");
}

#[test]
fn change_type_code_to_code_updates_lang_and_mirrors_into_plugin() {
    // Changing the lang on an existing code block must update both
    // BlockKind::Code.lang AND plugin.attrs["lang"].
    let mut block = EditorBlock::code("rust".into(), "fn main() {}".into());
    // Stamp a PluginMeta manually (simulating a block loaded via from_core).
    block.plugin = Some(PluginMeta::code("rust"));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code {
                lang: "python".into(),
            },
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(&b.kind, BlockKind::Code { lang } if &**lang == "python"));
    assert!(
        matches!(&b.body, BlockBody::Code(t) if t == "fn main() {}"),
        "code text must be preserved"
    );
    let meta = b.plugin.as_ref().expect("code block must carry PluginMeta");
    assert_eq!(
        meta.attrs.get("lang").and_then(Value::as_str),
        Some("python"),
        "plugin.attrs[\"lang\"] must mirror the new lang"
    );
}

#[test]
fn change_type_list_to_paragraph_converts_body_to_inline() {
    let it = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("first item")],
    };
    let mut block = EditorBlock::list(false, vec![it]);
    // Stamp list PluginMeta (matching what from_core produces).
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Paragraph,
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::Paragraph));
    assert!(
        matches!(&b.body, BlockBody::Inline(runs) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == "first item"),
        "body must be Inline with flattened list item text"
    );
    assert!(b.plugin.is_none(), "plugin must be cleared for Paragraph");
}

#[test]
fn change_type_list_to_heading_converts_body_to_inline() {
    let it0 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("first")],
    };
    let it1 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("second")],
    };
    let mut block = EditorBlock::list(true, vec![it0, it1]);
    block.plugin = Some(PluginMeta::list(true));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(3),
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::Heading(3)));
    assert!(
        matches!(&b.body, BlockBody::Inline(runs) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == "first\nsecond"),
        "body must be Inline with joined list item texts"
    );
    assert!(b.plugin.is_none(), "plugin must be cleared for Heading");
}

#[test]
fn change_type_list_to_code_converts_body_to_code() {
    let it0 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("line1")],
    };
    let it1 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("line2")],
    };
    let mut block = EditorBlock::list(false, vec![it0, it1]);
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code {
                lang: "bash".into(),
            },
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(&b.kind, BlockKind::Code { lang } if &**lang == "bash"));
    assert!(
        matches!(&b.body, BlockBody::Code(t) if t == "line1\nline2"),
        "code body must be joined list item texts"
    );
    let meta = b.plugin.as_ref().expect("code block must carry PluginMeta");
    assert_eq!(meta.block_type_name.as_ref(), "code");
}

#[test]
fn change_type_list_to_list_updates_ordered_and_mirrors_into_plugin() {
    // Toggling ordered on an existing list must update BlockKind::List.ordered
    // AND plugin.attrs["ordered"].
    let it = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("item")],
    };
    let mut block = EditorBlock::list(false, vec![it]);
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: true },
        },
    );
    let b = &doc.blocks[0];
    assert!(matches!(b.kind, BlockKind::List { ordered: true }));
    match &b.body {
        BlockBody::List(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].runs, vec![InlineRun::plain("item")]);
        }
        _ => panic!("body must be List"),
    }
    let meta = b.plugin.as_ref().expect("list block must carry PluginMeta");
    assert_eq!(
        meta.attrs.get("ordered").and_then(Value::as_bool),
        Some(true),
        "plugin.attrs[\"ordered\"] must mirror the new ordered flag"
    );
}

#[test]
fn edit_attrs_on_code_block_mirrors_lang_into_kind() {
    // Applying EditAttrs on a code block must update plugin.attrs["lang"]
    // AND mirror the new lang into BlockKind::Code.lang.
    let mut block = EditorBlock::code("rust".into(), "fn main() {}".to_string());
    // Stamp a PluginMeta manually (simulating a block loaded via from_core).
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "lang".to_string(),
        serde_json::Value::String("rust".to_string()),
    );
    block.plugin = Some(PluginMeta {
        block_type_name: Rc::from("code"),
        attrs: attrs.clone(),
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("code")),
        native: Some(Rc::from("code")),
    });
    let id = block.id;
    let mut doc = doc_with(vec![block]);

    // Apply the edit.
    let mut new_attrs = serde_json::Map::new();
    new_attrs.insert(
        "lang".to_string(),
        serde_json::Value::String("python".to_string()),
    );
    apply(
        &mut doc,
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: Box::new(new_attrs.clone()),
        },
    );

    // Verify attrs updated.
    let meta = doc.blocks[0]
        .plugin
        .as_ref()
        .expect("plugin meta must exist");
    assert_eq!(
        meta.attrs.get("lang").and_then(Value::as_str),
        Some("python")
    );

    // Verify kind.lang mirrored.
    assert!(matches!(
        &doc.blocks[0].kind,
        BlockKind::Code { lang } if &**lang == "python"
    ));

    // Verify to_core emits the new lang.
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "python" }));
}

// ============================================================================
// ChangeType round-trip tests — confirm body shape survives to_core.
// ============================================================================

#[test]
fn change_type_code_to_paragraph_round_trips() {
    let block = EditorBlock::code("rust".into(), "fn main() {}".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Paragraph,
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "paragraph");
    assert_eq!(core.blocks[0].text.as_deref(), Some("fn main() {}"));
}

#[test]
fn change_type_code_to_heading_round_trips() {
    let block = EditorBlock::code("python".into(), "print('hello')".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "heading");
    assert_eq!(core.blocks[0].text.as_deref(), Some("print('hello')"));
}

#[test]
fn change_type_code_to_list_round_trips() {
    let block = EditorBlock::code("rust".into(), "fn main() {}".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: false },
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].children.len(), 1);
    assert_eq!(
        core.blocks[0].children[0].children[0].text.as_deref(),
        Some("fn main() {}")
    );
}

#[test]
fn change_type_code_to_code_new_lang_round_trips() {
    let mut block = EditorBlock::code("rust".into(), "fn main() {}".into());
    block.plugin = Some(PluginMeta::code("rust"));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code {
                lang: "python".into(),
            },
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "code");
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "python" }));
    assert_eq!(core.blocks[0].text.as_deref(), Some("fn main() {}"));
}

#[test]
fn change_type_list_to_paragraph_round_trips() {
    let it = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("first item")],
    };
    let mut block = EditorBlock::list(false, vec![it]);
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Paragraph,
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "paragraph");
    assert_eq!(core.blocks[0].text.as_deref(), Some("first item"));
}

#[test]
fn change_type_list_to_heading_round_trips() {
    let it0 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("first")],
    };
    let it1 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("second")],
    };
    let mut block = EditorBlock::list(true, vec![it0, it1]);
    block.plugin = Some(PluginMeta::list(true));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(3),
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "heading");
    assert_eq!(core.blocks[0].text.as_deref(), Some("first\nsecond"));
}

#[test]
fn change_type_list_to_code_round_trips() {
    let it0 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("line1")],
    };
    let it1 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("line2")],
    };
    let mut block = EditorBlock::list(false, vec![it0, it1]);
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code {
                lang: "bash".into(),
            },
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "code");
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "bash" }));
    assert_eq!(core.blocks[0].text.as_deref(), Some("line1\nline2"));
}

#[test]
fn change_type_list_to_list_ordered_toggle_round_trips() {
    let it = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("item")],
    };
    let mut block = EditorBlock::list(false, vec![it]);
    block.plugin = Some(PluginMeta::list(false));
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: true },
        },
    );
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].attrs, json!({ "ordered": true }));
    assert_eq!(
        core.blocks[0].children[0].children[0].text.as_deref(),
        Some("item")
    );
}

// ============================================================================
// Coercion tests — stale body shapes are converted to match the block's kind.
// ============================================================================

#[test]
fn coerce_body_to_kind_inline_to_code_preserves_text() {
    // Regression: a stale Inline commit on a Code block should coerce to
    // Code body, preserving the text, not leave {kind: Code, body: Inline}.
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);

    // First, change the kind to Code.
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code { lang: Rc::from("") },
        },
    );
    assert!(matches!(
        &doc.blocks[0].body,
        BlockBody::Code(t) if t == "hello world"
    ));

    // Now apply a stale Inline body (the regression scenario).
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("stale")])),
            built_in: false,
        },
    );
    // Coercion should have converted the Inline to Code, preserving "stale".
    assert!(matches!(&doc.blocks[0].body, BlockBody::Code(t) if t == "stale"));
}

#[test]
fn coerce_body_to_kind_inline_to_list_preserves_text() {
    let (id, block) = paragraph_with_id("line1\nline2");
    let mut doc = doc_with(vec![block]);

    // Change to List.
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: false },
        },
    );

    // Stale Inline commit.
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("line1\nline2")])),
            built_in: false,
        },
    );
    // Should coerce to List with one item per line.
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!("expected List body");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].runs, vec![InlineRun::plain("line1")]);
    assert_eq!(items[1].runs, vec![InlineRun::plain("line2")]);
}

#[test]
fn coerce_body_to_kind_matching_body_unchanged() {
    // When the body shape already matches the kind, coercion is a no-op.
    let (id, block) = paragraph_with_id("hello");
    let mut doc = doc_with(vec![block]);

    let initial_body = doc.blocks[0].body.clone();
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(initial_body.clone()),
            built_in: false,
        },
    );
    // The body should be unchanged (canonicalization may normalize runs).
    // What matters is no panic and no silent data loss.
    assert_eq!(doc.blocks.len(), 1);
}

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun,
};

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
            new_block: EditorBlock::heading(1, vec![InlineRun::plain("Title")]),
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
            new_block: EditorBlock::paragraph(vec![InlineRun::plain("y")]),
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
                lang: "rust".into(),
            },
        },
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::Code { .. }));
    assert_eq!(run_text(&doc.blocks[0]), "hello world");
}

#[test]
fn edit_inline_replaces_runs() {
    let (id, a) = paragraph_with_id("old");
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::EditInline {
            block_id: id,
            new_runs: vec![InlineRun::plain("new")],
        },
    );
    assert_eq!(run_text(&doc.blocks[0]), "new");
}

#[test]
fn edit_code_replaces_text() {
    let block = EditorBlock::code("rust".into(), "old".into());
    let id = block.id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::EditCode {
            block_id: id,
            new_text: "new".into(),
        },
    );
    assert_eq!(run_text(&doc.blocks[0]), "new");
}

#[test]
fn split_code_block_inserts_newline() {
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
    assert_eq!(meta.block_type_name, "list");
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
    fn edit_inline_round_trip() {
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let before_body = doc.blocks[0].body.clone();
        let action = BlockAction::EditInline {
            block_id: id,
            new_runs: vec![InlineRun::plain("changed")],
        };
        let (_canonical, inverse) =
            apply(&mut doc, action).expect("EditInline must record an inverse");
        // Sanity: doc actually changed.
        assert_ne!(doc.blocks[0].body, before_body);
        // Apply the inverse; the body must match the pre-state.
        let _ = apply(&mut doc, inverse).expect("inverse must also record");
        assert_eq!(doc.blocks[0].body, before_body);
    }

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
        let new_block = EditorBlock::paragraph(vec![InlineRun::plain("inserted")]);
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
    fn edit_code_round_trip() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        // Force a Code body for the test.
        block.body = BlockBody::Code("fn main() {}".to_string());
        block.kind = BlockKind::Code {
            lang: String::new(),
        };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditCode {
                block_id: id,
                new_text: "fn main() { println!(\"hi\"); }".to_string(),
            },
        );
    }

    #[test]
    fn edit_list_item_round_trip() {
        let it0 = lopress_editor::model::types::ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("old")],
        };
        let item_id = it0.id;
        let list = EditorBlock::list(false, vec![it0]);
        let block_id = list.id;
        let mut doc = doc_with(vec![list]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditListItem {
                block_id,
                item_id,
                new_runs: vec![InlineRun::plain("new")],
            },
        );
    }

    #[test]
    fn merge_list_item_with_prev_round_trip() {
        let it0 = lopress_editor::model::types::ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("foo")],
        };
        let it1 = lopress_editor::model::types::ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("bar")],
        };
        let cur_id = it1.id;
        let list = EditorBlock::list(false, vec![it0, it1]);
        let block_id = list.id;
        let mut doc = doc_with(vec![list]);
        assert_round_trip(
            &mut doc,
            BlockAction::MergeListItemWithPrev {
                block_id,
                item_id: cur_id,
            },
        );
    }

    #[test]
    fn edit_block_body_inline_round_trip() {
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let new_body = BlockBody::Inline(vec![InlineRun::plain("entirely different content")]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
            },
        );
    }

    #[test]
    fn edit_block_body_code_round_trip() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("fn main() {}".to_string());
        block.kind = BlockKind::Code {
            lang: String::new(),
        };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        let new_body = BlockBody::Code("fn other() { /* ... */ }".to_string());
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
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
        let new_body = BlockBody::List(vec![
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
        ]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
            },
        );
    }

    #[test]
    fn split_code_block_is_now_recordable() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("foobar".to_string());
        block.kind = BlockKind::Code {
            lang: String::new(),
        };
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

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

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

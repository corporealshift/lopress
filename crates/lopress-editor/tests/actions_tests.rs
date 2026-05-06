#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun,
};
use lopress_editor::selection::{DocPosition, DocSelection};
use lopress_editor::ui::blocks::inline_editor::InlineFlag;

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
            run: 0,
            offset: 5,
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
            run: 0,
            offset: 2,
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
            run: 0,
            offset: 5,
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
            run: 0,
            offset: 1,
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
            run: 0,
            offset: 8,
        },
    );
    // Code block does not split; a newline is inserted at offset.
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "fn main(\n) {}");
}

// ── Multi-block actions ──────────────────────────────────────────────────────

fn heading_with_id(level: u8, text: &str) -> (BlockId, EditorBlock) {
    let b = EditorBlock::heading(level, vec![InlineRun::plain(text)]);
    let id = b.id;
    (id, b)
}

#[test]
fn delete_range_within_single_block() {
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::DeleteRange {
            selection: DocSelection {
                anchor: DocPosition::new(id, 0, 5),
                head: DocPosition::new(id, 0, 11),
            },
        },
    );
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "hello");
}

#[test]
fn delete_range_across_three_blocks_merges_into_leading_kind() {
    let (id_a, a) = heading_with_id(1, "Hello");
    let (_id_b, b) = paragraph_with_id("middle");
    let (id_c, c) = paragraph_with_id("rest");
    let mut doc = doc_with(vec![a, b, c]);
    apply(
        &mut doc,
        BlockAction::DeleteRange {
            selection: DocSelection {
                anchor: DocPosition::new(id_a, 0, 3),
                head: DocPosition::new(id_c, 0, 4),
            },
        },
    );
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading(1)));
    assert_eq!(run_text(&doc.blocks[0]), "Hel");
}

#[test]
fn delete_range_keeps_trailing_chars_after_endpoint() {
    let (id_a, a) = paragraph_with_id("alpha");
    let (id_b, b) = paragraph_with_id("beta");
    let mut doc = doc_with(vec![a, b]);
    apply(
        &mut doc,
        BlockAction::DeleteRange {
            selection: DocSelection {
                anchor: DocPosition::new(id_a, 0, 2),
                head: DocPosition::new(id_b, 0, 1),
            },
        },
    );
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(run_text(&doc.blocks[0]), "aleta");
}

#[test]
fn toggle_inline_range_across_blocks_sets_when_mixed() {
    let (id_a, a) = paragraph_with_id("alpha");
    let (id_b, b) = paragraph_with_id("beta");
    let mut doc = doc_with(vec![a, b]);
    apply(
        &mut doc,
        BlockAction::ToggleInlineRange {
            selection: DocSelection {
                anchor: DocPosition::new(id_a, 0, 0),
                head: DocPosition::new(id_b, 0, 4),
            },
            flag: InlineFlag::Bold,
        },
    );
    if let BlockBody::Inline(runs) = &doc.blocks[0].body {
        assert!(runs.iter().all(|r| r.bold));
    }
    if let BlockBody::Inline(runs) = &doc.blocks[1].body {
        assert!(runs.iter().all(|r| r.bold));
    }
}

#[test]
fn toggle_inline_range_clears_when_all_set() {
    let mut a = EditorBlock::paragraph(vec![InlineRun {
        text: "alpha".into(),
        bold: true,
        ..Default::default()
    }]);
    let id_a = a.id;
    let mut b = EditorBlock::paragraph(vec![InlineRun {
        text: "beta".into(),
        bold: true,
        ..Default::default()
    }]);
    let id_b = b.id;
    a.id = id_a;
    b.id = id_b;
    let mut doc = doc_with(vec![a, b]);
    apply(
        &mut doc,
        BlockAction::ToggleInlineRange {
            selection: DocSelection {
                anchor: DocPosition::new(id_a, 0, 0),
                head: DocPosition::new(id_b, 0, 4),
            },
            flag: InlineFlag::Bold,
        },
    );
    if let BlockBody::Inline(runs) = &doc.blocks[0].body {
        assert!(runs.iter().all(|r| !r.bold));
    }
    if let BlockBody::Inline(runs) = &doc.blocks[1].body {
        assert!(runs.iter().all(|r| !r.bold));
    }
}

#[test]
fn paste_blocks_into_middle_of_inline_block_splits_then_inserts() {
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);
    let pasted = vec![
        EditorBlock::heading(2, vec![InlineRun::plain("Inserted")]),
        EditorBlock::paragraph(vec![InlineRun::plain("body")]),
    ];
    apply(
        &mut doc,
        BlockAction::PasteBlocks {
            at: DocPosition::new(id, 0, 5),
            blocks: pasted,
        },
    );
    assert_eq!(doc.blocks.len(), 4);
    assert_eq!(run_text(&doc.blocks[0]), "hello");
    assert!(matches!(doc.blocks[1].kind, BlockKind::Heading(2)));
    assert_eq!(run_text(&doc.blocks[1]), "Inserted");
    assert_eq!(run_text(&doc.blocks[2]), "body");
    assert_eq!(run_text(&doc.blocks[3]), " world");
}

#[test]
fn paste_blocks_at_end_of_inline_appends() {
    let (id, block) = paragraph_with_id("hello");
    let mut doc = doc_with(vec![block]);
    let pasted = vec![EditorBlock::paragraph(vec![InlineRun::plain("then")])];
    apply(
        &mut doc,
        BlockAction::PasteBlocks {
            at: DocPosition::new(id, 0, 5),
            blocks: pasted,
        },
    );
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(run_text(&doc.blocks[0]), "hello");
    // Split at end produces an empty trailing paragraph; pasted goes between.
    assert_eq!(run_text(&doc.blocks[1]), "then");
    assert_eq!(run_text(&doc.blocks[2]), "");
}

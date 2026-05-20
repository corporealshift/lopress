#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::types::{BlockKind, EditorBlock, EditorDoc, InlineRun};

fn doc_with(blocks: Vec<EditorBlock>) -> EditorDoc {
    EditorDoc {
        blocks,
        front_matter: lopress_core::FrontMatter::default(),
    }
}

fn para(text: &str) -> EditorBlock {
    EditorBlock::paragraph(vec![InlineRun::plain(text)])
}

/// Apply an action to a clone of `doc` and return the inverse from the
/// (canonical, inverse) pair. Used by inverse-shape tests that want to
/// examine the inverse without permanently mutating the test's doc.
fn inverse_of(doc: &EditorDoc, action: BlockAction) -> BlockAction {
    let mut clone = doc.clone();
    let (_canonical, inverse) = apply(&mut clone, action).unwrap();
    inverse
}

#[test]
fn inverse_of_edit_inline_is_old_runs() {
    let old = para("before");
    let id = old.id;
    let doc = doc_with(vec![old]);
    let inv = inverse_of(
        &doc,
        BlockAction::EditInline {
            block_id: id,
            new_runs: vec![InlineRun::plain("after")],
        },
    );
    match inv {
        BlockAction::EditInline { block_id, new_runs } => {
            assert_eq!(block_id, id);
            assert_eq!(new_runs, vec![InlineRun::plain("before")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_merge_with_prev_is_split_at_join_point() {
    let a = para("hello ");
    let b = para("world");
    let prev_id = a.id;
    let cur_id = b.id;
    let doc = doc_with(vec![a, b]);
    // "hello " is 6 bytes
    let inv = inverse_of(&doc, BlockAction::MergeWithPrev { block_id: cur_id });
    match inv {
        BlockAction::Split {
            block_id,
            byte_offset,
            new_block_id,
        } => {
            assert_eq!(block_id, prev_id);
            assert_eq!(byte_offset, 6);
            // The Split inverse of an inline-into-inline merge carries the
            // merged-away block's id so undo→redo is id-stable.
            assert_eq!(new_block_id, Some(cur_id));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_change_type_is_change_type_with_old_kind() {
    let b = para("text");
    let id = b.id;
    let doc = doc_with(vec![b]);
    let inv = inverse_of(
        &doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    match inv {
        BlockAction::ChangeType { block_id, new_kind } => {
            assert_eq!(block_id, id);
            assert_eq!(new_kind, BlockKind::Paragraph);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_delete_is_insert_after_with_predecessor() {
    let a = para("anchor");
    let b = para("victim");
    let anchor_id = a.id;
    let victim_id = b.id;
    let doc = doc_with(vec![a, b]);
    let inv = inverse_of(
        &doc,
        BlockAction::Delete {
            block_id: victim_id,
        },
    );
    match inv {
        BlockAction::InsertAfter { anchor, new_block } => {
            assert_eq!(anchor, anchor_id);
            assert_eq!(new_block.id, victim_id);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_insert_after_is_delete_new_block() {
    let a = para("anchor");
    let new_b = para("inserted");
    let new_id = new_b.id;
    let anchor_id = a.id;
    let doc = doc_with(vec![a]);
    let inv = inverse_of(
        &doc,
        BlockAction::InsertAfter {
            anchor: anchor_id,
            new_block: new_b,
        },
    );
    match inv {
        BlockAction::Delete { block_id } => assert_eq!(block_id, new_id),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn undo_stack_push_and_pop() {
    use lopress_editor::undo::UndoStack;
    let a = para("text");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("edited")],
    };
    let (canonical, inverse) = apply(&mut doc, action).unwrap();
    stack.push_after_apply(canonical, inverse);

    let undo_action = stack.pop_undo().unwrap();
    match undo_action {
        BlockAction::EditInline { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("text")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn undo_stack_redo_available_after_undo() {
    use lopress_editor::undo::UndoStack;
    let a = para("original");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("edited")],
    };
    let (canonical, inverse) = apply(&mut doc, action).unwrap();
    stack.push_after_apply(canonical, inverse);

    stack.pop_undo().unwrap();
    let redo_action = stack.pop_redo().unwrap();
    match redo_action {
        BlockAction::EditInline { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("edited")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn edit_inline_within_one_second_coalesces() {
    use lopress_editor::undo::UndoStack;
    let a = para("a");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let a1 = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("ab")],
    };
    let (c1, i1) = apply(&mut doc, a1).unwrap();
    stack.push_after_apply(c1, i1);

    let a2 = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("abc")],
    };
    let (c2, i2) = apply(&mut doc, a2).unwrap();
    stack.push_after_apply(c2, i2);

    // Should have only ONE undo entry (coalesced); the inverse keeps the
    // oldest old_runs ("a") so a single undo restores all the way back.
    assert_eq!(stack.undo_depth(), 1);
    let undo = stack.pop_undo().unwrap();
    match undo {
        BlockAction::EditInline { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("a")]);
        }
        _ => panic!("wrong variant"),
    }
}

use lopress_editor::model::types::{BlockId, ListItem};

fn list_item(text: &str) -> ListItem {
    ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain(text)],
    }
}

#[test]
fn inverse_of_edit_list_item_restores_old_runs() {
    let it0 = list_item("old");
    let item_id = it0.id;
    let list = EditorBlock::list(false, vec![it0]);
    let block_id = list.id;
    let doc = doc_with(vec![list]);
    let inv = inverse_of(
        &doc,
        BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs: vec![InlineRun::plain("new")],
        },
    );
    match inv {
        BlockAction::EditListItem { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("old")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_merge_list_item_is_split_at_join_point() {
    let it0 = list_item("foo");
    let it1 = list_item("bar");
    let prev_id = it0.id;
    let cur_id = it1.id;
    let list = EditorBlock::list(false, vec![it0, it1]);
    let block_id = list.id;
    let doc = doc_with(vec![list]);
    let inv = inverse_of(
        &doc,
        BlockAction::MergeListItemWithPrev {
            block_id,
            item_id: cur_id,
        },
    );
    match inv {
        BlockAction::SplitListItem {
            item_id,
            byte_offset,
            new_block_id,
            ..
        } => {
            assert_eq!(item_id, prev_id);
            assert_eq!(byte_offset, 3);
            assert_eq!(new_block_id, Some(cur_id));
        }
        _ => panic!("wrong variant"),
    }
}

/// After Task 4's wiring, undo↔redo of a Split is id-stable across
/// arbitrarily many cycles because the canonical action carries
/// `new_block_id: Some(...)`. No post-apply patching needed.
#[test]
fn split_undo_redo_round_trip_preserves_block_id() {
    use lopress_editor::undo::UndoStack;
    let a = para("hello world");
    let a_id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    // Apply Split.
    let action = BlockAction::Split {
        block_id: a_id,
        byte_offset: 5,
        new_block_id: None,
    };
    let (canonical, inverse) = apply(&mut doc, action).unwrap();
    stack.push_after_apply(canonical, inverse);
    let original_new_id = doc.blocks[1].id;

    // Undo.
    let undo_action = stack.pop_undo().unwrap();
    let _ = apply(&mut doc, undo_action).unwrap();
    assert_eq!(doc.blocks.len(), 1);

    // Redo — same id reused.
    let redo_action = stack.pop_redo().unwrap();
    let _ = apply(&mut doc, redo_action).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(
        doc.blocks[1].id, original_new_id,
        "redo must preserve the original new_block_id"
    );

    // Undo again.
    let undo_action_2 = stack.pop_undo().unwrap();
    let _ = apply(&mut doc, undo_action_2).unwrap();
    assert_eq!(doc.blocks.len(), 1);

    // Redo again — id still stable.
    let redo_action_2 = stack.pop_redo().unwrap();
    let _ = apply(&mut doc, redo_action_2).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(
        doc.blocks[1].id, original_new_id,
        "second redo must also preserve the id"
    );
}

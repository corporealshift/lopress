#![allow(clippy::unwrap_used)]

use lopress_editor::actions::BlockAction;
use lopress_editor::model::types::{
    BlockKind, EditorBlock, EditorDoc, InlineRun,
};
use lopress_editor::undo::compute_inverse;

fn doc_with(blocks: Vec<EditorBlock>) -> EditorDoc {
    EditorDoc { blocks, front_matter: lopress_core::FrontMatter::default() }
}

fn para(text: &str) -> EditorBlock {
    EditorBlock::paragraph(vec![InlineRun::plain(text)])
}

#[test]
fn inverse_of_edit_inline_is_old_runs() {
    let old = para("before");
    let id = old.id;
    let doc = doc_with(vec![old]);
    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("after")],
    };
    let inv = compute_inverse(&doc, &action).unwrap();
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
    let inv = compute_inverse(&doc, &BlockAction::MergeWithPrev { block_id: cur_id }).unwrap();
    match inv {
        BlockAction::Split { block_id, byte_offset } => {
            assert_eq!(block_id, prev_id);
            assert_eq!(byte_offset, 6);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_change_type_is_change_type_with_old_kind() {
    let b = para("text");
    let id = b.id;
    let doc = doc_with(vec![b]);
    let inv = compute_inverse(
        &doc,
        &BlockAction::ChangeType { block_id: id, new_kind: BlockKind::Heading(2) },
    )
    .unwrap();
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
    let inv = compute_inverse(&doc, &BlockAction::Delete { block_id: victim_id }).unwrap();
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
    let inv = compute_inverse(
        &doc,
        &BlockAction::InsertAfter { anchor: anchor_id, new_block: new_b },
    )
    .unwrap();
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
    stack.push_before_apply(&doc, &action);
    lopress_editor::actions::apply(&mut doc, action);

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
    stack.push_before_apply(&doc, &action.clone());
    lopress_editor::actions::apply(&mut doc, action);

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

    let a1 = BlockAction::EditInline { block_id: id, new_runs: vec![InlineRun::plain("ab")] };
    stack.push_before_apply(&doc, &a1);
    lopress_editor::actions::apply(&mut doc, a1);

    let a2 = BlockAction::EditInline { block_id: id, new_runs: vec![InlineRun::plain("abc")] };
    stack.push_before_apply(&doc, &a2);
    lopress_editor::actions::apply(&mut doc, a2);

    // Should have only ONE undo entry (coalesced)
    assert_eq!(stack.undo_depth(), 1);
    let undo = stack.pop_undo().unwrap();
    match undo {
        BlockAction::EditInline { new_runs, .. } => {
            // Restores to original "a", not to intermediate "ab"
            assert_eq!(new_runs, vec![InlineRun::plain("a")]);
        }
        _ => panic!("wrong variant"),
    }
}

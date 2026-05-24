//! Focus resolution helpers for the editing view.
//!
//! `focus_block_for` derives the block to focus *before* an action is
//! applied (the target block). `focus_after_apply` resolves the block to
//! focus *after* the action is applied (the surviving block — in most
//! cases the same as the pre-focus, but `MergeWithPrev` deletes its
//! target and focus must land on the predecessor).
//!
//! `defer_focus` schedules a focus update on the next event-loop tick
//! rather than immediately, avoiding Floem's "set focus while already
//! processing focus" race.

use crate::actions::BlockAction;
use crate::model::types::{BlockId, EditorDoc};
use floem::reactive::{RwSignal, SignalUpdate};
use std::time::Duration;

/// The block a just-applied undo/redo action should restore focus to.
pub fn focus_block_for(action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
        BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
        BlockAction::Delete { .. } | BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditBlockBody { block_id, .. } => Some(*block_id),
    }
}

/// Which block should hold focus after `action` is applied to `doc`
/// (`doc` is the state *before* the apply). Most actions keep their target
/// block alive, so `focus_block_for` suffices — but `MergeWithPrev` deletes
/// its target (folds it into the predecessor), so focus must land on the
/// surviving predecessor, looked up here while the target still exists.
pub fn focus_after_apply(doc: Option<&EditorDoc>, action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::MergeWithPrev { block_id } => {
            let d = doc?;
            let i = d.blocks.iter().position(|b| b.id == *block_id)?;
            i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
        }
        _ => focus_block_for(action),
    }
}

/// Set `focus_target` on the next event-loop tick rather than immediately.
///
/// Used in the action sink, undo/redo builders, and the list/code widgets
/// to avoid Floem's "set focus while already processing focus" race.
pub fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
    floem::action::exec_after(Duration::from_millis(0), move |_| {
        focus_target.set(Some(target_id));
    });
}

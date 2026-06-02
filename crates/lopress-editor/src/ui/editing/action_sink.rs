//! Action sink: the chokepoint for all block-tree mutations.
//!
//! Every `BlockAction` routes through the closure returned by
//! `build_action_sink`. It handles the slash menu toggle, pre/post focus
//! computation, dispatches to `apply`, pushes undo/redo entries, and
//! triggers the dirty flag.

use crate::actions::{apply, BlockAction};
use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::inline_editor::ActionSink;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use std::rc::Rc;
use std::time::Duration;

/// Build the `on_action` closure that every block-tree mutation routes through.
///
/// Parameters:
/// - `current_doc`: the reactive document model.
/// - `focus_target`: signal set by focus resolution after each action.
/// - `slash_menu_open`: signal tracking the open slash-menu block id.
/// - `undo_stack`: the undo/redo stack.
/// - `mark_dirty`: callback to mark the document dirty (triggers save debounce).
pub fn build_action_sink(
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    undo_stack: RwSignal<crate::undo::UndoStack>,
    mark_dirty: Rc<dyn Fn()>,
) -> ActionSink {
    let on_action_mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move |action: BlockAction| {
        let _t = lopress_core::perf::span("editor.on_action");
        if let BlockAction::OpenSlashMenu { block_id } = action {
            slash_menu_open.set(Some(block_id));
            return;
        }

        // Pre-focus must read pre-apply state (the block before the one
        // being merged into its predecessor). Capture it before the apply
        // mutates the doc.
        let pre_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::MergeWithPrev { block_id }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .filter(|&i| i > 0)
                .and_then(|i| d.blocks.get(i - 1))
                .map(|b| b.id),
            _ => None,
        });

        // Apply the action; capture the returned (canonical, inverse) pair
        // and push it onto the undo stack. apply returns None for
        // unrecordable cases (UI-only, no-op, or stage-1-unrecordable
        // structural splits / first-block delete).
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if recorded.is_some() {
            on_action_mark_dirty();
            // Dismiss the slash menu only when an action actually mutates the
            // document. A no-op action — e.g. the empty-buffer `EditBlockBody`
            // commit emitted when the slash popup grabs focus from the
            // just-opened block — must NOT close the menu, or the menu opens
            // and instantly closes (it never appears to the user).
            if slash_menu_open.get_untracked().is_some() {
                slash_menu_open.set(None);
            }
        }
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
        }

        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            _ => None,
        });
        let change_type_focus = match &action {
            BlockAction::ChangeType { block_id, .. } => Some(*block_id),
            _ => None,
        };
        // A freshly inserted block (e.g. the empty-document "add block"
        // button) should take focus so the caret lands in it immediately.
        let insert_focus = match &action {
            BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
            _ => None,
        };
        if let Some(id) = pre_focus
            .or(post_focus)
            .or(change_type_focus)
            .or(insert_focus)
        {
            floem::action::exec_after(Duration::from_millis(0), move |_| {
                focus_target.set(Some(id));
            });
        }
    })
}

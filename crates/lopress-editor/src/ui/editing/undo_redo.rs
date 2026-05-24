//! Undo/redo builders for the editing view.
//!
//! Each builder takes the signals it needs and returns an `Rc<dyn Fn()>`
/// closure. The closures share the same focus-computation pattern: pop
/// from the stack, resolve the focus target from the pre-apply doc, apply
/// the inverse action, and mark dirty.

use crate::actions::apply;
use crate::model::types::{BlockId, EditorDoc};
use crate::ui::editing::focus::focus_after_apply;
use floem::reactive::{RwSignal, SignalUpdate, SignalWith};
use std::rc::Rc;
use std::time::Duration;

/// Build the `on_undo` closure.
///
/// Pops an entry from the undo stack, resolves the focus target from the
/// pre-apply doc (MergeWithPrev deletes its target so focus must land on
/// the predecessor), applies the inverse action, and marks dirty.
pub fn build_undo(
    undo_stack: RwSignal<crate::undo::UndoStack>,
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    mark_dirty: Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let mut popped = None;
        undo_stack.update(|s| {
            popped = s.pop_undo();
        });
        if let Some(action) = popped {
            // Compute focus from the pre-apply doc — MergeWithPrev
            // deletes its target, so focus must resolve to the
            // surviving predecessor before the apply runs.
            let focus_id =
                current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
            let action_for_apply = action.clone();
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    let _ = apply(d, action_for_apply);
                }
            });
            // No post-apply id surgery: Split / SplitListItem in stored
            // entries carry new_block_id: Some(...), so re-applying them
            // is id-stable without patching the redo entry.
            if let Some(id) = focus_id {
                floem::action::exec_after(Duration::from_millis(0), move |_| {
                    focus_target.set(Some(id));
                });
            }
            mark_dirty();
        }
    })
}

/// Build the `on_redo` closure.
///
/// Same pattern as `build_undo`: pop from the redo stack, resolve focus,
/// apply the canonical action, mark dirty.
pub fn build_redo(
    undo_stack: RwSignal<crate::undo::UndoStack>,
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    mark_dirty: Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let mut popped = None;
        undo_stack.update(|s| {
            popped = s.pop_redo();
        });
        if let Some(action) = popped {
            let focus_id =
                current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
            let action_for_apply = action.clone();
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    let _ = apply(d, action_for_apply);
                }
            });
            // No post-apply id surgery for the same reason as on_undo.
            if let Some(id) = focus_id {
                floem::action::exec_after(Duration::from_millis(0), move |_| {
                    focus_target.set(Some(id));
                });
            }
            mark_dirty();
        }
    })
}

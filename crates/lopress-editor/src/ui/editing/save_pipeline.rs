//! Save pipeline: debounce signals, dirty tracking, status polling, and
//! the debounced save+rebuild closure.
//!
//! `SavePipeline` is a plain bag of signals (no methods that hold state).
//! `start_save_pipeline` bundles the signal creation, starts the debounce
//! timer, and kicks off the build/serve status polls.

use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::blocks::inline_editor::ActiveCommitSlot;
use floem::action::debounce_action;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use lopress_gui_host::{BuildStatus, ServeStatus};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Bag of save-pipeline signals exposed to `editing_view` for the footer
/// and the debounced save closure.
pub struct SavePipeline {
    pub mark_dirty: Rc<dyn Fn()>,
    pub dirty_sig: RwSignal<bool>,
    pub save_error_sig: RwSignal<Option<String>>,
    pub build_status_sig: RwSignal<BuildStatus>,
    pub serve_status_sig: RwSignal<ServeStatus>,
}

/// Create the save-pipeline signals, start the debounce timer, and kick off
/// the build/serve status polls.
///
/// Returns a `SavePipeline` that `editing_view` passes to the footer and
/// uses for the `on_action` mark_dirty callback.
pub fn start_save_pipeline(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> SavePipeline {
    // ── Save-debounce signals ────────────────────────────────────────
    // `dirty_counter` bumps on every legitimate edit; `debounce_action`
    // watches it and runs the save closure 500 ms after the last bump.
    // `dirty_sig` / `save_error_sig` drive the footer's status display.
    let build_status_sig: RwSignal<BuildStatus> = RwSignal::new(BuildStatus::Idle);
    let dirty_sig: RwSignal<bool> = RwSignal::new(false);
    let save_error_sig: RwSignal<Option<String>> = RwSignal::new(None);
    let dirty_counter: RwSignal<u64> = RwSignal::new(0);

    let mark_dirty: Rc<dyn Fn()> = Rc::new(move || {
        dirty_sig.set(true);
        dirty_counter.update(|n| *n = n.wrapping_add(1));
    });

    // Status polls — read session status and update the signals.
    {
        let editing_for_poll = Rc::clone(&editing);
        let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.build_status())
                .unwrap_or(BuildStatus::Idle)
        });
        crate::ui::footer::start_build_status_poll(session_reader, build_status_sig);
    }

    let serve_status_sig: RwSignal<ServeStatus> = RwSignal::new(ServeStatus::Starting);

    {
        let editing_for_poll = Rc::clone(&editing);
        let serve_reader: Rc<dyn Fn() -> ServeStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.serve_status())
                .unwrap_or(ServeStatus::Starting)
        });
        crate::ui::footer::start_serve_status_poll(serve_reader, serve_status_sig);
    }

    // Debounced save+rebuild. `debounce_action` resets its internal timer on
    // every counter bump and fires the closure 500 ms after the last bump.
    {
        let editing_for_save = Rc::clone(&editing);
        let dc = dirty_counter;
        let ds = dirty_sig;
        let ses = save_error_sig;
        debounce_action(dc, Duration::from_millis(500), move || {
            let result = current_doc.with_untracked(|d| {
                let doc = d.as_ref()?;
                let guard = editing_for_save.borrow();
                let state = guard.as_ref()?;
                Some(state.save_doc(doc))
            });
            let result = match result {
                Some(r) => r,
                None => return,
            };
            match result {
                Ok(()) => {
                    ds.set(false);
                    ses.set(None);
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
                Err(msg) => {
                    ses.set(Some(msg));
                }
            }
        });
    }

    SavePipeline {
        mark_dirty,
        dirty_sig,
        save_error_sig,
        build_status_sig,
        serve_status_sig,
    }
}

/// Copyable bundle of the signals a doc-switch flush needs. Built once in
/// `editing_view` from the save pipeline + the pane-stable commit slot, and
/// captured by every closure that replaces `current_doc` (sidebar open,
/// "+ New post/page").
#[derive(Clone, Copy)]
pub struct FlushSignals {
    pub active_commit: ActiveCommitSlot,
    pub dirty_sig: RwSignal<bool>,
    pub save_error_sig: RwSignal<Option<String>>,
}

/// Flush pending edits before a doc-switch path replaces `current_doc`.
///
/// Two loss windows close here. First, the focused editor's
/// typed-but-uncommitted buffer: sidebar rows aren't focusable, so clicking
/// one never blurs the editor and the FocusLost commit never fires — run the
/// registered `active_commit` instead. Second, committed-but-unsaved edits:
/// the debounced save reads `current_doc` at fire time, which after a switch
/// is already the *new* doc, so anything dirty must be saved synchronously
/// now.
///
/// Returns `false` when a dirty document could not be saved — callers must
/// abort the switch, otherwise the unsaved edits are discarded. The error is
/// left in `save_error_sig` for the footer to display.
pub fn flush_pending_edits(
    signals: FlushSignals,
    editing: &Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> bool {
    let FlushSignals {
        active_commit,
        dirty_sig,
        save_error_sig,
    } = signals;
    if let Some(commit) = active_commit.get_untracked() {
        // Marks the doc dirty via the action sink when the buffer differs
        // from the model; a no-change commit is dropped by `apply`.
        commit();
    }
    if !dirty_sig.get_untracked() {
        return true;
    }
    let result = current_doc.with_untracked(|d| {
        let doc = d.as_ref()?;
        let guard = editing.borrow();
        let state = guard.as_ref()?;
        Some(state.save_doc(doc))
    });
    match result {
        // No doc open (or no session): nothing to lose.
        None => true,
        Some(Ok(())) => {
            dirty_sig.set(false);
            save_error_sig.set(None);
            if let Some(state) = editing.borrow().as_ref() {
                state.session.rebuild();
            }
            true
        }
        Some(Err(msg)) => {
            save_error_sig.set(Some(msg));
            false
        }
    }
}

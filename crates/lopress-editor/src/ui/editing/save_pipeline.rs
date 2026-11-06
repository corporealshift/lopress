//! Save pipeline: debounce signals, dirty tracking, status polling, and
//! the debounced save+rebuild closure.
//!
//! `SavePipeline` is a plain bag of signals (no methods that hold state).
//! `start_save_pipeline` bundles the signal creation, starts the debounce
//! timer, and kicks off the build/serve status polls.

use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::blocks::inline_editor::{ActiveCommitSlot, SelectionRestore, SelectionRestoreSlot};
use crate::ui::confirm_dialog::{ConfirmChoice, ConfirmRequest, ConfirmSlot};
use crate::ui::editing::notes::{reload_notes, save_notes, NotesState};
use crate::ui::editing::save_rules::{
    decide_refresh, decide_save, decide_switch, RefreshStep, SaveStep, SwitchStep,
};
use crate::undo::UndoStack;
use floem::action::debounce_action;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use lopress_gui_host::{BuildStatus, DiskStamp, DiskState, ServeStatus, WorkspaceSummary};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Save status of the open document. Created in `root_view` because the debug
/// `/state` and `/open` wiring at root scope needs it.
#[derive(Clone, Copy)]
pub struct DocStatus {
    /// The post has edits that aren't on disk. Notes track their own.
    pub dirty: RwSignal<bool>,
    pub disk: RwSignal<DiskState>,
    /// Why the last refresh failed. Cleared once the files match again.
    pub refresh_error: RwSignal<Option<String>>,
}

impl DocStatus {
    pub fn new() -> Self {
        Self {
            dirty: RwSignal::new(false),
            disk: RwSignal::new(DiskState::Unchanged),
            refresh_error: RwSignal::new(None),
        }
    }
}

impl Default for DocStatus {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset everything tied to the previous document. Every path that loads a
/// document calls this, with no `editing` borrow held.
pub fn after_doc_loaded(
    status: DocStatus,
    notes: NotesState,
    undo_stack: RwSignal<UndoStack>,
    editing: &Rc<RefCell<Option<EditingState>>>,
) {
    status.dirty.set(false);
    status.disk.set(DiskState::Unchanged);
    status.refresh_error.set(None);
    undo_stack.update(|s| *s = UndoStack::new());
    reload_notes(notes, editing);
}

/// What a save request did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    NothingToSave,
    /// The file changed on disk, so nothing was written.
    NeedsOverwrite,
    /// The error is in `save_error_sig` or the notes' `save_error`.
    Failed,
    NoDocument,
}

/// Runs a save. The selection is where to put the caret back after the commit
/// rebuilds the block column; only a block editor can supply one.
pub type SaveSink = Rc<dyn Fn(Option<SelectionRestore>) -> SaveOutcome>;

/// Bag of save-pipeline signals exposed to `editing_view` for the footer
/// and the debounced save closure.
pub struct SavePipeline {
    pub mark_dirty: Rc<dyn Fn()>,
    /// Bumped by every change to the document.
    pub edit_rev: RwSignal<u64>,
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
    current_path: RwSignal<Option<PathBuf>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
    notes: NotesState,
    status: DocStatus,
) -> SavePipeline {
    // ── Save-debounce signals ────────────────────────────────────────
    // `dirty_counter` bumps on every legitimate edit; `debounce_action`
    // watches it and runs the save closure 500 ms after the last bump.
    // `status.dirty` / `save_error_sig` drive the footer's status display.
    let build_status_sig: RwSignal<BuildStatus> = RwSignal::new(BuildStatus::Idle);
    let dirty_sig = status.dirty;
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

    start_disk_poll(Rc::clone(&editing), status);

    // Debounced save+rebuild. `debounce_action` resets its internal timer on
    // every counter bump and fires the closure 500 ms after the last bump.
    {
        let editing_for_save = Rc::clone(&editing);
        let dc = dirty_counter;
        let ds = dirty_sig;
        let ses = save_error_sig;
        let cp = current_path;
        let ws = workspace_signal;
        debounce_action(dc, Duration::from_millis(500), move || {
            let result = current_doc.with_untracked(|d| {
                let doc = d.as_ref()?;
                let mut guard = editing_for_save.borrow_mut();
                let state = guard.as_mut()?;
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
                    crate::ui::editing::filename_sync::sync_filename_and_update(
                        &editing_for_save,
                        current_doc,
                        cp,
                        ws,
                        notes,
                    );
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

    // Notes save on their own timer, with no rebuild: they don't affect the site.
    {
        let editing_for_notes = Rc::clone(&editing);
        debounce_action(notes.edits, Duration::from_millis(500), move || {
            save_notes(notes, &editing_for_notes);
        });
    }

    SavePipeline {
        mark_dirty,
        edit_rev: dirty_counter,
        save_error_sig,
        build_status_sig,
        serve_status_sig,
    }
}

/// Watch the open files on a 250 ms timer. A timer rather than the watcher,
/// which ignores `notes/`. Stats every tick and reads the files only when a
/// stamp moves. A failed read keeps the old stamp, so the next tick retries.
fn start_disk_poll(editing: Rc<RefCell<Option<EditingState>>>, status: DocStatus) {
    fn tick(
        editing: Rc<RefCell<Option<EditingState>>>,
        status: DocStatus,
        last: Option<DiskStamp>,
    ) {
        floem::action::exec_after(Duration::from_millis(250), move |_| {
            let last = check_disk(&editing, status, last);
            tick(editing, status, last);
        });
    }
    tick(editing, status, None);
}

/// One poll tick. Returns the stamp to compare against next time.
fn check_disk(
    editing: &Rc<RefCell<Option<EditingState>>>,
    status: DocStatus,
    last: Option<DiskStamp>,
) -> Option<DiskStamp> {
    let guard = editing.borrow();
    let Some(state) = guard.as_ref() else {
        return last;
    };
    let stamp = state.disk_stamp();
    if stamp == last {
        return last;
    }
    match state.disk_state() {
        Some(Ok(disk)) => {
            if disk != status.disk.get_untracked() {
                status.disk.set(disk);
            }
            if disk == DiskState::Unchanged {
                status.refresh_error.set(None);
            }
            stamp
        }
        Some(Err(_)) | None => last,
    }
}

/// Everything a save needs. Built once in `editing_view` and captured by every
/// closure that saves or replaces `current_doc`.
#[derive(Clone)]
pub struct SaveCtx {
    pub editing: Rc<RefCell<Option<EditingState>>>,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub current_path: RwSignal<Option<PathBuf>>,
    pub workspace_signal: RwSignal<WorkspaceSummary>,
    pub active_commit: ActiveCommitSlot,
    pub status: DocStatus,
    pub save_error_sig: RwSignal<Option<String>>,
    pub notes: NotesState,
    pub undo_stack: RwSignal<UndoStack>,
    pub edit_rev: RwSignal<u64>,
    pub restore_selection: SelectionRestoreSlot,
    pub confirm: ConfirmSlot,
}

impl SaveCtx {
    /// The post or its notes have edits that aren't on disk.
    fn unsaved(&self) -> bool {
        self.status.dirty.get_untracked() || self.notes.dirty.get_untracked()
    }

    /// Reads the files rather than trusting the poll, which can lag a save by
    /// up to one tick. A failed read is left for the write to report.
    fn disk_now(&self) -> DiskState {
        match self
            .editing
            .borrow()
            .as_ref()
            .and_then(EditingState::disk_state)
        {
            Some(Ok(state)) => state,
            Some(Err(_)) | None => DiskState::Unchanged,
        }
    }
}

/// Move a focused editor's buffer into the document. Typing records nothing
/// until a commit, so every save starts here.
fn commit_focused(ctx: &SaveCtx, restore: Option<SelectionRestore>) {
    let Some(commit) = ctx.active_commit.get_untracked() else {
        return;
    };
    let before = ctx.edit_rev.get_untracked();
    if restore.is_some() {
        ctx.restore_selection.set(restore);
    }
    // Marks the doc dirty via the action sink when the buffer differs from
    // the model; a no-change commit is dropped by `apply`.
    commit();
    // No change means no rebuild, so nothing would consume the restore.
    if ctx.edit_rev.get_untracked() == before {
        ctx.restore_selection.set(None);
    }
}

/// Write whatever is unsaved. Notes go first, so a clean post still saves its
/// notes, and a notes-only save skips the rebuild: they don't affect the site.
fn write_unsaved(ctx: &SaveCtx) -> bool {
    if !save_notes(ctx.notes, &ctx.editing) {
        return false;
    }
    if !ctx.status.dirty.get_untracked() {
        return true;
    }
    let result = ctx.current_doc.with_untracked(|d| {
        let doc = d.as_ref()?;
        let mut guard = ctx.editing.borrow_mut();
        let state = guard.as_mut()?;
        Some(state.save_doc(doc))
    });
    match result {
        // No doc open (or no session): nothing to lose.
        None => true,
        Some(Ok(())) => {
            ctx.status.dirty.set(false);
            ctx.save_error_sig.set(None);
            crate::ui::editing::filename_sync::sync_filename_and_update(
                &ctx.editing,
                ctx.current_doc,
                ctx.current_path,
                ctx.workspace_signal,
                ctx.notes,
            );
            if let Some(state) = ctx.editing.borrow().as_ref() {
                state.session.rebuild();
            }
            true
        }
        Some(Err(msg)) => {
            ctx.save_error_sig.set(Some(msg));
            false
        }
    }
}

/// Save the open document. `overwrite` skips the check for outside changes.
pub fn save_now(ctx: &SaveCtx, restore: Option<SelectionRestore>, overwrite: bool) -> SaveOutcome {
    commit_focused(ctx, restore);
    if ctx.current_doc.with_untracked(Option::is_none) {
        return SaveOutcome::NoDocument;
    }
    let disk = ctx.disk_now();
    if disk == DiskState::Missing {
        // The whole post is unsaved now, even with no edits.
        ctx.status.dirty.set(true);
    }
    match (decide_save(ctx.unsaved(), disk), overwrite) {
        (SaveStep::NothingToDo, _) => SaveOutcome::NothingToSave,
        (SaveStep::ConfirmOverwrite, false) => SaveOutcome::NeedsOverwrite,
        (SaveStep::ConfirmOverwrite, true) | (SaveStep::Write, _) => {
            if write_unsaved(ctx) {
                SaveOutcome::Saved
            } else {
                SaveOutcome::Failed
            }
        }
    }
}

/// Replace the open document and its notes with what's on disk. If the file
/// can't be loaded, the open document stays and the error goes in the bar.
fn refresh_from_disk(ctx: &SaveCtx) {
    let reloaded = {
        let mut guard = ctx.editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.reload_current().map(|()| {
            (
                state.current_doc.clone(),
                state.current_ref.as_ref().map(|r| r.path.clone()),
            )
        })
    };
    match reloaded {
        Ok((doc, path)) => {
            ctx.current_doc.set(doc);
            // Same path, but setting it reseeds the inspector.
            ctx.current_path.set(path);
            after_doc_loaded(ctx.status, ctx.notes, ctx.undo_stack, &ctx.editing);
        }
        Err(e) => ctx.status.refresh_error.set(Some(e)),
    }
}

/// Save, asking first if that would replace an outside change. `then` runs
/// once the post is safely on disk.
pub fn request_save(
    ctx: &SaveCtx,
    restore: Option<SelectionRestore>,
    then: Option<Rc<dyn Fn()>>,
) -> SaveOutcome {
    let outcome = save_now(ctx, restore, false);
    match outcome {
        SaveOutcome::Saved | SaveOutcome::NothingToSave => {
            if let Some(then) = then {
                then();
            }
        }
        SaveOutcome::NeedsOverwrite => {
            let ctx_for_overwrite = ctx.clone();
            ctx.confirm.set(Some(ConfirmRequest {
                message: "This file changed on disk. Overwrite those changes?",
                choices: vec![
                    ConfirmChoice::new("Overwrite", move || {
                        let saved = save_now(&ctx_for_overwrite, None, true);
                        if let (SaveOutcome::Saved, Some(then)) = (saved, &then) {
                            then();
                        }
                    }),
                    ConfirmChoice::cancel(),
                ],
            }));
        }
        SaveOutcome::Failed | SaveOutcome::NoDocument => {}
    }
    outcome
}

/// Run `then`, which replaces the open document, once any unsaved work has
/// been dealt with.
pub fn guard_switch(ctx: &SaveCtx, then: Rc<dyn Fn()>) {
    commit_focused(ctx, None);
    match decide_switch(ctx.unsaved()) {
        SwitchStep::Proceed => then(),
        SwitchStep::Confirm => {
            let ctx_for_save = ctx.clone();
            let then_after_save = Rc::clone(&then);
            ctx.confirm.set(Some(ConfirmRequest {
                message: "Save changes to this post?",
                choices: vec![
                    ConfirmChoice::new("Save", move || {
                        request_save(&ctx_for_save, None, Some(Rc::clone(&then_after_save)));
                    }),
                    ConfirmChoice::new("Don't save", move || then()),
                    ConfirmChoice::cancel(),
                ],
            }));
        }
    }
}

/// Load the outside changes, asking first if that discards unsaved edits.
pub fn request_refresh(ctx: &SaveCtx) {
    commit_focused(ctx, None);
    match decide_refresh(ctx.unsaved(), ctx.status.disk.get_untracked()) {
        RefreshStep::Unavailable => {}
        RefreshStep::Reload => refresh_from_disk(ctx),
        RefreshStep::ConfirmDiscard => {
            let ctx_for_refresh = ctx.clone();
            ctx.confirm.set(Some(ConfirmRequest {
                message: "Refreshing will discard your unsaved changes.",
                choices: vec![
                    ConfirmChoice::new("Discard and refresh", move || {
                        refresh_from_disk(&ctx_for_refresh);
                    }),
                    ConfirmChoice::cancel(),
                ],
            }));
        }
    }
}

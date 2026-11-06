//! Reactive state and persistence for the per-document notes drawer.

use crate::state::EditingState;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use std::cell::RefCell;
use std::rc::Rc;

/// Signals behind the notes drawer. Created in `root_view` so the debug ctrl
/// wiring at root scope can reach them.
#[derive(Clone, Copy)]
pub struct NotesState {
    /// Bumped when a different document's notes load. Keys the drawer's
    /// rebuild. Not `current_path`: a slug rename changes that, and would
    /// rebuild the editor mid-typing.
    pub doc_generation: RwSignal<u64>,
    pub open: RwSignal<bool>,
    /// Live buffer, written on every edit. The single source for saves.
    pub text: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    pub load_error: RwSignal<Option<String>>,
    /// Separate from the footer's `save_error_sig`, which a successful post
    /// save clears.
    pub save_error: RwSignal<Option<String>>,
}

impl NotesState {
    pub fn new() -> Self {
        Self {
            doc_generation: RwSignal::new(0),
            open: RwSignal::new(false),
            text: RwSignal::new(String::new()),
            dirty: RwSignal::new(false),
            load_error: RwSignal::new(None),
            save_error: RwSignal::new(None),
        }
    }
}

impl Default for NotesState {
    fn default() -> Self {
        Self::new()
    }
}

/// Load the open document's notes into `notes` and rebuild the drawer.
///
/// Call after the document changes, and with no `editing` borrow held.
pub fn reload_notes(notes: NotesState, editing: &Rc<RefCell<Option<EditingState>>>) {
    let loaded = editing
        .borrow()
        .as_ref()
        .map_or_else(|| Ok(String::new()), EditingState::load_notes);
    match loaded {
        Ok(text) => {
            notes.text.set(text);
            notes.load_error.set(None);
        }
        Err(e) => {
            notes.text.set(String::new());
            notes.load_error.set(Some(e));
        }
    }
    notes.dirty.set(false);
    notes.save_error.set(None);
    notes.doc_generation.update(|n| *n = n.wrapping_add(1));
}

/// Save the buffer if it has unsaved edits. Returns `false` when the save
/// failed; the text stays in the buffer and the error in `save_error`.
pub fn save_notes(notes: NotesState, editing: &Rc<RefCell<Option<EditingState>>>) -> bool {
    if !notes.dirty.get_untracked() {
        return true;
    }
    let text = notes.text.get_untracked();
    let result = editing.borrow_mut().as_mut().map(|s| s.save_notes(&text));
    match result {
        // No session: nothing to save into.
        None => true,
        Some(Ok(())) => {
            notes.dirty.set(false);
            notes.save_error.set(None);
            true
        }
        Some(Err(msg)) => {
            notes
                .save_error
                .set(Some(format!("notes not saved: {msg}")));
            false
        }
    }
}

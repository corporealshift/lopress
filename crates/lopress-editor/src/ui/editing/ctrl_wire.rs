//! Debug ctrl wiring: snapshot, open, close, and action effects.
//!
//! Gated on `#[cfg(debug_assertions)]`. `wire_ctrl_root` runs once at
//! `root_view` scope (always mounted) and owns the snapshot, open, and close
//! effects, so `/open` and `/close` work even from the welcome screen. It
//! returns the action-channel signal so `editing_view` can wire the action
//! effect via `wire_ctrl_action`, where the `on_action` sink is available.

use crate::actions::BlockAction;
use crate::ctrl::{
    CtrlActionEnvelope, CtrlActionResult, CtrlCloseEnvelope, CtrlCloseResult, CtrlHandle,
    CtrlOpenEnvelope, CtrlOpenResult,
};
use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::blocks::inline_editor::ActionSink;
use crate::ui::StateTag;
use floem::ext_event::create_signal_from_channel;
use floem::reactive::{create_effect, ReadSignal, RwSignal, SignalGet, SignalUpdate, SignalWith};
use lopress_gui_host::{DocumentRef, Session};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Root-scope ctrl wiring: snapshot + open + close effects. Runs once at
/// `root_view` scope so `/open` and `/close` work from the welcome screen.
/// Returns the action-channel read signal for `wire_ctrl_action`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wire_ctrl_root(
    ctrl_handle: CtrlHandle,
    ctrl_action_rx: crossbeam_channel::Receiver<CtrlActionEnvelope>,
    ctrl_open_rx: crossbeam_channel::Receiver<CtrlOpenEnvelope>,
    ctrl_close_rx: crossbeam_channel::Receiver<CtrlCloseEnvelope>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    editing: Rc<RefCell<Option<EditingState>>>,
    state_tag: RwSignal<StateTag>,
) -> ReadSignal<Option<CtrlActionEnvelope>> {
    // Snapshot effect: serialise current doc + path into the handle on change.
    let snap = ctrl_handle.snapshot.clone();
    create_effect(move |_| {
        let json = current_doc.with(|maybe| {
            crate::ctrl::serialize_state(maybe.as_ref(), current_path.get_untracked().as_deref())
        });
        *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
    });

    // Open effect: discover workspace, open it if needed, open the document.
    let open_read = create_signal_from_channel(ctrl_open_rx);
    let editing_for_open = Rc::clone(&editing);
    create_effect(move |_| {
        if let Some((path_str, reply_tx)) = open_read.get() {
            let result = open_document_by_path(
                &path_str,
                &editing_for_open,
                current_doc,
                current_path,
                state_tag,
            );
            let _ = reply_tx.send(result);
        }
    });

    // Close effect: drop the session, clear doc/path, return to welcome.
    let close_read = create_signal_from_channel(ctrl_close_rx);
    let editing_for_close = Rc::clone(&editing);
    create_effect(move |_| {
        if let Some((reply_tx,)) = close_read.get() {
            let result = if editing_for_close.borrow().is_some() {
                *editing_for_close.borrow_mut() = None;
                current_doc.set(None);
                current_path.set(None);
                state_tag.set(StateTag::Welcome);
                CtrlCloseResult::Closed
            } else {
                CtrlCloseResult::NoWorkspace
            };
            let _ = reply_tx.send(result);
        }
    });

    create_signal_from_channel(ctrl_action_rx)
}

/// Editing-scope ctrl wiring: the action effect. Reads the signal returned by
/// `wire_ctrl_root` and dispatches through the `on_action` sink.
pub(crate) fn wire_ctrl_action(
    action_read: ReadSignal<Option<CtrlActionEnvelope>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_action: ActionSink,
) {
    create_effect(move |_| {
        if let Some((ctrl_action, reply_tx)) = action_read.get() {
            let block_id = ctrl_action.block_id();
            // on_action MUST run outside with_untracked — it calls
            // current_doc.update() and would re-borrow the signal.
            let translated: Result<BlockAction, CtrlActionResult> =
                current_doc.with_untracked(|maybe| match maybe.as_ref() {
                    None => Err(CtrlActionResult::NoDocument),
                    Some(doc) => ctrl_action
                        .into_block_action(doc)
                        .ok_or(CtrlActionResult::BlockNotFound { block_id }),
                });
            let result = match translated {
                Ok(action) => {
                    on_action(action);
                    CtrlActionResult::Dispatched
                }
                Err(failure) => failure,
            };
            let _ = reply_tx.send(result);
        }
    });
}

/// Nearest ancestor directory of `md` that contains a `lopress.toml`.
fn find_workspace_root(md: &Path) -> Option<PathBuf> {
    md.ancestors()
        .skip(1)
        .find(|dir| dir.join("lopress.toml").exists())
        .map(Path::to_path_buf)
}

/// Resolve `path_str` to a document, open its workspace if needed, load the
/// document into `current_doc`, and switch to the editing view.
fn open_document_by_path(
    path_str: &str,
    editing: &Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    state_tag: RwSignal<StateTag>,
) -> CtrlOpenResult {
    let raw = PathBuf::from(path_str);
    // Absolute → used as-is. Relative → resolved against the open workspace
    // root (requires a workspace already open).
    let target = if raw.is_absolute() {
        raw
    } else {
        let Some(root) = editing
            .borrow()
            .as_ref()
            .map(|s| s.session.workspace().root)
        else {
            return CtrlOpenResult::NoWorkspace;
        };
        root.join(&raw)
    };
    if !target.exists() {
        return CtrlOpenResult::NotFound;
    }
    let Some(ws_root) = find_workspace_root(&target) else {
        return CtrlOpenResult::NotFound;
    };
    // Open the workspace unless the matching one is already active.
    let already_open = editing
        .borrow()
        .as_ref()
        .map(|s| s.session.workspace().root == ws_root)
        .unwrap_or(false);
    if !already_open {
        match Session::open(&ws_root) {
            Ok(session) => *editing.borrow_mut() = Some(EditingState::new(session)),
            Err(_) => return CtrlOpenResult::NotFound,
        }
    }
    let doc_ref = DocumentRef {
        path: target.clone(),
        title: String::new(),
        slug: String::new(),
        date: None,
        is_draft: false,
        has_parse_error: false,
    };
    {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return CtrlOpenResult::NotFound;
        };
        state.open_document(&doc_ref);
    }
    let doc = editing
        .borrow()
        .as_ref()
        .and_then(|s| s.current_doc.clone());
    // Set current_doc + current_path BEFORE flipping state_tag so editing_view
    // sees a populated doc when it mounts.
    current_doc.set(doc);
    current_path.set(Some(target));
    state_tag.set(StateTag::Editing);
    CtrlOpenResult::Opened
}

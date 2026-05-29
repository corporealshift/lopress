//! Debug ctrl wiring: snapshot effect and action-receive effect.
//!
//! Gated on `#[cfg(debug_assertions)]`. `wire_ctrl` sets up two Floem
//! effects — one that serialises the current document into the ctrl
//! handle's snapshot, and one that receives ctrl actions from the
//! channel, translates them to `BlockAction`, and dispatches via
//! `on_action`.

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
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Wire up the debug ctrl handle and action channel.
///
/// Sets up four effects:
/// 1. Serialises `current_doc` + `current_path` into the ctrl handle's
///    snapshot on every signal change.
/// 2. Listens on `ctrl_action_rx` for ctrl actions, translates them
///    to `BlockAction`, and dispatches via `on_action`.
/// 3. Listens on `ctrl_open_rx` for open requests and dispatches via
///    `on_open`.
/// 4. Listens on `ctrl_close_rx` for close requests and clears state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wire_ctrl(
    ctrl_handle: CtrlHandle,
    ctrl_action_rx: crossbeam_channel::Receiver<CtrlActionEnvelope>,
    ctrl_open_rx: crossbeam_channel::Receiver<CtrlOpenEnvelope>,
    ctrl_close_rx: crossbeam_channel::Receiver<CtrlCloseEnvelope>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: ActionSink,
    on_open: Rc<dyn Fn(PathBuf)>,
    editing: Rc<RefCell<Option<EditingState>>>,
    state_tag: RwSignal<StateTag>,
) {
    // Existing snapshot effect — unchanged.
    let snap = ctrl_handle.snapshot.clone();
    create_effect(move |_| {
        let json = current_doc.with(|maybe| {
            crate::ctrl::serialize_state(maybe.as_ref(), current_path.get_untracked().as_deref())
        });
        *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
    });

    // Existing action effect — unchanged shape, just rebound to the local rx.
    let action_read = create_signal_from_channel(ctrl_action_rx);
    create_effect(move |_| {
        if let Some((ctrl_action, reply_tx)) = action_read.get() {
            let block_id = ctrl_action.block_id();
            // Translate against the current doc. into_block_action's
            // only failure mode is an unknown block id; a missing doc
            // is detected separately so the caller gets a precise
            // result. on_action MUST run outside with_untracked — it
            // calls current_doc.update() and would re-borrow the signal.
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

    // New: open effect. Resolves the requested path against the open workspace
    // (or as absolute) and dispatches through the same `on_open` closure the
    // welcome view uses.
    let open_read = create_signal_from_channel(ctrl_open_rx);
    let editing_for_open = Rc::clone(&editing);
    create_effect(move |_| {
        if let Some((path_str, reply_tx)) = open_read.get() {
            let raw = PathBuf::from(&path_str);
            let resolved: Option<PathBuf> = if raw.is_absolute() {
                Some(raw)
            } else {
                editing_for_open
                    .borrow()
                    .as_ref()
                    .map(|s| s.session.workspace().root.join(&raw))
            };
            let result = match resolved {
                None => CtrlOpenResult::NoWorkspace,
                Some(p) if !p.exists() => CtrlOpenResult::NotFound,
                Some(p) => {
                    on_open(p);
                    CtrlOpenResult::Opened
                }
            };
            let _ = reply_tx.send(result);
        }
    });

    // New: close effect. Clears `current_doc`, drops the EditingState, returns
    // the app to the welcome view.
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
}

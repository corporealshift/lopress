//! Debug ctrl wiring: snapshot effect and action-receive effect.
//!
//! Gated on `#[cfg(debug_assertions)]`. `wire_ctrl` sets up two Floem
//! effects — one that serialises the current document into the ctrl
//! handle's snapshot, and one that receives ctrl actions from the
//! channel, translates them to `BlockAction`, and dispatches via
//! `on_action`.

use crate::actions::BlockAction;
use crate::ctrl::{CtrlActionResult, CtrlHandle};
use crate::model::types::EditorDoc;
use crate::ui::blocks::inline_editor::ActionSink;
use floem::ext_event::create_signal_from_channel;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalWith};
use std::path::PathBuf;

/// Wire up the debug ctrl handle and action channel.
///
/// Sets up two effects:
/// 1. Serialises `current_doc` + `current_path` into the ctrl handle's
///    snapshot on every signal change.
/// 2. Listens on `ctrl_action_rx` for ctrl actions, translates them
///    to `BlockAction`, and dispatches via `on_action`.
pub fn wire_ctrl(
    ctrl_handle: CtrlHandle,
    ctrl_action_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: ActionSink,
) {
    let snap = ctrl_handle.snapshot.clone();
    create_effect(move |_| {
        let json = current_doc.with(|maybe| {
            crate::ctrl::serialize_state(
                maybe.as_ref(),
                current_path.get_untracked().as_deref(),
            )
        });
        *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
    });

    let action_read = create_signal_from_channel(ctrl_action_rx);
    create_effect(move |_| {
        if let Some((ctrl_action, reply_tx)) = action_read.get() {
            let block_id = ctrl_action.block_id();
            // Translate against the current doc. into_block_action's
            // only failure mode is an unknown block id; a missing doc
            // is detected separately so the caller gets a precise
            // result. on_action MUST run outside with_untracked — it
            // calls current_doc.update() and would re-borrow the signal.
            let translated: Result<BlockAction, CtrlActionResult> = current_doc
                .with_untracked(|maybe| match maybe.as_ref() {
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

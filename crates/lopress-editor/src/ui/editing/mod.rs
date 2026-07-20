//! Editing-mode view: assembles the pieces built by sibling modules.
//!
//! Each sibling module (`focus`, `pane_key`, `action_sink`, `undo_redo`,
//! `save_pipeline`, `new_doc`, `ctrl_wire`) owns a responsibility and
//! exports a free function that `editing_view` calls.

pub mod action_sink;
#[cfg(debug_assertions)]
pub mod ctrl_wire;
pub mod filename_sync;
pub mod focus;
pub mod new_doc;
pub mod pane_key;
pub mod save_pipeline;
pub mod undo_redo;

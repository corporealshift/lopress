//! Editing-mode view: assembles the pieces built by sibling modules.
//!
//! Each sibling module (`focus`, `pane_key`, `action_sink`, `undo_redo`,
//! `save_pipeline`, `new_doc`, `ctrl_wire`) owns a responsibility and
//! exports a free function that `editing_view` calls.

pub mod focus;
pub mod pane_key;
pub mod new_doc;

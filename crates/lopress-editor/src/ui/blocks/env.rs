//! The block-independent environment every editor widget renders into.
//! Created once per column rebuild; cloned freely (all fields are Copy signals or Rc).

use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, ActiveCommitSlot, FocusPublisher};
use crate::ui::link_bar::LinkEdit;
use floem::reactive::RwSignal;
use std::rc::Rc;

/// The block-independent environment every editor widget renders into.
/// Created once per column rebuild; cloned freely (all fields are Copy
/// signals or Rc).
#[derive(Clone)]
pub struct BlockEnv {
    pub on_action: ActionSink,                    // Rc<dyn Fn(BlockAction)>
    pub focus_target: RwSignal<Option<BlockId>>,  // Copy
    pub focus_pub: FocusPublisher,                // Copy (signals inside)
    pub current_doc: RwSignal<Option<EditorDoc>>, // Copy
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
    /// Stable pane-level signal driving the link-URL editor bar. Set by the
    /// toolbar Link button / Ctrl+K (with the captured selection range); read
    /// by the pane-level `link_bar_view`. Stable across column rebuilds, unlike
    /// `focus_pub`, so the bar survives the focus-loss rebuild.
    pub link_edit: RwSignal<Option<LinkEdit>>, // Copy
    /// Pane-stable slot holding the focused block editor's commit closure,
    /// flushed by doc-switch paths before `current_doc` is replaced. Like
    /// `link_edit`, it lives in `editing_view` (not the rebuilt column) so
    /// the flush survives focus-loss rebuilds. See `ActiveCommitSlot`.
    pub active_commit: ActiveCommitSlot, // Copy
}

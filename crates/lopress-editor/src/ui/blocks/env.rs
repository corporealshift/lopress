//! The block-independent environment every editor widget renders into.
//! Created once per column rebuild; cloned freely (all fields are Copy signals or Rc).

use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
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
}

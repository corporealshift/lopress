//! Reactive plumbing for doc-level selection.
//!
//! `SelectionContext` bundles the three pane-level handles that every
//! editable block widget needs: the canonical doc selection, the current
//! document (so cross-block keyboard routing can enumerate blocks), and the
//! geometry cache (so vertical-arrow navigation can find the offset whose
//! cached x is closest in the target block).

use crate::model::types::EditorDoc;
use crate::selection::{DocSelection, GeometryCache};
use floem::reactive::RwSignal;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct SelectionContext {
    pub doc_selection: RwSignal<DocSelection>,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub geometry: Rc<RefCell<GeometryCache>>,
}

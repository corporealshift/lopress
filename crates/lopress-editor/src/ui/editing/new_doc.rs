//! "+ New post" / "+ New page" sidebar actions.

use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::sidebar::{new_doc_stub, unique_untitled_path};
use floem::reactive::{RwSignal, SignalUpdate};
use lopress_gui_host::{DocumentRef, WorkspaceSummary};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Whether a "+ New …" sidebar action targets the Posts or Pages directory.
#[derive(Clone, Copy)]
pub enum DocKind {
    Post,
    Page,
}

impl DocKind {
    pub fn default_title(self) -> &'static str {
        match self {
            DocKind::Post => "New Post",
            DocKind::Page => "New Page",
        }
    }
}

/// Build the closure the sidebar invokes for "+ New post" / "+ New page".
///
/// The closure: picks a fresh `untitled-N.md` filename, writes the stub
/// markdown, rescans the workspace, then opens the new doc through
/// `EditingState::open_document` so the editor pane and current_path signal
/// stay in sync with the sidebar.
pub fn make_new_doc_action(
    editing: Rc<RefCell<Option<EditingState>>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    kind: DocKind,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        let dir = match kind {
            DocKind::Post => state.session.posts_dir(),
            DocKind::Page => state.session.pages_dir(),
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("failed to create {}: {e}", dir.display());
            return;
        }
        let path = unique_untitled_path(&dir);
        if let Err(e) = std::fs::write(&path, new_doc_stub(kind.default_title())) {
            eprintln!("failed to write {}: {e}", path.display());
            return;
        }

        let summary = state.session.rescan();
        let doc_ref = summary
            .posts
            .iter()
            .chain(summary.pages.iter())
            .find(|d| d.path == path)
            .cloned()
            .unwrap_or_else(|| DocumentRef {
                path: path.clone(),
                title: kind.default_title().to_string(),
                is_draft: true,
                has_parse_error: false,
            });

        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        workspace_signal.set(summary);
    })
}

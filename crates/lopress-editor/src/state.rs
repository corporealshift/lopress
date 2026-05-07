//! App state. Adds the live `EditorDoc` slot to `EditingState` (Task 7).

use crate::model::from_core::doc_from_core;
use crate::model::types::EditorDoc;
use crate::settings::Settings;
use lopress_core::Document;
use lopress_gui_host::{DocumentRef, Session};
use lopress_plugin::PluginRegistry;

/// Top-level application state, discriminated by which screen is active.
pub enum AppState {
    Welcome(WelcomeState),
    Editing(Box<EditingState>),
}

/// State for the Welcome screen.
#[derive(Default, Clone)]
pub struct WelcomeState {
    /// Error message to display above the action buttons, if any.
    pub error: Option<String>,
}

/// State for the Editing screen.
pub struct EditingState {
    pub session: Session,
    /// Plugin registry loaded from the workspace at session-open time.
    /// Used by `from_core` to classify plugin-declared block types and by
    /// the plugin block view to render attr forms.
    pub plugin_registry: PluginRegistry,
    /// The currently active document, if one has been opened.
    pub current_doc: Option<EditorDoc>,
    pub current_ref: Option<DocumentRef>,
    /// Last error encountered while editing.
    pub last_error: Option<String>,
}

impl EditingState {
    /// Create a new `EditingState` wrapping the given `session`.
    pub fn new(session: Session) -> Self {
        let plugin_registry = session.plugin_registry();
        Self {
            session,
            plugin_registry,
            current_doc: None,
            current_ref: None,
            last_error: None,
        }
    }

    /// Load and parse the document at `doc_ref.path`, replacing `current_doc`.
    /// On failure, clears `current_doc` and stores the error message.
    pub fn open_document(&mut self, doc_ref: &DocumentRef) {
        match self.session.load_document(&doc_ref.path) {
            Ok(loaded) => {
                let core_doc = Document {
                    front_matter: loaded.front_matter,
                    blocks: loaded.blocks,
                };
                self.current_doc = Some(doc_from_core(&core_doc, &self.plugin_registry));
                self.current_ref = Some(doc_ref.clone());
                self.last_error = None;
            }
            Err(e) => {
                self.current_doc = None;
                self.current_ref = Some(doc_ref.clone());
                self.last_error = Some(e.to_string());
            }
        }
    }
}

/// Shared context threaded through the entire application.
pub struct AppContext {
    pub settings: Settings,
    pub state: AppState,
}

impl AppContext {
    /// Create a new `AppContext` with the given settings, starting in the Welcome state.
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            state: AppState::Welcome(WelcomeState::default()),
        }
    }
}

//! App state. The full document model (EditorDoc, etc.) arrives in Task 4.
//! Task 3 adds AppState, WelcomeState, EditingState and AppContext.

use crate::settings::Settings;
use lopress_gui_host::{DocumentRef, Session};

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

/// State for the Editing screen. The full document model is added in Task 4.
pub struct EditingState {
    pub session: Session,
    /// The currently active document, if one has been opened.
    pub current_ref: Option<DocumentRef>,
    /// Last error encountered while editing.
    pub last_error: Option<String>,
}

impl EditingState {
    /// Create a new `EditingState` wrapping the given `session`.
    pub fn new(session: Session) -> Self {
        Self {
            session,
            current_ref: None,
            last_error: None,
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

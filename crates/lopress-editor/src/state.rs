//! App state. Adds the live `EditorDoc` slot to `EditingState` (Task 7).

use crate::model::from_core::doc_from_core;
use crate::model::to_core::doc_to_core;
use crate::model::types::EditorDoc;
use crate::settings::Settings;
use lopress_core::perf;
use lopress_core::{Document, FrontMatter};
use lopress_gui_host::{DocumentRef, LoadedDocument, Session};
use lopress_plugin::PluginRegistry;
use std::path::PathBuf;

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
    ///
    /// The plugin registry is seeded with the built-in base plugins first,
    /// then user plugins from the workspace are layered on top. A user plugin
    /// that declares a block name already owned by a base plugin is rejected
    /// by `insert` (and silently skipped here) — base plugins are non-removable.
    pub fn new(session: Session) -> Self {
        let mut plugin_registry = PluginRegistry::default();
        if let Err(e) = plugin_registry.load_base_plugins() {
            eprintln!("failed to load base plugins: {e}");
        }
        for plugin in session.plugin_registry().plugins {
            // `insert` recomputes block/theme indices from the registry's
            // current length, so moving a `LoadedPlugin` across registries
            // is sound. Duplicate block names (e.g. a user plugin shadowing
            // a base block) are skipped.
            let _ = plugin_registry.insert(plugin);
        }
        Self {
            session,
            plugin_registry,
            current_doc: None,
            current_ref: None,
            last_error: None,
        }
    }

    /// Save the given `EditorDoc` to the path of the currently open document.
    /// Returns the error message on failure.
    ///
    /// Takes the doc to save by reference rather than reading
    /// `self.current_doc` because the live edit state lives in a UI signal,
    /// not in `EditingState` (which only stores the doc as opened).
    pub fn save_doc(&self, doc: &EditorDoc) -> Result<(), String> {
        let path = self
            .current_ref
            .as_ref()
            .map(|r| r.path.clone())
            .ok_or_else(|| "no document open".to_string())?;
        let core = doc_to_core(doc);
        let loaded = LoadedDocument {
            path,
            front_matter: core.front_matter,
            blocks: core.blocks,
            dirty: false,
            dirty_at: None,
            last_written: None,
            last_save_error: None,
        };
        self.session.save(&loaded).map_err(|e| e.to_string())
    }

    /// Rename the open document's file so its stem matches `front_matter`'s
    /// effective slug. Returns the new path when a rename happened, `None`
    /// when the filename already matched (or no doc is open).
    ///
    /// Updates `self.current_ref.path` on a successful rename; callers are
    /// responsible for reflecting the new path in the UI signals and
    /// re-scanning the workspace.
    pub fn sync_filename(&mut self, front_matter: &FrontMatter) -> Result<Option<PathBuf>, String> {
        let Some(current) = self.current_ref.as_ref().map(|r| r.path.clone()) else {
            return Ok(None);
        };
        match crate::ui::editing::filename_sync::rename_to_slug(front_matter, &current) {
            Ok(Some(new_path)) => {
                if let Some(r) = self.current_ref.as_mut() {
                    r.path = new_path.clone();
                }
                Ok(Some(new_path))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Load and parse the document at `doc_ref.path`, replacing `current_doc`.
    /// On failure, clears `current_doc` and stores the error message.
    pub fn open_document(&mut self, doc_ref: &DocumentRef) {
        let load_result = {
            let _t = perf::span("editor.open_document.load_parse");
            self.session.load_document(&doc_ref.path)
        };
        match load_result {
            Ok(loaded) => {
                let core_doc = Document {
                    front_matter: loaded.front_matter,
                    blocks: loaded.blocks,
                };
                let editor_doc = {
                    let _t = perf::span("editor.open_document.from_core");
                    doc_from_core(&core_doc, &self.plugin_registry)
                };
                self.current_doc = Some(editor_doc);
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

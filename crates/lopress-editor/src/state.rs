//! App state. Adds the live `EditorDoc` slot to `EditingState` (Task 7).

use crate::model::from_core::doc_from_core;
use crate::model::to_core::doc_to_core;
use crate::model::types::EditorDoc;
use crate::settings::Settings;
use lopress_core::perf;
use lopress_core::Document;
use lopress_gui_host::{DocumentRef, LoadedDocument, Session};
use lopress_plugin::PluginRegistry;
use std::path::{Path, PathBuf};

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
    /// If the front-matter slug differs from the current filename, the file
    /// is renamed to match the slug before saving.
    ///
    /// Returns `Some(new_path)` when a rename occurred, `None` otherwise.
    ///
    /// Takes the doc to save by reference rather than reading
    /// `self.current_doc` because the live edit state lives in a UI signal,
    /// not in `EditingState` (which only stores the doc as opened).
    pub fn save_doc(&mut self, doc: &EditorDoc) -> Result<Option<PathBuf>, String> {
        let old_path = self
            .current_ref
            .as_ref()
            .map(|r| r.path.clone())
            .ok_or_else(|| "no document open".to_string())?;

        // Compute the desired filename from front-matter slug.
        let slug = self.current_ref.as_ref().map(|r| r.slug.as_str());
        let rename_target = slug_rename_target(&old_path, slug);

        let (new_path, did_rename) = match rename_target {
            Some(target) => {
                if target.exists() {
                    eprintln!(
                        "slug rename skipped: target already exists at {}",
                        target.display()
                    );
                    (old_path.clone(), false)
                } else {
                    match std::fs::rename(&old_path, &target) {
                        Ok(()) => {
                            // Update current_ref so subsequent saves target the new path.
                            if let Some(ref mut r) = self.current_ref {
                                r.path = target.clone();
                            }
                            (target, true)
                        }
                        Err(e) => {
                            eprintln!("slug rename failed: {e}");
                            (old_path.clone(), false)
                        }
                    }
                }
            }
            None => (old_path.clone(), false),
        };

        let core = doc_to_core(doc);
        let loaded = LoadedDocument {
            path: new_path.clone(),
            front_matter: core.front_matter,
            blocks: core.blocks,
            dirty: false,
            dirty_at: None,
            last_written: None,
            last_save_error: None,
        };
        self.session
            .save(&loaded)
            .map_err(|e| e.to_string())
            .map(|()| if did_rename { Some(new_path) } else { None })
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

/// Compute the desired filename for a document based on its front-matter slug.
///
/// Returns `Some(path)` when the slug is valid and differs from the current
/// filename, meaning a rename is warranted.  Returns `None` when the slug
/// is absent/empty, matches the current name, or contains illegal characters.
///
/// Validation rules (no slugification — reject, don't fix):
/// - Slug must be `Some` and non-empty after trimming.
/// - Only `[A-Za-z0-9._ -]` allowed; no leading/trailing dot or space.
/// - Must be a single path component (no `/` or `\`).
pub fn slug_rename_target(current_path: &Path, slug: Option<&str>) -> Option<PathBuf> {
    let raw = slug?;
    // Reject leading/trailing space or dot (Windows-safe) BEFORE trimming.
    if raw.starts_with(['.', ' ']) || raw.ends_with(['.', ' ']) {
        return None;
    }
    let slug = raw.trim();
    if slug.is_empty() {
        return None;
    }
    // Reject path separators and any character outside the safe set.
    if slug.contains(['/', '\\']) {
        return None;
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ' ' | '-'))
    {
        return None;
    }
    // No leading or trailing dot (spaces already rejected above).
    if slug.starts_with('.') || slug.ends_with('.') {
        return None;
    }
    let dir = current_path.parent()?;
    let desired = dir.join(format!("{slug}.md"));
    if desired == current_path {
        return None;
    }
    Some(desired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slug_returns_new_path() {
        let path = PathBuf::from("src/posts/old-name.md");
        let target = slug_rename_target(&path, Some("new-name"));
        assert_eq!(target, Some(PathBuf::from("src/posts/new-name.md")));
    }

    #[test]
    fn slug_with_spaces_is_valid() {
        let path = PathBuf::from("src/posts/my-post.md");
        let target = slug_rename_target(&path, Some("my post"));
        assert_eq!(target, Some(PathBuf::from("src/posts/my post.md")));
    }

    #[test]
    fn slug_with_dots_and_underscores_is_valid() {
        let path = PathBuf::from("src/posts/old.md");
        let target = slug_rename_target(&path, Some("a.b_c"));
        assert_eq!(target, Some(PathBuf::from("src/posts/a.b_c.md")));
    }

    #[test]
    fn none_slug_returns_none() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, None).is_none());
    }

    #[test]
    fn empty_slug_returns_none() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("")).is_none());
    }

    #[test]
    fn whitespace_only_slug_returns_none() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("   ")).is_none());
    }

    #[test]
    fn same_name_returns_none() {
        let path = PathBuf::from("src/posts/my-post.md");
        assert!(slug_rename_target(&path, Some("my-post")).is_none());
    }

    #[test]
    fn trailing_dot_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("bad.")).is_none());
    }

    #[test]
    fn trailing_space_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("bad ")).is_none());
    }

    #[test]
    fn leading_dot_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some(".bad")).is_none());
    }

    #[test]
    fn leading_space_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some(" bad")).is_none());
    }

    #[test]
    fn path_separator_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("a/b")).is_none());
    }

    #[test]
    fn backslash_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("a\\b")).is_none());
    }

    #[test]
    fn special_chars_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("bad@#$")).is_none());
    }

    #[test]
    fn unicode_rejected() {
        let path = PathBuf::from("src/posts/foo.md");
        assert!(slug_rename_target(&path, Some("café")).is_none());
    }
}

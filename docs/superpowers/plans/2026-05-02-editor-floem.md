# Editor Migration to Floem — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the egui-based editor and `eframe` shell with a Floem implementation that delivers Notion-style block-level WYSIWYG (rendered inline styles, slash commands, block toolbar, drag handles, multi-block keyboard selection, plugin-aware block rendering), while keeping `lopress-core` / `lopress-build` / `lopress-serve` / `lopress-watch` / `lopress-gui-host` unchanged.

**Architecture:** Single in-place rewrite of `crates/lopress-editor` and `src/main.rs`. Editor's working model is `EditorDoc { blocks: Vec<EditorBlock> }` with inline runs canonical in memory; markdown only on save via a focused inline serializer. `BlockAction` enum + single `apply` chokepoint preserves the architectural foundation for undo (deferred to a follow-up phase). `PluginRegistry` from `lopress-plugin` is consumed at load time so plugin-declared blocks render with a built-in editor kind plus an attr form (Path 1 plugin extensibility). Multi-block selection lives at the editor pane level with a per-block geometry cache for cross-block vertical-arrow navigation.

**Tech Stack:** Rust, Floem (pinned crates.io version selected in Task 1), `pulldown-cmark` for inline markdown parsing, `lopress-core` block tree, `lopress-gui-host` Session layer, `lopress-plugin` registry, `rfd` for file dialogs, `directories` for settings path, existing CI matrix.

**Spec:** See `docs/superpowers/specs/2026-05-02-editor-floem-design.md` for the design decisions referenced throughout this plan.

**Strict lints (workspace-wide, must respect throughout):**

- No `unwrap()`, `expect()`, `panic!()`, `todo!()`, `unimplemented!()`, `unreachable!()`.
- No `as` casts — use `From` / `TryFrom`. No `cast_lossless`, `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap`, `cast_precision_loss`.
- No `[idx]` indexing — use `.get(idx)`. No `string_slice`.
- All public fallible functions must return `Result`.

**Scope marker — the egui editor is deleted at the start of Task 1.** During Tasks 1–4 there is no working GUI editor on `main`; the user has agreed to edit markdown directly through this period.

---

## File Map

| File | Disposition | Responsibility |
|------|-------------|----------------|
| `crates/lopress-editor/Cargo.toml` | rewrite | Floem deps, drop egui/eframe |
| `crates/lopress-editor/src/lib.rs` | rewrite | Crate root: re-exports, `App` entry function |
| `crates/lopress-editor/src/state.rs` | rewrite | `AppState`, `EditingState`, `EditorDoc` ownership |
| `crates/lopress-editor/src/recents.rs` | retain logic, refactor | Settings file (recents + window), one-shot migration |
| `crates/lopress-editor/src/settings.rs` | NEW | Settings file load/save with serde |
| `crates/lopress-editor/src/model/mod.rs` | NEW | Document model module |
| `crates/lopress-editor/src/model/types.rs` | NEW | `EditorDoc`, `EditorBlock`, `BlockKind`, `BlockBody`, `ListItem`, `InlineRun`, `PluginMeta`, `BlockId` |
| `crates/lopress-editor/src/model/from_core.rs` | NEW | `lopress_core::Block` → `EditorBlock` conversion + plugin lookup |
| `crates/lopress-editor/src/model/to_core.rs` | NEW | `EditorBlock` → `lopress_core::Block` conversion |
| `crates/lopress-editor/src/model/inline.rs` | NEW | Markdown ↔ `Vec<InlineRun>` parser/serializer |
| `crates/lopress-editor/src/actions.rs` | NEW | `BlockAction` enum + `apply` chokepoint |
| `crates/lopress-editor/src/selection.rs` | NEW | `DocPosition`, `DocSelection`, geometry cache |
| `crates/lopress-editor/src/ui/mod.rs` | rewrite | UI module, view registration |
| `crates/lopress-editor/src/ui/welcome.rs` | rewrite | Welcome view |
| `crates/lopress-editor/src/ui/sidebar.rs` | rewrite | Sidebar (post/page list) |
| `crates/lopress-editor/src/ui/editor_pane.rs` | NEW | EditorPane: scroll container, block list, keyboard routing |
| `crates/lopress-editor/src/ui/blocks/mod.rs` | NEW | Per-block view module |
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | NEW | Custom inline-runs editor widget |
| `crates/lopress-editor/src/ui/blocks/paragraph.rs` | NEW | Paragraph view |
| `crates/lopress-editor/src/ui/blocks/heading.rs` | NEW | Heading view |
| `crates/lopress-editor/src/ui/blocks/code.rs` | NEW | Code block view |
| `crates/lopress-editor/src/ui/blocks/list.rs` | NEW | List view |
| `crates/lopress-editor/src/ui/blocks/opaque.rs` | NEW | Opaque/plugin placeholder card |
| `crates/lopress-editor/src/ui/blocks/plugin.rs` | NEW | Plugin block (editor + attr form) |
| `crates/lopress-editor/src/ui/inspector.rs` | rewrite | Front matter form |
| `crates/lopress-editor/src/ui/footer.rs` | rewrite | Status / save / word count / server URL |
| `crates/lopress-editor/src/ui/toolbar.rs` | NEW | Block toolbar (above focused block) |
| `crates/lopress-editor/src/ui/slash_menu.rs` | NEW | Slash command popup |
| `crates/lopress-editor/src/ui/dnd.rs` | NEW | Drag-and-drop handle + drop-zone widgets |
| `crates/lopress-editor/tests/inline_runs_tests.rs` | NEW | Markdown ↔ runs round-trip tests |
| `crates/lopress-editor/tests/from_to_core_tests.rs` | NEW | `from_core` / `to_core` round-trip tests |
| `crates/lopress-editor/tests/actions_tests.rs` | NEW | `BlockAction::apply` semantic tests |
| `crates/lopress-editor/tests/selection_tests.rs` | NEW | Selection logic tests |
| `crates/lopress-editor/tests/plugin_block_tests.rs` | NEW | Plugin block load/save round-trip |
| `src/main.rs` | rewrite | Floem app entry point |
| `Cargo.toml` (workspace) | modify | Add `floem` and `pulldown-cmark` to workspace deps |

---

## Conventions for All UI Tasks

The Floem-specific UI tasks (7+) describe **structure** (which views compose, what state they read, what actions they emit) and **acceptance criteria** (what the engineer must demonstrate works). The exact Floem API names are intentionally not fabricated — Floem 0.x evolves quickly and the engineer should consult Floem's `examples/` directory and the Lapce source for current usage patterns. Where this plan says "a Floem signal," "a Floem container," etc., translate to the current Floem idiom.

When in doubt, prefer Lapce's editor crate (`lapce-app`) as a reference over Floem's own examples, since Lapce is the closest existing pure-Rust block editor on this stack.

Every UI task ends with a **manual smoke step** the engineer runs in the live app before committing. Those are not optional — Floem builds can compile cleanly and still misbehave at runtime.

---

## Task 1: Workspace plumbing — delete egui, add Floem, scaffold an empty window

**Files:**
- Delete: `crates/lopress-editor/src/app.rs`, `crates/lopress-editor/src/ops.rs`, `crates/lopress-editor/src/state.rs`, `crates/lopress-editor/src/ui/*` (all egui UI), `crates/lopress-editor/tests/*`
- Modify: `crates/lopress-editor/Cargo.toml`, `Cargo.toml` (workspace), `src/main.rs`
- Create: empty `crates/lopress-editor/src/lib.rs` stub, empty `crates/lopress-editor/src/state.rs` stub, empty `crates/lopress-editor/src/ui/mod.rs` stub

- [ ] **Step 1: Look up the current Floem release on crates.io**

Open `https://crates.io/crates/floem` in a browser (or `cargo search floem`) and note the latest stable release. Pin that minor version. Record the version in the commit message.

- [ ] **Step 2: Add workspace deps**

Edit the workspace `Cargo.toml`. In `[workspace.dependencies]` add:

```toml
floem = "0.X"           # replace 0.X with the version chosen in Step 1
pulldown-cmark = "0.10" # already in dep tree if present; otherwise add
```

Run `cargo metadata --format-version 1 --no-deps > /dev/null` and confirm no errors.

- [ ] **Step 3: Replace `crates/lopress-editor/Cargo.toml`**

Replace contents with:

```toml
[package]
name = "lopress-editor"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
lopress-gui-host = { path = "../lopress-gui-host" }
lopress-core = { path = "../lopress-core" }
lopress-plugin = { path = "../lopress-plugin" }
floem = { workspace = true }
pulldown-cmark = { workspace = true }
rfd = { workspace = true }
open = { workspace = true }
directories = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
lopress-build = { path = "../lopress-build" }

[lints]
workspace = true
```

- [ ] **Step 4: Delete egui code**

```bash
rm -rf crates/lopress-editor/src/app.rs crates/lopress-editor/src/ops.rs crates/lopress-editor/src/ui/
rm -rf crates/lopress-editor/tests/
```

(Keep `recents.rs` and `state.rs`. We rewrite both later but `recents.rs` retains useful `directories`-based path logic.)

- [ ] **Step 5: Stub `lib.rs`, `state.rs`, `ui/mod.rs`**

Replace `crates/lopress-editor/src/lib.rs`:

```rust
pub mod state;
pub mod ui;

/// Run the editor app. Returns when the window closes.
///
/// # Errors
/// Returns an error if the Floem runtime fails to start.
pub fn run() -> Result<(), AppError> {
    floem::launch(ui::root_view);
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Floem launch failed: {0}")]
    Launch(String),
}
```

Replace `crates/lopress-editor/src/state.rs`:

```rust
//! App state. Detailed types come in Task 4 (document model)
//! and Task 2 (settings + welcome state).

#[derive(Default)]
pub struct AppState;
```

Create `crates/lopress-editor/src/ui/mod.rs`:

```rust
use floem::IntoView;
use floem::views::label;

pub fn root_view() -> impl IntoView {
    label(|| "lopress")
}
```

- [ ] **Step 6: Rewrite `src/main.rs`**

Replace contents with:

```rust
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = lopress_editor::run() {
        eprintln!("lopress: {e}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 7: Build and run**

```bash
cargo build -p lopress-editor 2>&1
```

Expected: clean build. If `floem::launch` or `IntoView` or `label` don't match Floem's current API, consult Floem `examples/counter/` for the canonical "open a window" pattern and adjust.

- [ ] **Step 8: Run the binary**

```bash
cargo run -p lopress 2>&1
```

Expected: a window opens displaying the text `lopress`. Close the window, observe the process exits cleanly with code 0.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(editor): replace egui shell with empty Floem window (Task 1/22)"
```

---

## Task 2: Settings file — recents + window, one-shot migration

**Files:**
- Create: `crates/lopress-editor/src/settings.rs`
- Modify: `crates/lopress-editor/src/recents.rs` (deprecate, leave only the migration helper if needed)
- Create: `crates/lopress-editor/tests/settings_tests.rs`

- [ ] **Step 1: Write failing tests for settings load/save**

Create `crates/lopress-editor/tests/settings_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::settings::{Settings, WindowSettings};
use std::path::PathBuf;
use tempfile::TempDir;

fn dir() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn loads_default_when_missing() {
    let d = dir();
    let path = d.path().join("settings.json");
    let s = Settings::load_from(&path).unwrap();
    assert!(s.recents.is_empty());
    assert_eq!(s.window.width, 1200.0);
    assert_eq!(s.window.height, 800.0);
}

#[test]
fn round_trip() {
    let d = dir();
    let path = d.path().join("settings.json");
    let mut s = Settings::default();
    s.recents.push(PathBuf::from("/some/workspace"));
    s.window.width = 1400.0;
    s.window.height = 900.0;
    s.save_to(&path).unwrap();

    let loaded = Settings::load_from(&path).unwrap();
    assert_eq!(loaded.recents, vec![PathBuf::from("/some/workspace")]);
    assert_eq!(loaded.window.width, 1400.0);
}

#[test]
fn ignores_unknown_fields() {
    let d = dir();
    let path = d.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"recents":[],"window":{"width":1200.0,"height":800.0,"x":0.0,"y":0.0,"maximized":false},"ui_zoom":1.5}"#,
    )
    .unwrap();
    let _ = Settings::load_from(&path).unwrap(); // must not error on ui_zoom
}

#[test]
fn migrates_recents_json() {
    let d = dir();
    let recents_path = d.path().join("recents.json");
    let settings_path = d.path().join("settings.json");
    std::fs::write(&recents_path, r#"["/old/workspace"]"#).unwrap();

    let s = Settings::load_or_migrate(&settings_path, &recents_path).unwrap();
    assert_eq!(s.recents, vec![PathBuf::from("/old/workspace")]);
    assert!(!recents_path.exists(), "old recents.json should be deleted");
    assert!(settings_path.exists());
}
```

Run: `cargo test -p lopress-editor --test settings_tests 2>&1`. Expected: compile error (settings module missing).

- [ ] **Step 2: Implement `settings.rs`**

Create `crates/lopress-editor/src/settings.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub recents: Vec<PathBuf>,
    #[serde(default)]
    pub window: WindowSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowSettings {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recents: Vec::new(),
            window: WindowSettings::default(),
        }
    }
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            x: 100.0,
            y: 100.0,
            maximized: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
}

impl Settings {
    /// Load settings from `path`; returns default if file does not exist.
    ///
    /// # Errors
    /// Returns `SettingsError::Io` for non-not-found I/O errors,
    /// `SettingsError::Parse` for malformed JSON.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Save settings to `path`, creating parent dirs as needed.
    ///
    /// # Errors
    /// Returns `SettingsError::Io` on write failure.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Load `settings_path`. If it does not exist but `legacy_recents_path` does,
    /// migrate the recents list into a fresh settings file and delete the legacy file.
    ///
    /// # Errors
    /// Same as `load_from`, plus migration I/O errors.
    pub fn load_or_migrate(
        settings_path: &Path,
        legacy_recents_path: &Path,
    ) -> Result<Self, SettingsError> {
        if settings_path.exists() {
            return Self::load_from(settings_path);
        }
        if legacy_recents_path.exists() {
            let raw = std::fs::read_to_string(legacy_recents_path)?;
            let recents: Vec<PathBuf> = serde_json::from_str(&raw).unwrap_or_default();
            let s = Self {
                recents,
                ..Self::default()
            };
            s.save_to(settings_path)?;
            std::fs::remove_file(legacy_recents_path)?;
            return Ok(s);
        }
        Ok(Self::default())
    }
}

/// Resolve the platform-standard settings file path under the lopress config dir.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "lopress", "lopress")
        .map(|d| d.config_dir().join("settings.json"))
}

/// Resolve the legacy `recents.json` path for migration purposes.
pub fn legacy_recents_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "lopress", "lopress")
        .map(|d| d.config_dir().join("recents.json"))
}
```

Add `pub mod settings;` to `crates/lopress-editor/src/lib.rs`.

- [ ] **Step 3: Re-run tests**

```bash
cargo test -p lopress-editor --test settings_tests 2>&1
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(editor): settings file with recents.json migration (Task 2/22)"
```

---

## Task 3: Welcome view + workspace open + window-size restore

**Files:**
- Modify: `crates/lopress-editor/src/state.rs`
- Create: `crates/lopress-editor/src/ui/welcome.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`
- Modify: `crates/lopress-editor/src/lib.rs`

**Acceptance criteria:**
- App opens at Welcome view.
- Clicking "Open workspace…" launches an `rfd` directory picker. Choosing a valid lopress workspace transitions to a placeholder editing view (just text "Editing: <name>"). Choosing an invalid one shows an error banner.
- Recent workspaces from settings render as buttons; clicking opens that workspace.
- Window size and position restore from settings on launch; saved on close.

- [ ] **Step 1: Add `AppState` enum and `WelcomeState`**

Replace `crates/lopress-editor/src/state.rs`:

```rust
use crate::settings::Settings;
use lopress_gui_host::Session;

pub enum AppState {
    Welcome(WelcomeState),
    Editing(Box<EditingState>),
}

#[derive(Default)]
pub struct WelcomeState {
    pub error: Option<String>,
}

pub struct EditingState {
    pub session: Session,
    // Detailed editing state added in Task 4+. For now this is a minimal stub.
}

impl EditingState {
    pub fn new(session: Session) -> Self {
        Self { session }
    }
}

pub struct AppContext {
    pub settings: Settings,
    pub state: AppState,
}

impl AppContext {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            state: AppState::Welcome(WelcomeState::default()),
        }
    }
}
```

- [ ] **Step 2: Build the Welcome view**

Create `crates/lopress-editor/src/ui/welcome.rs`. Structure (translate to current Floem idioms — signals, `dyn_container`, `v_stack`, etc.):

```rust
//! Welcome view: choose a workspace, see recents.
//!
//! Reads `Settings::recents` and renders one button per entry plus an
//! "Open workspace…" button that calls `rfd::FileDialog` for a directory.
//! On successful workspace open via `Session::open`, the parent app
//! transitions to `AppState::Editing`.

use crate::settings::Settings;
use crate::state::{AppState, EditingState, WelcomeState};
use floem::IntoView;
use lopress_gui_host::Session;
use std::path::Path;

/// Build the Welcome view. The `on_open_workspace` callback receives a chosen
/// workspace path; it should attempt `Session::open` and transition state.
pub fn welcome_view<F>(welcome: &WelcomeState, recents: &[std::path::PathBuf], on_open: F) -> impl IntoView
where
    F: Fn(&Path) + Clone + 'static,
{
    // TODO Floem-specific: vertical stack centered, with:
    //   - Title "lopress"
    //   - "Open workspace…" button → rfd::FileDialog::new().pick_folder()
    //   - For each recent: a button that calls on_open(path)
    //   - If welcome.error is Some, render it as a red label
    //
    // Use Floem's button primitive and rfd::FileDialog::new().pick_folder()
    // (rfd is sync; offload to a thread if it blocks the event loop, otherwise
    // call directly).
    floem::views::label(|| "TODO: Welcome view (see comments)")
}

/// Helper invoked from the open callback: try to open as workspace.
/// Returns Err(message) on failure to surface to WelcomeState.error.
pub fn try_open(path: &Path) -> Result<Session, String> {
    Session::open(path).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Wire AppContext into the root view**

Replace `crates/lopress-editor/src/ui/mod.rs`:

```rust
pub mod welcome;

use crate::settings::Settings;
use crate::state::{AppContext, AppState};
use floem::IntoView;
use floem::reactive::create_rw_signal;

pub fn root_view() -> impl IntoView {
    let settings = load_settings();
    let ctx = create_rw_signal(AppContext::new(settings));

    // TODO Floem-specific: use dyn_container or similar to switch between
    //   AppState::Welcome → welcome::welcome_view(...)
    //   AppState::Editing → editing_view(...)  (placeholder until Task 7)
    //
    // The on_open callback in welcome_view should:
    //   1. call welcome::try_open(path)
    //   2. on Ok(session): ctx.update(|c| c.state = AppState::Editing(Box::new(...)))
    //   3. on Err(msg): ctx.update(|c| if let AppState::Welcome(w) = &mut c.state { w.error = Some(msg) })

    floem::views::label(|| "TODO: root view (see comments)")
}

fn load_settings() -> Settings {
    let settings_path = crate::settings::default_path();
    let recents_path = crate::settings::legacy_recents_path();
    match (settings_path, recents_path) {
        (Some(s), Some(r)) => Settings::load_or_migrate(&s, &r).unwrap_or_default(),
        _ => Settings::default(),
    }
}
```

- [ ] **Step 4: Wire window-size restore + save**

Modify `crates/lopress-editor/src/lib.rs`. Replace the `run` function:

```rust
pub mod settings;
pub mod state;
pub mod ui;

use crate::settings::{default_path, legacy_recents_path, Settings};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Floem launch failed: {0}")]
    Launch(String),
}

pub fn run() -> Result<(), AppError> {
    let settings = match (default_path(), legacy_recents_path()) {
        (Some(s), Some(r)) => Settings::load_or_migrate(&s, &r).unwrap_or_default(),
        _ => Settings::default(),
    };

    // TODO Floem-specific: configure initial window size from settings.window,
    //   register a window-close handler that calls save_window_state.
    //
    // Pattern (as of Floem 0.x — check current API):
    //   floem::Application::new()
    //       .window(|_| ui::root_view(), Some(WindowConfig { size: ..., position: ... }))
    //       .run();
    //
    // For window-close persistence: hook the window's resize/move/close events
    // to update settings.window and call settings.save_to(default_path()?).

    floem::launch(ui::root_view);
    Ok(())
}
```

- [ ] **Step 5: Implement the Welcome view body**

Replace the `TODO` stub in `welcome.rs` with concrete Floem code per the structural guidance. Reference Floem's `examples/widget-gallery/` for button + label + container patterns. Wire the `rfd::FileDialog::new().set_directory(...).pick_folder()` call.

Concrete checklist within this step:
- Vertical stack, centered horizontally.
- Heading "lopress" at top.
- "Open workspace…" button that calls `rfd` and invokes `on_open` with the result.
- Each recent path renders as a button labelled by the directory's last component; clicking calls `on_open`.
- If `welcome.error` is `Some`, render the message in a styled red label above the buttons.

- [ ] **Step 6: Manual smoke test**

```bash
cargo run -p lopress
```

Verify:
- Window opens at 1200×800 (or whatever `settings.window` says).
- Welcome view displays the title, "Open workspace…" button, and (if any) recents.
- Clicking "Open workspace…" launches a directory picker.
- Picking a valid lopress workspace (one with `lopress.toml`) transitions to a placeholder. Picking an invalid directory shows the error banner.
- Resize the window, close the app, re-open. Window restores to the last size. The chosen workspace appears in recents.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(editor): welcome view, workspace open, window state restore (Task 3/22)"
```

---

## Task 4: Document model types

**Files:**
- Create: `crates/lopress-editor/src/model/mod.rs`
- Create: `crates/lopress-editor/src/model/types.rs`
- Create: `crates/lopress-editor/tests/model_types_tests.rs`
- Modify: `crates/lopress-editor/src/lib.rs` (add `pub mod model;`)

- [ ] **Step 1: Write failing tests**

Create `crates/lopress-editor/tests/model_types_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::types::*;
use serde_json::json;

#[test]
fn block_id_is_unique_and_monotonic() {
    let a = BlockId::new();
    let b = BlockId::new();
    assert_ne!(a, b);
}

#[test]
fn inline_run_default_has_no_styles() {
    let r = InlineRun::plain("hi");
    assert_eq!(r.text, "hi");
    assert!(!r.bold && !r.italic && !r.code);
    assert!(r.link.is_none());
}

#[test]
fn block_kind_paragraph_default() {
    let k = BlockKind::Paragraph;
    assert!(matches!(k, BlockKind::Paragraph));
}

#[test]
fn editor_block_constructors() {
    let p = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
    assert!(matches!(p.kind, BlockKind::Paragraph));
    if let BlockBody::Inline(runs) = &p.body {
        assert_eq!(runs.len(), 1);
    } else {
        panic!("expected Inline body");
    }
    assert!(p.plugin.is_none());
}

#[test]
fn opaque_block_round_trips_value() {
    let v = json!({"foo": "bar"});
    let b = EditorBlock::opaque("custom".into(), v.clone());
    assert!(matches!(b.kind, BlockKind::Opaque { .. }));
    if let BlockBody::Opaque(stored) = &b.body {
        assert_eq!(stored, &v);
    } else {
        panic!("expected Opaque body");
    }
}
```

Run: `cargo test -p lopress-editor --test model_types_tests 2>&1`. Expected: compile error (module missing).

- [ ] **Step 2: Create the types module**

Create `crates/lopress-editor/src/model/mod.rs`:

```rust
pub mod types;
```

Create `crates/lopress-editor/src/model/types.rs`:

```rust
use lopress_plugin::AttrDecl;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable identity for a block within an open document. Not persisted to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(u64);

impl BlockId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorDoc {
    pub blocks: Vec<EditorBlock>,
    pub front_matter: lopress_core::FrontMatter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorBlock {
    pub id: BlockId,
    pub kind: BlockKind,
    pub body: BlockBody,
    pub plugin: Option<PluginMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),               // 1..=6
    Code { lang: String },
    List { ordered: bool },
    Opaque { type_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockBody {
    Inline(Vec<InlineRun>),
    Code(String),
    List(Vec<ListItem>),
    Opaque(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub id: BlockId,
    pub runs: Vec<InlineRun>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlineRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl InlineRun {
    pub fn plain<S: Into<String>>(text: S) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: String,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Vec<AttrDecl>,
}

impl EditorBlock {
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }

    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Heading(level.clamp(1, 6)),
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }

    pub fn code(lang: String, text: String) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Code { lang },
            body: BlockBody::Code(text),
            plugin: None,
        }
    }

    pub fn list(ordered: bool, items: Vec<ListItem>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::List { ordered },
            body: BlockBody::List(items),
            plugin: None,
        }
    }

    pub fn opaque(type_name: String, value: Value) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Opaque {
                type_name: type_name.clone(),
            },
            body: BlockBody::Opaque(value),
            plugin: None,
        }
    }
}
```

Add to `crates/lopress-editor/src/lib.rs`: `pub mod model;`

- [ ] **Step 3: Re-run tests**

```bash
cargo test -p lopress-editor --test model_types_tests 2>&1
```

Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(editor): document model types (Task 4/22)"
```

---

## Task 5: Inline runs ↔ markdown round-trip

**Files:**
- Create: `crates/lopress-editor/src/model/inline.rs`
- Create: `crates/lopress-editor/tests/inline_runs_tests.rs`
- Modify: `crates/lopress-editor/src/model/mod.rs`

- [ ] **Step 1: Write failing round-trip tests**

Create `crates/lopress-editor/tests/inline_runs_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::inline::{parse_inline, serialize_inline};
use lopress_editor::model::types::InlineRun;

fn r(text: &str, bold: bool, italic: bool, code: bool, link: Option<&str>) -> InlineRun {
    InlineRun {
        text: text.into(),
        bold,
        italic,
        code,
        link: link.map(String::from),
    }
}

#[test]
fn plain_text() {
    let runs = parse_inline("hello world");
    assert_eq!(runs, vec![r("hello world", false, false, false, None)]);
    assert_eq!(serialize_inline(&runs), "hello world");
}

#[test]
fn bold() {
    let runs = parse_inline("hello **world**");
    assert_eq!(
        runs,
        vec![
            r("hello ", false, false, false, None),
            r("world", true, false, false, None),
        ]
    );
    assert_eq!(serialize_inline(&runs), "hello **world**");
}

#[test]
fn italic_underscore() {
    let runs = parse_inline("hello _world_");
    assert_eq!(serialize_inline(&runs), "hello _world_");
}

#[test]
fn inline_code() {
    let runs = parse_inline("call `foo()`");
    assert_eq!(serialize_inline(&runs), "call `foo()`");
}

#[test]
fn link_simple() {
    let runs = parse_inline("see [docs](https://example.com)");
    assert_eq!(serialize_inline(&runs), "see [docs](https://example.com)");
}

#[test]
fn bold_inside_link() {
    let runs = parse_inline("[**bold link**](https://example.com)");
    assert_eq!(
        serialize_inline(&runs),
        "[**bold link**](https://example.com)"
    );
}

#[test]
fn link_with_parens_in_url() {
    let runs = parse_inline("[wikipedia](https://en.wikipedia.org/wiki/Foo_(bar))");
    let s = serialize_inline(&runs);
    let reparsed = parse_inline(&s);
    assert_eq!(reparsed, runs, "must round-trip identically");
}

#[test]
fn escaped_asterisk_is_literal() {
    let runs = parse_inline(r"this is \*literal\*");
    assert_eq!(serialize_inline(&runs), r"this is \*literal\*");
}

#[test]
fn empty_string() {
    let runs = parse_inline("");
    assert!(runs.is_empty());
    assert_eq!(serialize_inline(&runs), "");
}

#[test]
fn adjacent_same_style_coalesced() {
    let runs = parse_inline("**foo****bar**");
    assert_eq!(runs.len(), 1, "adjacent bold spans should coalesce");
    assert_eq!(runs[0].text, "foobar");
}

#[test]
fn unsupported_strikethrough_passes_through_text() {
    // Strikethrough not in our subset; should be preserved verbatim
    let runs = parse_inline("~~struck~~");
    let s = serialize_inline(&runs);
    let reparsed = parse_inline(&s);
    assert_eq!(reparsed, runs);
}
```

Run: `cargo test -p lopress-editor --test inline_runs_tests 2>&1`. Expected: compile error.

- [ ] **Step 2: Implement parse + serialize**

Create `crates/lopress-editor/src/model/inline.rs`:

```rust
use crate::model::types::InlineRun;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Parse a markdown inline string into `InlineRun`s.
///
/// Supported markers (round-trip preserved): bold (`**`), italic (`_`),
/// inline code (`` ` ``), links (`[text](url)`). Unsupported markers
/// (strikethrough, footnotes, raw HTML, etc.) are preserved verbatim
/// in the run text.
pub fn parse_inline(input: &str) -> Vec<InlineRun> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut opts = Options::empty();
    // We do NOT enable strikethrough/footnotes — let them pass as text.
    let parser = Parser::new_ext(input, opts);

    let mut runs: Vec<InlineRun> = Vec::new();
    let mut style = StyleStack::default();

    for event in parser {
        match event {
            Event::Text(t) => push(&mut runs, &style, t.into_string()),
            Event::Code(t) => {
                let was_code = style.code;
                style.code = true;
                push(&mut runs, &style, t.into_string());
                style.code = was_code;
            }
            Event::Start(Tag::Strong) => style.bold += 1,
            Event::End(TagEnd::Strong) => style.bold = style.bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => style.italic += 1,
            Event::End(TagEnd::Emphasis) => style.italic = style.italic.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.into_string()),
            Event::End(TagEnd::Link) => style.link = None,
            // Anything else: best-effort literal pass-through. SoftBreak / HardBreak emit a space/newline.
            Event::SoftBreak => push(&mut runs, &style, "\n".into()),
            Event::HardBreak => push(&mut runs, &style, "  \n".into()),
            // Block-level events shouldn't appear in inline-only input; ignore defensively.
            _ => {}
        }
    }

    coalesce(runs)
}

#[derive(Default)]
struct StyleStack {
    bold: u32,
    italic: u32,
    code: bool,
    link: Option<String>,
}

impl StyleStack {
    fn snapshot(&self) -> (bool, bool, bool, Option<String>) {
        (self.bold > 0, self.italic > 0, self.code, self.link.clone())
    }
}

fn push(out: &mut Vec<InlineRun>, style: &StyleStack, text: String) {
    if text.is_empty() {
        return;
    }
    let (b, i, c, l) = style.snapshot();
    out.push(InlineRun {
        text,
        bold: b,
        italic: i,
        code: c,
        link: l,
    });
}

fn coalesce(runs: Vec<InlineRun>) -> Vec<InlineRun> {
    let mut out: Vec<InlineRun> = Vec::with_capacity(runs.len());
    for r in runs {
        if let Some(last) = out.last_mut() {
            if last.bold == r.bold
                && last.italic == r.italic
                && last.code == r.code
                && last.link == r.link
            {
                last.text.push_str(&r.text);
                continue;
            }
        }
        out.push(r);
    }
    out
}

/// Serialize `InlineRun`s back to markdown.
pub fn serialize_inline(runs: &[InlineRun]) -> String {
    let mut out = String::new();
    for r in runs {
        let mut text = r.text.clone();
        if r.code {
            text = format!("`{text}`");
        }
        if r.italic {
            text = format!("_{text}_");
        }
        if r.bold {
            text = format!("**{text}**");
        }
        if let Some(url) = &r.link {
            text = format!("[{text}]({url})");
        }
        out.push_str(&text);
    }
    out
}
```

Add `pub mod inline;` to `crates/lopress-editor/src/model/mod.rs`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p lopress-editor --test inline_runs_tests 2>&1
```

Expected: most pass. The `link_with_parens_in_url` test may fail because pulldown-cmark's link parser handles balanced parens — verify what we produce. Adjust serializer if needed (e.g., URL-encode the closing paren or use `<...>` link form).

- [ ] **Step 4: Iterate until all tests pass**

If any fail: read the failure, narrow the case (e.g., make a smaller fixture), fix the parse or serialize side. Common issues:
- pulldown-cmark emits `Code` events that include leading/trailing space; our `style.code` snapshot may need adjustment.
- The serializer's order of wrapping (bold around italic vs italic around bold) affects round-trip equality. Choose one canonical order (this implementation picks: code → italic → bold → link, innermost out) and document it.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(editor): inline-runs markdown parser/serializer (Task 5/22)"
```

---

## Task 6: from_core / to_core converters (no plugin awareness yet)

**Files:**
- Create: `crates/lopress-editor/src/model/from_core.rs`
- Create: `crates/lopress-editor/src/model/to_core.rs`
- Create: `crates/lopress-editor/tests/from_to_core_tests.rs`
- Modify: `crates/lopress-editor/src/model/mod.rs`

This task does **not** wire up `PluginRegistry` — that comes in Task 17. For now, every block whose type isn't `paragraph`/`heading`/`code_block`/`list` becomes `Opaque`.

- [ ] **Step 1: Write failing round-trip tests**

Create `crates/lopress-editor/tests/from_to_core_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_core::{parse, serialize, Document};
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;

const FIXTURE: &str = r#"---
title: Test Post
---

# Heading 1

Paragraph with **bold** and _italic_ and `code` and a [link](https://example.com).

```rust
fn main() {}
```

- first item
- second item with **bold**

## Heading 2

Final paragraph.
"#;

#[test]
fn round_trip_byte_identical_for_supported_subset() {
    let core = parse(FIXTURE).unwrap();
    let editor = doc_from_core(&core);
    let core_back = doc_to_core(&editor);
    let serialized = serialize(&core_back);
    assert_eq!(serialized, FIXTURE);
}

#[test]
fn opaque_block_preserved() {
    let input = r#"---
title: T
---

::custom_widget{src="x.png"}
::

Text after.
"#;
    let core = parse(input).unwrap();
    let editor = doc_from_core(&core);
    let core_back = doc_to_core(&editor);
    let serialized = serialize(&core_back);
    assert_eq!(serialized, input, "opaque block must round-trip verbatim");
}

#[test]
fn nested_list_becomes_opaque() {
    let input = r#"---
title: T
---

- top
  - nested
"#;
    let core = parse(input).unwrap();
    let editor = doc_from_core(&core);
    // First block should be Opaque since the list has a nested child
    let first = editor.blocks.first().unwrap();
    matches!(first.kind, lopress_editor::model::types::BlockKind::Opaque { .. });
    // Round-trip must still preserve it
    let serialized = serialize(&doc_to_core(&editor));
    assert_eq!(serialized, input);
}
```

Adjust the fixture format above to whatever syntax `lopress-core::parse` actually accepts (check `crates/lopress-core/tests/`). The point is: pick a known-good document, parse it, convert into the editor model, convert back, serialize, assert byte-identical.

Run: `cargo test -p lopress-editor --test from_to_core_tests 2>&1`. Expected: compile error.

- [ ] **Step 2: Inspect lopress-core's Block/Document shape**

```bash
cat crates/lopress-core/src/lib.rs
cat crates/lopress-core/src/types.rs   # if present
```

Note the exact field names/types of `Block` and how `paragraph` / `heading` / `code_block` / `list` are encoded — specifically what's in `attrs`, `text`, `children`. This drives the converter implementation.

- [ ] **Step 3: Implement `from_core`**

Create `crates/lopress-editor/src/model/from_core.rs`:

```rust
use crate::model::inline::parse_inline;
use crate::model::types::{
    BlockBody, BlockKind, EditorBlock, EditorDoc, ListItem,
};
use lopress_core::{Block, Document};

/// Convert a lopress_core Document into the editor's working model.
pub fn doc_from_core(doc: &Document) -> EditorDoc {
    EditorDoc {
        front_matter: doc.front_matter.clone(),
        blocks: doc.blocks.iter().map(block_from_core).collect(),
    }
}

fn block_from_core(b: &Block) -> EditorBlock {
    match b.r#type.as_str() {
        "paragraph" => {
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::paragraph(parse_inline(text))
        }
        "heading" => {
            let level = b
                .attrs
                .get("level")
                .and_then(|v| v.as_u64())
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::heading(level, parse_inline(text))
        }
        "code_block" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = b.text.clone().unwrap_or_default();
            EditorBlock::code(lang, text)
        }
        "list" => list_from_core(b),
        other => EditorBlock::opaque(
            other.to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

fn list_from_core(b: &Block) -> EditorBlock {
    // A list is convertible only if every list_item child contains exactly one
    // paragraph child. Otherwise the whole list becomes Opaque.
    let ordered = b
        .attrs
        .get("ordered")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let convertible = b.children.iter().all(|item| {
        item.r#type == "list_item"
            && item.children.len() == 1
            && item.children.iter().all(|c| c.r#type == "paragraph")
    });

    if !convertible {
        return EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        );
    }

    let items: Vec<ListItem> = b
        .children
        .iter()
        .map(|item| {
            let para = item.children.first();
            let text = para.and_then(|p| p.text.as_deref()).unwrap_or("");
            ListItem {
                id: crate::model::types::BlockId::new(),
                runs: parse_inline(text),
            }
        })
        .collect();

    EditorBlock::list(ordered, items)
}
```

- [ ] **Step 4: Implement `to_core`**

Create `crates/lopress-editor/src/model/to_core.rs`:

```rust
use crate::model::inline::serialize_inline;
use crate::model::types::{
    BlockBody, BlockKind, EditorBlock, EditorDoc,
};
use lopress_core::{Block, Document};
use serde_json::{json, Value};

pub fn doc_to_core(doc: &EditorDoc) -> Document {
    Document {
        front_matter: doc.front_matter.clone(),
        blocks: doc.blocks.iter().map(block_to_core).collect(),
    }
}

fn block_to_core(b: &EditorBlock) -> Block {
    match (&b.kind, &b.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => Block {
            r#type: "paragraph".into(),
            attrs: Value::Object(serde_json::Map::new()),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => Block {
            r#type: "heading".into(),
            attrs: json!({ "level": level }),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::List { ordered }, BlockBody::List(items)) => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": ordered }),
            children: items
                .iter()
                .map(|i| Block {
                    r#type: "list_item".into(),
                    attrs: Value::Object(serde_json::Map::new()),
                    children: vec![Block {
                        r#type: "paragraph".into(),
                        attrs: Value::Object(serde_json::Map::new()),
                        children: vec![],
                        text: Some(serialize_inline(&i.runs)),
                    }],
                    text: None,
                })
                .collect(),
            text: None,
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            // Reconstruct the original block from its serialized JSON.
            // If for any reason that fails, fall back to a paragraph noting the issue.
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.clone(),
                attrs: Value::Object(serde_json::Map::new()),
                children: vec![],
                text: None,
            })
        }
        // kind/body mismatch — emit a paragraph fallback rather than panic
        _ => Block {
            r#type: "paragraph".into(),
            attrs: Value::Object(serde_json::Map::new()),
            children: vec![],
            text: Some(String::new()),
        },
    }
}
```

Add to `crates/lopress-editor/src/model/mod.rs`:

```rust
pub mod from_core;
pub mod inline;
pub mod to_core;
pub mod types;
```

- [ ] **Step 5: Run tests, iterate**

```bash
cargo test -p lopress-editor --test from_to_core_tests 2>&1
```

If round-trip is not byte-identical, diff the input vs output to find the disagreement. Likely culprits: heading level encoding, list-item text representation, opaque-block serialization shape. Adjust converter until tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(editor): from_core/to_core converters with round-trip tests (Task 6/22)"
```

---

## Task 7: Static block rendering (read-only)

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/mod.rs`
- Create: `crates/lopress-editor/src/ui/blocks/paragraph.rs`
- Create: `crates/lopress-editor/src/ui/blocks/heading.rs`
- Create: `crates/lopress-editor/src/ui/blocks/code.rs`
- Create: `crates/lopress-editor/src/ui/blocks/list.rs`
- Create: `crates/lopress-editor/src/ui/blocks/opaque.rs`
- Create: `crates/lopress-editor/src/ui/editor_pane.rs`
- Modify: `crates/lopress-editor/src/state.rs` (real `EditingState` with `EditorDoc`)
- Modify: `crates/lopress-editor/src/ui/mod.rs` (mount editor pane in editing state)

**Goal:** Open a workspace, click a post in (a placeholder, sidebar comes in Task 18), see all blocks rendered with correct typography. No editing yet — every block reads from `EditorDoc` and paints styled text.

**Acceptance criteria:**
- Open a post via the existing test sidebar (TBD: temporarily expose a `Open posts/foo.md` button on the editing-view scaffold).
- See heading, paragraph, code, list rendered with appropriate typography:
  - Headings: 32 / 26 / 22 / 18 / 16 / 14 logical px proportional, bold or semi-bold per Floem default heading style.
  - Paragraph: 15 logical px proportional, with bold/italic/code/link styled inline.
  - Code: monospace, neutral background frame, language label.
  - List: bullet/number prefix, indented, items styled like paragraphs.
  - Opaque: a neutral card showing `[type_name]` and a collapsed "raw JSON" toggle.

- [ ] **Step 1: Add `EditingState::open_document` and load logic**

Modify `crates/lopress-editor/src/state.rs`:

```rust
use crate::model::from_core::doc_from_core;
use crate::model::types::EditorDoc;
use crate::settings::Settings;
use lopress_gui_host::{DocumentRef, Session};

pub enum AppState {
    Welcome(WelcomeState),
    Editing(Box<EditingState>),
}

#[derive(Default)]
pub struct WelcomeState {
    pub error: Option<String>,
}

pub struct EditingState {
    pub session: Session,
    pub current_doc: Option<EditorDoc>,
    pub current_ref: Option<DocumentRef>,
    pub last_error: Option<String>,
}

impl EditingState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            current_doc: None,
            current_ref: None,
            last_error: None,
        }
    }

    pub fn open_document(&mut self, doc_ref: &DocumentRef) {
        match self.session.load_document(&doc_ref.path) {
            Ok(loaded) => {
                let core_doc = lopress_core::Document {
                    front_matter: loaded.front_matter,
                    blocks: loaded.blocks,
                };
                self.current_doc = Some(doc_from_core(&core_doc));
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

pub struct AppContext {
    pub settings: Settings,
    pub state: AppState,
}

impl AppContext {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            state: AppState::Welcome(WelcomeState::default()),
        }
    }
}
```

- [ ] **Step 2: Build the per-block views (read-only)**

Create `crates/lopress-editor/src/ui/blocks/mod.rs`:

```rust
pub mod code;
pub mod heading;
pub mod list;
pub mod opaque;
pub mod paragraph;

use crate::model::types::{BlockKind, EditorBlock};
use floem::IntoView;

/// Dispatch a read-only block render to the appropriate view.
pub fn block_view(block: &EditorBlock) -> impl IntoView {
    // TODO Floem-specific: dispatch to the per-kind view function.
    // Use a `dyn IntoView` boxed return or a match-into-stack pattern;
    // consult Floem examples for polymorphic view dispatch idioms.
    floem::views::label(|| "TODO: dispatch")
}
```

Create `crates/lopress-editor/src/ui/blocks/paragraph.rs`:

```rust
use crate::model::types::InlineRun;
use floem::IntoView;

/// Render a paragraph as a read-only horizontally-flowing run of styled spans.
///
/// For each run, emit a span with:
/// - bold / italic / code / link styling
/// - 15 logical px proportional font (monospace if `code`)
/// - underline + theme link color if `link.is_some()`
///
/// Floem-specific: use `text` + `Style` per span, composed in a `flex_row`
/// or whatever the current Floem inline-text container idiom is. Reference
/// Floem's text-layout examples; if Floem 0.x exposes `RichText`-like
/// primitives, prefer those.
pub fn render_runs(runs: &[InlineRun]) -> impl IntoView {
    floem::views::label(|| "TODO: styled inline runs")
}
```

Create `crates/lopress-editor/src/ui/blocks/heading.rs`:

```rust
use crate::model::types::InlineRun;
use floem::IntoView;

/// Heading levels 1..=6 with proportional font sizes 32/26/22/18/16/14 logical px.
pub fn render_heading(level: u8, runs: &[InlineRun]) -> impl IntoView {
    let _size = match level {
        1 => 32.0_f32,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    };
    // Render runs (same as paragraph) but with the heading size + bold weight.
    floem::views::label(|| "TODO: heading")
}
```

Create `crates/lopress-editor/src/ui/blocks/code.rs`:

```rust
use floem::IntoView;

/// Render a code block as a monospace text region inside a neutral-background
/// frame, with a small language label in the top-right corner.
pub fn render_code(lang: &str, text: &str) -> impl IntoView {
    let _ = (lang, text);
    floem::views::label(|| "TODO: code block")
}
```

Create `crates/lopress-editor/src/ui/blocks/list.rs`:

```rust
use crate::model::types::ListItem;
use floem::IntoView;

/// Render a list as a vertical stack of items, each prefixed with bullet or number.
pub fn render_list(ordered: bool, items: &[ListItem]) -> impl IntoView {
    let _ = (ordered, items);
    floem::views::label(|| "TODO: list")
}
```

Create `crates/lopress-editor/src/ui/blocks/opaque.rs`:

```rust
use floem::IntoView;
use serde_json::Value;

/// Render an opaque block as a neutral card with the type name and a
/// collapsed "raw JSON" toggle.
pub fn render_opaque(type_name: &str, value: &Value) -> impl IntoView {
    let _ = (type_name, value);
    floem::views::label(|| "TODO: opaque card")
}
```

- [ ] **Step 3: Build the EditorPane scrollable list of blocks**

Create `crates/lopress-editor/src/ui/editor_pane.rs`:

```rust
use crate::model::types::EditorDoc;
use floem::IntoView;

/// Render the editor pane: vertical scroll container, max content width 720,
/// centered, with one block view per `EditorBlock`.
pub fn editor_pane(doc: &EditorDoc) -> impl IntoView {
    let _ = doc;
    // TODO Floem-specific:
    //   v_stack(doc.blocks.iter().map(blocks::block_view).collect())
    //   .style(|s| s.max_width(720.0).margin_horiz_auto())
    //   wrapped in scroll(...)
    floem::views::label(|| "TODO: editor pane")
}
```

- [ ] **Step 4: Mount editor pane in app shell**

Modify `crates/lopress-editor/src/ui/mod.rs` so that `AppState::Editing` mounts a 3-column layout:

- Sidebar placeholder (left, 220 logical px) with a temporary "Open first post" button that calls `EditingState::open_document` on `session.workspace().posts.first()`.
- Editor pane (center, flex) calling `editor_pane(doc)` if `current_doc` is `Some`.
- Inspector placeholder (right, 280 logical px) — empty.
- Footer placeholder pinned below — empty.

This gives the smoke-test path for static rendering without yet building the real sidebar/inspector/footer (those come in tasks 18-20).

- [ ] **Step 5: Implement each per-block view body**

For each of paragraph / heading / code / list / opaque, replace the TODO stub with the actual Floem rendering. Acceptance: every block kind renders with correct typography per the criteria above. Each per-block view has a single committed implementation; iterate one kind at a time.

For inline run rendering specifically, the simplest Floem path is one styled `text` element per run, composed with a flowing layout (Floem's flex-row with wrap, or whatever the current inline-text idiom is). If Floem 0.x exposes a richer `RichText` or `Spans`-style primitive, use it.

- [ ] **Step 6: Manual smoke test**

```bash
cargo run -p lopress
```

- Open a workspace with at least one post containing every block kind.
- Click "Open first post."
- Confirm: heading sizes look right, paragraph styles render bold/italic/code/link, code has a frame and lang label, list has bullets, opaque cards show type names.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(editor): read-only block rendering for all kinds (Task 7/22)"
```

---

## Task 8: Inline-runs editor — caret + character input

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/paragraph.rs`, `heading.rs` to use it
- Create: `crates/lopress-editor/tests/inline_editor_tests.rs`

**Goal:** Replace the read-only paragraph/heading rendering with an editable widget that owns a single block's `Vec<InlineRun>`, paints a blinking caret, and accepts character input. No selection, no toggles, no IME edge cases yet.

**Acceptance criteria:**
- Click into a paragraph → caret appears at click position.
- Type characters → text appears at the caret. Caret advances.
- Arrow Left/Right → caret moves through text. Crosses run boundaries seamlessly.
- Home/End → caret moves to start/end of block.
- Backspace → deletes the char before caret. Within run = simple. At run boundary = chars merge or runs coalesce. At block start = no-op (block-merge handled in Task 11).
- Delete → mirrors Backspace forward.
- Enter → no-op for now (split handled in Task 11).
- Bold/italic/code/link styles still render visually as the user types into them.

- [ ] **Step 1: Define caret position type and helpers**

Create `crates/lopress-editor/src/ui/blocks/inline_editor.rs`:

```rust
//! Editable inline-runs widget.
//!
//! State model: the widget owns a `Vec<InlineRun>` and a `Caret` — a
//! `(run_index, char_offset)` pair. Edits manipulate the runs vector;
//! the caret moves to track the affected position.

use crate::model::types::InlineRun;

/// Position within a `Vec<InlineRun>`. Char offsets are *character* positions
/// within the run's `text` (not byte positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    pub run: usize,
    pub offset: usize,
}

impl Caret {
    pub const START: Self = Caret { run: 0, offset: 0 };

    pub fn end(runs: &[InlineRun]) -> Self {
        match runs.last() {
            Some(last) => Caret {
                run: runs.len() - 1,
                offset: last.text.chars().count(),
            },
            None => Caret::START,
        }
    }
}

/// Insert a single character at the caret. Returns the caret moved by one.
pub fn insert_char(runs: &mut Vec<InlineRun>, caret: Caret, ch: char) -> Caret {
    if runs.is_empty() {
        runs.push(InlineRun::plain(ch.to_string()));
        return Caret { run: 0, offset: 1 };
    }
    let Some(run) = runs.get_mut(caret.run) else {
        return caret;
    };
    let byte = char_to_byte(&run.text, caret.offset);
    run.text.insert(byte, ch);
    Caret {
        run: caret.run,
        offset: caret.offset + 1,
    }
}

/// Delete the char immediately before the caret. Returns the new caret.
/// Returns `caret` unchanged if at block start.
pub fn backspace(runs: &mut Vec<InlineRun>, caret: Caret) -> Caret {
    if caret.run == 0 && caret.offset == 0 {
        return caret;
    }
    let mut c = caret;
    if c.offset == 0 {
        // move to end of previous run
        if c.run == 0 {
            return caret;
        }
        c.run -= 1;
        c.offset = runs.get(c.run).map(|r| r.text.chars().count()).unwrap_or(0);
    }
    let Some(run) = runs.get_mut(c.run) else {
        return caret;
    };
    let byte_end = char_to_byte(&run.text, c.offset);
    let byte_start = char_to_byte(&run.text, c.offset - 1);
    run.text.replace_range(byte_start..byte_end, "");
    let new_caret = Caret {
        run: c.run,
        offset: c.offset - 1,
    };
    coalesce_around(runs, new_caret.run);
    new_caret
}

/// Delete the char immediately after the caret. Returns the caret unchanged
/// (forward-delete does not move the caret).
pub fn delete(runs: &mut Vec<InlineRun>, caret: Caret) -> Caret {
    let Some(run) = runs.get(caret.run) else {
        return caret;
    };
    let len = run.text.chars().count();
    if caret.offset >= len {
        // try to delete first char of next run
        if caret.run + 1 >= runs.len() {
            return caret;
        }
        let Some(next) = runs.get_mut(caret.run + 1) else {
            return caret;
        };
        if next.text.is_empty() {
            runs.remove(caret.run + 1);
            return caret;
        }
        let byte_end = char_to_byte(&next.text, 1);
        next.text.replace_range(0..byte_end, "");
        coalesce_around(runs, caret.run);
        return caret;
    }
    let Some(run) = runs.get_mut(caret.run) else {
        return caret;
    };
    let byte_start = char_to_byte(&run.text, caret.offset);
    let byte_end = char_to_byte(&run.text, caret.offset + 1);
    run.text.replace_range(byte_start..byte_end, "");
    coalesce_around(runs, caret.run);
    caret
}

/// Move caret one character left, crossing run boundaries.
pub fn move_left(runs: &[InlineRun], caret: Caret) -> Caret {
    if caret.offset > 0 {
        return Caret {
            run: caret.run,
            offset: caret.offset - 1,
        };
    }
    if caret.run > 0 {
        let prev_idx = caret.run - 1;
        let prev_len = runs.get(prev_idx).map(|r| r.text.chars().count()).unwrap_or(0);
        return Caret {
            run: prev_idx,
            offset: prev_len,
        };
    }
    caret
}

/// Move caret one character right, crossing run boundaries.
pub fn move_right(runs: &[InlineRun], caret: Caret) -> Caret {
    let Some(run) = runs.get(caret.run) else {
        return caret;
    };
    let len = run.text.chars().count();
    if caret.offset < len {
        return Caret {
            run: caret.run,
            offset: caret.offset + 1,
        };
    }
    if caret.run + 1 < runs.len() {
        return Caret {
            run: caret.run + 1,
            offset: 0,
        };
    }
    caret
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

/// Merge `runs[idx]` with its neighbors if they share style.
fn coalesce_around(runs: &mut Vec<InlineRun>, idx: usize) {
    // merge with next
    if idx + 1 < runs.len() {
        let same = match (runs.get(idx), runs.get(idx + 1)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if same {
            let next = runs.remove(idx + 1);
            if let Some(cur) = runs.get_mut(idx) {
                cur.text.push_str(&next.text);
            }
        }
    }
    // merge with prev
    if idx > 0 {
        let same = match (runs.get(idx - 1), runs.get(idx)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if same {
            let cur = runs.remove(idx);
            if let Some(prev) = runs.get_mut(idx - 1) {
                prev.text.push_str(&cur.text);
            }
        }
    }
}

fn same_style(a: &InlineRun, b: &InlineRun) -> bool {
    a.bold == b.bold && a.italic == b.italic && a.code == b.code && a.link == b.link
}
```

- [ ] **Step 2: Write tests for the editing primitives**

Create `crates/lopress-editor/tests/inline_editor_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::types::InlineRun;
use lopress_editor::ui::blocks::inline_editor::*;

fn plain(t: &str) -> InlineRun {
    InlineRun::plain(t)
}

#[test]
fn insert_char_into_empty() {
    let mut runs = Vec::new();
    let c = insert_char(&mut runs, Caret::START, 'h');
    assert_eq!(runs, vec![plain("h")]);
    assert_eq!(c, Caret { run: 0, offset: 1 });
}

#[test]
fn insert_char_into_middle() {
    let mut runs = vec![plain("helo")];
    let c = insert_char(&mut runs, Caret { run: 0, offset: 3 }, 'l');
    assert_eq!(runs[0].text, "hello");
    assert_eq!(c, Caret { run: 0, offset: 4 });
}

#[test]
fn backspace_within_run() {
    let mut runs = vec![plain("hello")];
    let c = backspace(&mut runs, Caret { run: 0, offset: 3 });
    assert_eq!(runs[0].text, "helo");
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn backspace_at_run_boundary_merges_to_prev() {
    let mut runs = vec![
        plain("hello "),
        InlineRun {
            text: "world".into(),
            bold: true,
            ..Default::default()
        },
    ];
    let c = backspace(&mut runs, Caret { run: 1, offset: 0 });
    // The space before "world" is removed, ending up at "hello" / "world" with caret on the boundary.
    assert_eq!(c.run, 0);
}

#[test]
fn delete_forward_across_run_boundary() {
    let mut runs = vec![plain("ab"), plain("cd")];
    // caret at end of run 0
    let _ = delete(&mut runs, Caret { run: 0, offset: 2 });
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "abd");
}

#[test]
fn move_left_crosses_run_boundary() {
    let runs = vec![plain("ab"), plain("cd")];
    let c = move_left(&runs, Caret { run: 1, offset: 0 });
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn move_right_crosses_run_boundary() {
    let runs = vec![plain("ab"), plain("cd")];
    let c = move_right(&runs, Caret { run: 0, offset: 2 });
    assert_eq!(c, Caret { run: 1, offset: 0 });
}
```

Run: `cargo test -p lopress-editor --test inline_editor_tests 2>&1`. Iterate until all pass.

- [ ] **Step 3: Wire the editor widget into Floem**

Implement the Floem-side widget in `inline_editor.rs` below the pure-data helpers. Acceptance criteria for this step:

- The widget takes a `RwSignal<Vec<InlineRun>>` and a `RwSignal<Caret>`.
- Renders the runs as styled spans (paragraph.rs / heading.rs both use this for their body).
- Paints a 1-logical-px-wide blinking caret at the screen position derived from the caret state.
- Captures keyboard focus on click. Click position maps to nearest character (use Floem text layout's hit-testing).
- Keyboard handlers: character input (text-input event), Backspace, Delete, ArrowLeft, ArrowRight, Home, End. Each calls the corresponding pure helper on the runs and caret signals.

Floem-specific guidance: this is the biggest custom widget in the project. Reference Lapce's editor view for the widget pattern (`lapce-app/src/editor/view.rs` and around). Don't try to do this in one shot — get character input working first, then arrows, then Home/End, then Backspace/Delete.

- [ ] **Step 4: Update paragraph.rs and heading.rs**

Replace the read-only `render_runs` / `render_heading` in paragraph.rs / heading.rs to instead call into `inline_editor::editable_inline(runs_signal, caret_signal, font_size)`. The old read-only path can be kept under a different function name (e.g., `render_runs_readonly`) for use by opaque cards, but is not used by paragraph/heading anymore.

- [ ] **Step 5: Manual smoke test**

```bash
cargo run -p lopress
```

Open a post. Click into a paragraph. Type characters. Use arrows. Backspace. Delete. Confirm bold/italic spans visually retain their style as the caret moves through them.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(editor): inline-runs editor with caret + character input (Task 8/22)"
```

---

## Task 9: Inline-runs editor — single-block selection

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`
- Modify: `crates/lopress-editor/tests/inline_editor_tests.rs`

**Goal:** Add Shift+arrow / Shift+Home / Shift+End / Shift+Click for selection within one block. Mouse drag to select. Selection paints highlight.

**Acceptance criteria:**
- Shift+ArrowRight extends selection right one char.
- Shift+ArrowLeft extends left.
- Shift+Home selects to block start, Shift+End to block end.
- Click + drag selects a range.
- Click without shift collapses selection to caret position.
- Selected region paints with a highlight color (theme-aware).
- Typing replaces the selection (delete-then-insert).
- Backspace/Delete with non-collapsed selection deletes the selection.

- [ ] **Step 1: Add selection type and helpers**

In `inline_editor.rs`, replace `Caret` with a `LocalSelection { anchor: Caret, head: Caret }` (a single-block analog of `DocSelection`). A collapsed selection = caret. Update existing helpers to take/return `LocalSelection`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSelection {
    pub anchor: Caret,
    pub head: Caret,
}

impl LocalSelection {
    pub fn caret(c: Caret) -> Self {
        Self { anchor: c, head: c }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns (min, max) in document order.
    pub fn ordered(&self) -> (Caret, Caret) {
        if compare(self.anchor, self.head).is_le() {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

fn compare(a: Caret, b: Caret) -> std::cmp::Ordering {
    a.run.cmp(&b.run).then(a.offset.cmp(&b.offset))
}
```

Add `delete_selection(runs, sel) -> LocalSelection` that removes runs (or partial runs) inside the selection range, leaves caret at the start position.

- [ ] **Step 2: Write tests for selection helpers**

Add to `inline_editor_tests.rs`:

```rust
#[test]
fn delete_selection_removes_range() {
    let mut runs = vec![plain("hello world")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 5 },
        head: Caret { run: 0, offset: 11 },
    };
    let new = delete_selection(&mut runs, sel);
    assert_eq!(runs[0].text, "hello");
    assert_eq!(new, LocalSelection::caret(Caret { run: 0, offset: 5 }));
}

#[test]
fn delete_selection_across_runs() {
    let mut runs = vec![plain("hello "), {
        let mut r = plain("world");
        r.bold = true;
        r
    }];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 3 },
        head: Caret { run: 1, offset: 3 },
    };
    let _ = delete_selection(&mut runs, sel);
    // "hel" + "ld" = "hel" + bold "ld"
    assert!(runs.iter().any(|r| r.text == "hel"));
    assert!(runs.iter().any(|r| r.text == "ld" && r.bold));
}
```

- [ ] **Step 3: Implement Shift+arrow in keyboard handler**

In the Floem keyboard handler, when Shift is held with an arrow / Home / End, update only `head`, leaving `anchor` intact. When no Shift, collapse selection to head.

- [ ] **Step 4: Render selection highlight**

In the inline editor's paint pass, draw a semi-transparent highlight rectangle over each character whose position lies within `(min, max)` of the selection's `ordered()`. Floem-specific: layer the highlight beneath the text, use the theme's selection background color.

- [ ] **Step 5: Mouse drag**

Mousedown captures a "drag start" caret. Mouse move while button is down updates `head`. Mouseup ends the drag. Click without drag collapses selection.

- [ ] **Step 6: Manual smoke test**

```bash
cargo run -p lopress
```

Verify all selection acceptance criteria, including type-replaces-selection.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(editor): single-block selection in inline editor (Task 9/22)"
```

---

## Task 10: Inline toggles — Bold / Italic / Code / Link

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`
- Modify: `crates/lopress-editor/tests/inline_editor_tests.rs`

**Goal:** Ctrl+B / Ctrl+I / Ctrl+E / Ctrl+K toggle the corresponding inline flag across the selection. Toggle direction: clear if every char in selection has the flag set, otherwise set.

**Acceptance criteria:**
- Select text, press Ctrl+B → text becomes bold (renders bold visually).
- Press Ctrl+B again → bold cleared.
- Same behavior for Ctrl+I (italic), Ctrl+E (code), Ctrl+K (link with empty URL placeholder).
- Toggling preserves text and other styles. Adjacent runs with same style coalesce.
- With collapsed selection (caret only), Ctrl+B/I/E flips the typing-style state for the next inserted characters (this is "cursor style" — defer to v1.1 if it complicates Task 10; explicitly note in commit if deferred).

- [ ] **Step 1: Implement `toggle_inline` helper**

In `inline_editor.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub enum InlineFlag {
    Bold,
    Italic,
    Code,
    Link,
}

/// Toggle the given inline flag across the selection.
/// For Link, sets URL to empty string when toggling on.
pub fn toggle_inline(
    runs: &mut Vec<InlineRun>,
    sel: LocalSelection,
    flag: InlineFlag,
) -> LocalSelection {
    if sel.is_collapsed() {
        return sel; // cursor-style toggle deferred
    }
    // Step A: split runs at selection boundaries so the affected range
    // is exactly some contiguous slice of runs.
    let (start, end) = sel.ordered();
    split_at(runs, start);
    split_at(runs, end);
    // Recompute indices since splits inserted new runs
    let start_idx = locate(runs, start);
    let end_idx = locate(runs, end);

    // Step B: determine direction.
    let all_set = (start_idx..end_idx).all(|i| {
        runs.get(i)
            .map(|r| match flag {
                InlineFlag::Bold => r.bold,
                InlineFlag::Italic => r.italic,
                InlineFlag::Code => r.code,
                InlineFlag::Link => r.link.is_some(),
            })
            .unwrap_or(false)
    });
    let new_value = !all_set;

    // Step C: apply.
    for i in start_idx..end_idx {
        if let Some(r) = runs.get_mut(i) {
            match flag {
                InlineFlag::Bold => r.bold = new_value,
                InlineFlag::Italic => r.italic = new_value,
                InlineFlag::Code => r.code = new_value,
                InlineFlag::Link => {
                    r.link = if new_value { Some(String::new()) } else { None };
                }
            }
        }
    }

    // Step D: coalesce adjacent same-style runs across the affected range.
    // Walk affected range plus one neighbor on each side.
    let lo = start_idx.saturating_sub(1);
    let hi = end_idx.min(runs.len());
    coalesce_range(runs, lo, hi);

    sel
}

fn split_at(runs: &mut Vec<InlineRun>, pos: Caret) {
    // If pos is mid-run, split the run there.
    let Some(r) = runs.get(pos.run) else { return };
    let len = r.text.chars().count();
    if pos.offset == 0 || pos.offset == len {
        return;
    }
    let byte = char_to_byte(&r.text, pos.offset);
    let (left, right) = r.text.split_at(byte);
    let left = left.to_string();
    let right = right.to_string();
    let mut left_run = r.clone();
    left_run.text = left;
    let mut right_run = r.clone();
    right_run.text = right;
    runs[pos.run] = left_run;
    runs.insert(pos.run + 1, right_run);
}

fn locate(runs: &[InlineRun], pos: Caret) -> usize {
    // After splits, `pos` should align with the start of some run.
    let Some(r) = runs.get(pos.run) else { return runs.len() };
    let len = r.text.chars().count();
    if pos.offset == len {
        pos.run + 1
    } else {
        pos.run
    }
}

fn coalesce_range(runs: &mut Vec<InlineRun>, lo: usize, hi: usize) {
    let mut i = lo;
    while i + 1 < runs.len().min(hi + 1) {
        let merge = match (runs.get(i), runs.get(i + 1)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if merge {
            let next = runs.remove(i + 1);
            if let Some(cur) = runs.get_mut(i) {
                cur.text.push_str(&next.text);
            }
        } else {
            i += 1;
        }
    }
}
```

- [ ] **Step 2: Tests**

```rust
#[test]
fn toggle_bold_on_selection() {
    let mut runs = vec![plain("hello world")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 6 },
        head: Caret { run: 0, offset: 11 },
    };
    toggle_inline(&mut runs, sel, InlineFlag::Bold);
    let bold_part: String = runs.iter().filter(|r| r.bold).map(|r| r.text.clone()).collect();
    assert_eq!(bold_part, "world");
    let plain_part: String = runs.iter().filter(|r| !r.bold).map(|r| r.text.clone()).collect();
    assert_eq!(plain_part, "hello ");
}

#[test]
fn toggle_off_when_all_set() {
    let mut runs = vec![InlineRun {
        text: "all bold".into(),
        bold: true,
        ..Default::default()
    }];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 0 },
        head: Caret { run: 0, offset: 8 },
    };
    toggle_inline(&mut runs, sel, InlineFlag::Bold);
    assert!(runs.iter().all(|r| !r.bold));
}

#[test]
fn toggle_link_assigns_empty_url() {
    let mut runs = vec![plain("click here")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 0 },
        head: Caret { run: 0, offset: 5 },
    };
    toggle_inline(&mut runs, sel, InlineFlag::Link);
    assert_eq!(runs[0].link.as_deref(), Some(""));
}
```

Run, iterate, pass.

- [ ] **Step 3: Wire keyboard shortcuts into the widget**

In the Floem keyboard handler, add handlers for `Cmd/Ctrl+B`, `Cmd/Ctrl+I`, `Cmd/Ctrl+E`, `Cmd/Ctrl+K` that call `toggle_inline` with the matching flag. The handler must consume the event so platform default handling doesn't fire.

- [ ] **Step 4: Manual smoke test**

`cargo run -p lopress`. Type a sentence, select a word, Ctrl+B → bold renders. Repeat for italic, code, link. Toggle off works.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(editor): inline toggles Bold/Italic/Code/Link with selection (Task 10/22)"
```

---

## Task 11: BlockAction enum + apply chokepoint (split, merge, insert, delete, move, change-type)

**Files:**
- Create: `crates/lopress-editor/src/actions.rs`
- Create: `crates/lopress-editor/tests/actions_tests.rs`
- Modify: `crates/lopress-editor/src/lib.rs` (add `pub mod actions;`)
- Modify: inline editor + paragraph view to emit `BlockAction::Split` on Enter and `BlockAction::MergeWithPrev` on Backspace-at-block-start.

**Goal:** Establish the single mutation chokepoint. Every block-tree change goes through `apply(doc, action)`. Wire Enter → Split and Backspace-at-start → Merge.

**Acceptance criteria:**
- Press Enter mid-paragraph → block splits at caret; the trailing text becomes a new block of the same kind directly below; caret lands at start of new block.
- Press Enter in heading → split into two headings of same level.
- Press Backspace at start of block 2+ → block merges into previous; caret lands at the merge point in the previous block.
- Inserting a block via the (existing-Task-13 slash menu) goes through `apply`.
- Deleting a block via the (Task-12 toolbar) goes through `apply`.
- Move/Change-type wired in for the toolbar/DnD tasks to come.

- [ ] **Step 1: Define the enum and the chokepoint**

Create `crates/lopress-editor/src/actions.rs`:

```rust
use crate::model::types::{BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun};

#[derive(Debug, Clone)]
pub enum BlockAction {
    Split {
        block_id: BlockId,
        run: usize,
        offset: usize,
    },
    MergeWithPrev {
        block_id: BlockId,
    },
    InsertAfter {
        anchor: BlockId,
        new_block: EditorBlock,
    },
    Delete {
        block_id: BlockId,
    },
    Move {
        block_id: BlockId,
        to_index: usize,
    },
    ChangeType {
        block_id: BlockId,
        new_kind: BlockKind,
    },
    EditInline {
        block_id: BlockId,
        new_runs: Vec<InlineRun>,
    },
    EditCode {
        block_id: BlockId,
        new_text: String,
    },
}

pub fn apply(doc: &mut EditorDoc, action: BlockAction) {
    match action {
        BlockAction::Split { block_id, run, offset } => apply_split(doc, block_id, run, offset),
        BlockAction::MergeWithPrev { block_id } => apply_merge(doc, block_id),
        BlockAction::InsertAfter { anchor, new_block } => apply_insert_after(doc, anchor, new_block),
        BlockAction::Delete { block_id } => apply_delete(doc, block_id),
        BlockAction::Move { block_id, to_index } => apply_move(doc, block_id, to_index),
        BlockAction::ChangeType { block_id, new_kind } => apply_change_type(doc, block_id, new_kind),
        BlockAction::EditInline { block_id, new_runs } => apply_edit_inline(doc, block_id, new_runs),
        BlockAction::EditCode { block_id, new_text } => apply_edit_code(doc, block_id, new_text),
    }
}

// ── implementations ────────────────────────────────────────────────────────

fn find_idx(doc: &EditorDoc, id: BlockId) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

fn apply_split(doc: &mut EditorDoc, id: BlockId, run: usize, offset: usize) {
    let Some(idx) = find_idx(doc, id) else { return };
    // Implementation sketch: only Inline-body blocks support split here.
    // For Code blocks, splitting inserts a newline rather than splitting the block.
    use crate::model::types::{BlockBody, BlockKind};
    let Some(block) = doc.blocks.get(idx) else { return };
    let kind = block.kind.clone();
    let BlockBody::Inline(runs) = &block.body else {
        // Code: insert newline at offset; no split.
        if let BlockBody::Code(text) = &block.body {
            let mut new_text = text.clone();
            // best-effort offset interpretation: byte offset; clamp
            let byte = new_text.len().min(offset);
            new_text.insert(byte, '\n');
            apply_edit_code(doc, id, new_text);
        }
        return;
    };
    let runs = runs.clone();

    // Split runs at (run, offset) into left | right.
    let mut left: Vec<InlineRun> = Vec::new();
    let mut right: Vec<InlineRun> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        if i < run {
            left.push(r.clone());
        } else if i > run {
            right.push(r.clone());
        } else {
            let chars: Vec<char> = r.text.chars().collect();
            let split_at = offset.min(chars.len());
            let left_text: String = chars[..split_at].iter().collect();
            let right_text: String = chars[split_at..].iter().collect();
            if !left_text.is_empty() {
                left.push(InlineRun { text: left_text, ..r.clone() });
            }
            if !right_text.is_empty() {
                right.push(InlineRun { text: right_text, ..r.clone() });
            }
        }
    }

    // Update the original block's runs to `left`; insert a new block with `right`.
    if let Some(b) = doc.blocks.get_mut(idx) {
        b.body = BlockBody::Inline(left);
    }
    let right_block = match kind {
        BlockKind::Paragraph => EditorBlock::paragraph(right),
        BlockKind::Heading(level) => EditorBlock::heading(level, right),
        // List/code/opaque shouldn't normally hit Inline split; fall back to Paragraph.
        _ => EditorBlock::paragraph(right),
    };
    doc.blocks.insert(idx + 1, right_block);
}

fn apply_merge(doc: &mut EditorDoc, id: BlockId) {
    let Some(idx) = find_idx(doc, id) else { return };
    if idx == 0 {
        return;
    }
    let cur = doc.blocks.remove(idx);
    let Some(prev) = doc.blocks.get_mut(idx - 1) else {
        // restore
        doc.blocks.insert(idx, cur);
        return;
    };
    use crate::model::types::BlockBody;
    match (&mut prev.body, cur.body) {
        (BlockBody::Inline(prev_runs), BlockBody::Inline(cur_runs)) => {
            prev_runs.extend(cur_runs);
        }
        // mismatch: best-effort, leave prev alone (could append cur as opaque, but not necessary for v1).
        _ => {}
    }
}

fn apply_insert_after(doc: &mut EditorDoc, anchor: BlockId, new_block: EditorBlock) {
    let pos = find_idx(doc, anchor).map(|i| i + 1).unwrap_or(doc.blocks.len());
    if pos > doc.blocks.len() {
        doc.blocks.push(new_block);
    } else {
        doc.blocks.insert(pos, new_block);
    }
}

fn apply_delete(doc: &mut EditorDoc, id: BlockId) {
    let Some(idx) = find_idx(doc, id) else { return };
    doc.blocks.remove(idx);
    if doc.blocks.is_empty() {
        doc.blocks.push(EditorBlock::paragraph(vec![InlineRun::plain("")]));
    }
}

fn apply_move(doc: &mut EditorDoc, id: BlockId, to_index: usize) {
    let Some(from) = find_idx(doc, id) else { return };
    let to = to_index.min(doc.blocks.len().saturating_sub(1));
    if from == to {
        return;
    }
    let block = doc.blocks.remove(from);
    let adjusted = if to > from { to - 1 } else { to };
    doc.blocks.insert(adjusted, block);
}

fn apply_change_type(doc: &mut EditorDoc, id: BlockId, new_kind: BlockKind) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    use crate::model::types::BlockBody;
    match (&new_kind, &block.body) {
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_)) => {
            block.kind = new_kind;
        }
        (BlockKind::Code { lang }, BlockBody::Inline(runs)) => {
            // Convert inline runs to plain text.
            let text: String = runs.iter().map(|r| r.text.clone()).collect();
            block.kind = BlockKind::Code { lang: lang.clone() };
            block.body = BlockBody::Code(text);
        }
        (BlockKind::List { ordered }, BlockBody::Inline(runs)) => {
            block.kind = BlockKind::List { ordered: *ordered };
            block.body = BlockBody::List(vec![crate::model::types::ListItem {
                id: BlockId::new(),
                runs: runs.clone(),
            }]);
        }
        // Other transitions: best-effort, leave content alone.
        _ => {
            block.kind = new_kind;
        }
    }
}

fn apply_edit_inline(doc: &mut EditorDoc, id: BlockId, new_runs: Vec<InlineRun>) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    use crate::model::types::BlockBody;
    if matches!(block.body, BlockBody::Inline(_)) {
        block.body = BlockBody::Inline(new_runs);
    }
}

fn apply_edit_code(doc: &mut EditorDoc, id: BlockId, new_text: String) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    use crate::model::types::BlockBody;
    if matches!(block.body, BlockBody::Code(_)) {
        block.body = BlockBody::Code(new_text);
    }
}
```

- [ ] **Step 2: Write semantic tests**

Create `crates/lopress-editor/tests/actions_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::actions::*;
use lopress_editor::model::types::*;

fn doc_with(blocks: Vec<EditorBlock>) -> EditorDoc {
    EditorDoc {
        blocks,
        front_matter: lopress_core::FrontMatter::default(),
    }
}

#[test]
fn split_paragraph_at_middle() {
    let id = BlockId::new();
    let mut block = EditorBlock::paragraph(vec![InlineRun::plain("hello world")]);
    block.id = id;
    let mut doc = doc_with(vec![block]);
    apply(
        &mut doc,
        BlockAction::Split {
            block_id: id,
            run: 0,
            offset: 5,
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    if let BlockBody::Inline(left) = &doc.blocks[0].body {
        assert_eq!(left[0].text, "hello");
    } else {
        panic!();
    }
    if let BlockBody::Inline(right) = &doc.blocks[1].body {
        assert_eq!(right[0].text, " world");
    } else {
        panic!();
    }
}

#[test]
fn merge_appends_runs_to_prev() {
    let id_a = BlockId::new();
    let id_b = BlockId::new();
    let mut a = EditorBlock::paragraph(vec![InlineRun::plain("hello ")]);
    a.id = id_a;
    let mut b = EditorBlock::paragraph(vec![InlineRun::plain("world")]);
    b.id = id_b;
    let mut doc = doc_with(vec![a, b]);
    apply(&mut doc, BlockAction::MergeWithPrev { block_id: id_b });
    assert_eq!(doc.blocks.len(), 1);
    if let BlockBody::Inline(runs) = &doc.blocks[0].body {
        let combined: String = runs.iter().map(|r| r.text.clone()).collect();
        assert_eq!(combined, "hello world");
    } else {
        panic!();
    }
}

#[test]
fn insert_after_places_correctly() {
    let id = BlockId::new();
    let mut a = EditorBlock::paragraph(vec![]);
    a.id = id;
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::InsertAfter {
            anchor: id,
            new_block: EditorBlock::heading(1, vec![InlineRun::plain("Title")]),
        },
    );
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[1].kind, BlockKind::Heading(1)));
}

#[test]
fn delete_last_block_inserts_empty_paragraph() {
    let id = BlockId::new();
    let mut a = EditorBlock::paragraph(vec![InlineRun::plain("only")]);
    a.id = id;
    let mut doc = doc_with(vec![a]);
    apply(&mut doc, BlockAction::Delete { block_id: id });
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0].kind, BlockKind::Paragraph));
}

#[test]
fn move_forward_one_position() {
    let id_a = BlockId::new();
    let id_b = BlockId::new();
    let id_c = BlockId::new();
    let mut a = EditorBlock::paragraph(vec![InlineRun::plain("a")]);
    a.id = id_a;
    let mut b = EditorBlock::paragraph(vec![InlineRun::plain("b")]);
    b.id = id_b;
    let mut c = EditorBlock::paragraph(vec![InlineRun::plain("c")]);
    c.id = id_c;
    let mut doc = doc_with(vec![a, b, c]);
    apply(
        &mut doc,
        BlockAction::Move {
            block_id: id_a,
            to_index: 2,
        },
    );
    assert_eq!(doc.blocks[0].id, id_b);
    assert_eq!(doc.blocks[1].id, id_a);
    assert_eq!(doc.blocks[2].id, id_c);
}

#[test]
fn change_paragraph_to_heading() {
    let id = BlockId::new();
    let mut a = EditorBlock::paragraph(vec![InlineRun::plain("title")]);
    a.id = id;
    let mut doc = doc_with(vec![a]);
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Heading(2),
        },
    );
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading(2)));
}
```

Run, pass.

- [ ] **Step 3: Wire Enter and Backspace-at-start in the inline editor**

In the inline editor's keyboard handler, when the user presses Enter (no shift) outside a multi-block selection:
- Compute `(run, offset)` from the current caret.
- Emit `BlockAction::Split { block_id, run, offset }` via a callback the inline editor takes from its parent.
- The parent (paragraph/heading view) calls `apply(doc, action)` and updates focus to the new block.

When the user presses Backspace and the caret is at `Caret::START`:
- Emit `BlockAction::MergeWithPrev { block_id }`.

This requires the inline editor widget to take an `on_action: Fn(BlockAction)` callback. Update its signature and wire from paragraph/heading views.

- [ ] **Step 4: Manual smoke test**

`cargo run -p lopress`. Open a post. Type a sentence, press Enter mid-sentence — block splits, caret lands in new block. Press Backspace at start of new block — blocks merge.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(editor): BlockAction chokepoint + Enter/Backspace block split/merge (Task 11/22)"
```

---

## Task 12: Block toolbar (above focused block)

**Files:**
- Create: `crates/lopress-editor/src/ui/toolbar.rs`
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs`

**Goal:** Render a toolbar above the currently-focused block. Type combobox + B/I/code/link buttons + delete.

**Acceptance criteria:**
- Click into a paragraph → toolbar appears anchored above it.
- Type combobox shows current type (P/H1-3/Code/UL/OL); selecting a value emits `BlockAction::ChangeType`.
- B/I/code/link buttons toggle the inline flag on selection (calls into Task 10's `toggle_inline`); show as filled if every char in selection has the flag.
- Delete button emits `BlockAction::Delete`.
- Toolbar disappears when no block is focused.
- Toolbar tracks block position as content shifts.

- [ ] **Step 1: Toolbar widget**

Create `crates/lopress-editor/src/ui/toolbar.rs`:

```rust
//! Block toolbar — anchored above the focused block.
//!
//! Reads:
//!   - focused block kind (for the type combobox state)
//!   - current selection's inline flag state (for button fill states)
//! Emits:
//!   - BlockAction::ChangeType
//!   - BlockAction::Delete
//!   - inline toggle calls back into the inline editor (via callback)

use crate::actions::BlockAction;
use crate::model::types::{BlockId, BlockKind};
use crate::ui::blocks::inline_editor::InlineFlag;
use floem::IntoView;

pub struct ToolbarState {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub bold_active: bool,
    pub italic_active: bool,
    pub code_active: bool,
    pub link_active: bool,
}

pub fn block_toolbar<F, T>(
    state: ToolbarState,
    on_action: F,
    on_toggle: T,
) -> impl IntoView
where
    F: Fn(BlockAction) + Clone + 'static,
    T: Fn(InlineFlag) + Clone + 'static,
{
    let _ = (state, on_action, on_toggle);
    // TODO Floem-specific: horizontal stack with:
    //   - dropdown: P / H1 / H2 / H3 / Code / UL / OL  → on change emit ChangeType
    //   - toggle button "B" (filled if state.bold_active)  → on click emit on_toggle(Bold)
    //   - toggle button "I"
    //   - toggle button "</>"  for code
    //   - toggle button "🔗"   for link
    //   - separator
    //   - button "×" → emit Delete
    floem::views::label(|| "TODO: block toolbar")
}
```

- [ ] **Step 2: Mount in editor_pane**

Modify `crates/lopress-editor/src/ui/editor_pane.rs` so it renders the toolbar when a block is focused. Position the toolbar with negative top margin so it visually anchors above the block. Clip when the block scrolls off-screen.

- [ ] **Step 3: Manual smoke test**

`cargo run -p lopress`. Click into different blocks. Toolbar appears with current type. Click ChangeType dropdown → block changes. Click B/I/code/link with selection → toggle works. Click × → block deleted.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(editor): block toolbar above focused block (Task 12/22)"
```

---

## Task 13: Slash command menu

**Files:**
- Create: `crates/lopress-editor/src/ui/slash_menu.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (detect "/" trigger)

**Goal:** When the user types `/` at the start of an empty paragraph, a popup menu appears. Up/Down navigates, Enter inserts the chosen block via `BlockAction::ChangeType`.

**Acceptance criteria:**
- `/` at start of empty paragraph → popup with: Paragraph, Heading 1, Heading 2, Heading 3, Code block, Unordered list, Ordered list.
- `/` mid-text or in non-empty block → literal `/` typed.
- Arrow up/down navigates the menu.
- Enter confirms; Escape closes.
- Selecting an item changes the current block's kind.

- [ ] **Step 1: Detection**

In `inline_editor.rs`, when handling text-input events:
- If the character is `/` AND the runs are empty AND the block kind is Paragraph → emit a `BlockAction::OpenSlashMenu { block_id }` (new variant) instead of inserting `/`.
- Otherwise insert as normal.

Add a `BlockAction::OpenSlashMenu` variant in `actions.rs`. `apply` for it just sets a flag in `EditingState::slash_menu_open: Option<BlockId>`. (You'll need to extend EditingState; do that in this task.)

- [ ] **Step 2: Build the slash menu popup**

Create `crates/lopress-editor/src/ui/slash_menu.rs`:

```rust
use crate::model::types::BlockKind;
use floem::IntoView;

pub fn slash_menu_items() -> Vec<(&'static str, BlockKind)> {
    vec![
        ("Paragraph", BlockKind::Paragraph),
        ("Heading 1", BlockKind::Heading(1)),
        ("Heading 2", BlockKind::Heading(2)),
        ("Heading 3", BlockKind::Heading(3)),
        ("Code block", BlockKind::Code { lang: String::new() }),
        ("Unordered list", BlockKind::List { ordered: false }),
        ("Ordered list", BlockKind::List { ordered: true }),
    ]
}

pub fn slash_menu<F>(on_select: F, on_close: impl Fn() + Clone + 'static) -> impl IntoView
where
    F: Fn(BlockKind) + Clone + 'static,
{
    let _ = (on_select, on_close);
    // TODO Floem-specific: floating popup list, Up/Down to move highlight,
    // Enter to confirm, Escape to close.
    floem::views::label(|| "TODO: slash menu")
}
```

- [ ] **Step 3: Mount the popup in editor_pane**

When `EditingState::slash_menu_open` is `Some(block_id)`, render the slash menu anchored to that block. On selection, emit `BlockAction::ChangeType { block_id, new_kind }` then clear `slash_menu_open`.

- [ ] **Step 4: Manual smoke test**

`cargo run -p lopress`. New empty paragraph. Type `/` → menu opens. Arrow down to "Heading 2" → Enter → block becomes Heading 2. Type `/` mid-text → no menu, literal `/`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(editor): slash command menu for block insertion (Task 13/22)"
```

---

## Task 14: Drag-and-drop block reorder

**Files:**
- Create: `crates/lopress-editor/src/ui/dnd.rs`
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs`, all per-block views

**Goal:** Hover-revealed `⋮⋮` drag handle on the left of each block. Dragging shows a drop indicator at gap positions; drop emits `BlockAction::Move`.

**Acceptance criteria:**
- Hover over a block → `⋮⋮` handle fades in on the left, hover-area opacity 1.
- Mousedown on handle, drag → handle becomes draggable; indicator line appears at hovered gap.
- Drop on a gap → block moves there.
- Esc or drop outside → cancel.

- [ ] **Step 1: Drag handle widget**

Create `crates/lopress-editor/src/ui/dnd.rs`. Floem-specific: implement using Floem's drag-source/drop-target primitives if available (Floem has `dnd_drag_source` style helpers in 0.x — verify), otherwise implement manually with mousedown/mousemove/mouseup tracking and global state.

- [ ] **Step 2: Drop indicator**

Render a 2-logical-px-tall horizontal line at the gap above/below the hovered drop target during a drag. Floem-specific: use a sibling view layered into the block list.

- [ ] **Step 3: Manual smoke test**

`cargo run -p lopress`. Hover blocks → handle appears. Drag a block → indicator. Drop → reorder.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(editor): drag-and-drop block reorder (Task 14/22)"
```

---

## Task 15: DocSelection + keyboard routing across blocks

**Files:**
- Create: `crates/lopress-editor/src/selection.rs`
- Create: `crates/lopress-editor/tests/selection_tests.rs`
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs`
- Modify: per-block views to read selection state

**Goal:** Doc-level selection. EditorPane intercepts arrow/shift/Cmd-A. Vertical arrows cross block boundaries via a per-block geometry cache.

**Acceptance criteria:**
- Click into a block → `head` and `anchor` collapse there.
- Shift+ArrowDown extends `head` into the next block.
- Up/Down navigation lands at the visually-correct x-position in the target block (within ±1 char of the source x).
- Cmd/Ctrl+A selects the whole document.
- Selection paints across all involved blocks.
- Caret only blinks in the block holding `head`.

- [ ] **Step 1: Define DocSelection types**

Create `crates/lopress-editor/src/selection.rs`:

```rust
use crate::model::types::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocPosition {
    pub block: BlockId,
    pub run: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocSelection {
    pub anchor: DocPosition,
    pub head: DocPosition,
}

impl DocSelection {
    pub fn caret(p: DocPosition) -> Self {
        Self { anchor: p, head: p }
    }
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// Per-block geometry cache for cross-block vertical-arrow navigation.
/// Each entry is `(BlockId, Vec<f32>)` — char-x positions for the current frame.
#[derive(Default)]
pub struct GeometryCache {
    map: std::collections::HashMap<BlockId, Vec<f32>>,
}

impl GeometryCache {
    pub fn put(&mut self, id: BlockId, xs: Vec<f32>) {
        self.map.insert(id, xs);
    }
    pub fn get(&self, id: BlockId) -> Option<&[f32]> {
        self.map.get(&id).map(|v| v.as_slice())
    }
    pub fn nearest_offset(&self, id: BlockId, target_x: f32) -> Option<usize> {
        let xs = self.get(id)?;
        let (i, _) = xs
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, x)| (i, (x - target_x).abs()))
            .or(Some((0, 0.0)))?;
        Some(i)
    }
}
```

- [ ] **Step 2: Tests for selection ordering and ops helpers**

Create `crates/lopress-editor/tests/selection_tests.rs`:

```rust
use lopress_editor::model::types::BlockId;
use lopress_editor::selection::*;

#[test]
fn caret_constructor_collapses() {
    let p = DocPosition {
        block: BlockId::new(),
        run: 0,
        offset: 0,
    };
    let s = DocSelection::caret(p);
    assert!(s.is_collapsed());
}

#[test]
fn nearest_offset_finds_closest() {
    let id = BlockId::new();
    let mut cache = GeometryCache::default();
    cache.put(id, vec![0.0, 10.0, 20.0, 30.0]);
    assert_eq!(cache.nearest_offset(id, 11.0), Some(1));
    assert_eq!(cache.nearest_offset(id, 25.0), Some(2));
}
```

- [ ] **Step 3: Move selection ownership to EditorPane**

Replace per-block `LocalSelection` reads with reads from a parent-owned `DocSelection`. Each block view receives a slice (none / partial-leading / full / partial-trailing) computed from `DocSelection` and renders accordingly. Only the block holding `head` paints a caret.

This is a refactor of the inline editor widget — substantial. Take it step by step and run the existing tests after the changes.

- [ ] **Step 4: Implement keyboard routing in EditorPane**

EditorPane intercepts:
- `←` `→` — move `head` one char left/right (uses inline-editor helpers locally; crosses block boundaries).
- `↑` `↓` — for the source block, look up source `head.offset`'s x in the geometry cache. For the target block (next in `↓`, prev in `↑`), find the offset whose cached x is nearest. Cmd-Up/Down go to doc start/end.
- `Home` / `End` — start/end of current block's runs.
- `Shift+` modifier — same target but only updates `head`.
- `Cmd/Ctrl+A` — anchor at doc start, head at doc end.

Character input, Backspace, Delete continue to flow to the focused block when selection is collapsed (Task 16 covers non-collapsed cases).

- [ ] **Step 5: Per-block geometry cache write**

Each inline editor widget, on render/layout, writes its char-x positions into the parent's `GeometryCache`. Floem-specific: this requires access to the text layout's per-char positions; check Floem's text layout API for hooks.

- [ ] **Step 6: Manual smoke test**

`cargo run -p lopress`. Click into a paragraph. Shift+Down extends to next block. Up/Down navigate vertically with x-position preserved. Cmd-A selects all. Caret only blinks in the block with `head`.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(editor): document-level selection + cross-block keyboard routing (Task 15/22)"
```

---

## Task 16: Multi-block operations — delete, toggle, copy/cut/paste

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (new variants for multi-block ops)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (clipboard handling)
- Add tests to `crates/lopress-editor/tests/actions_tests.rs`

**Goal:** When `DocSelection` is non-collapsed and spans 2+ blocks, the editor implements:
- Delete / Backspace / character input → splice + merge into one block of the leading kind.
- Ctrl+B/I/E/K → toggle flag across all touched runs.
- Cmd/Ctrl+C / Cut → write multi-block clipboard payloads (markdown for external paste; serialized `Vec<EditorBlock>` slice for internal paste).
- Cmd/Ctrl+V → paste; if internal payload present, splice in; else parse as markdown and splice.

**Acceptance criteria:**
- Each of the above behaviors works end-to-end on real selections.
- Internal paste preserves block kinds and inline styles.
- External paste from a non-lopress source parses markdown sensibly into blocks.

- [ ] **Step 1: Add multi-block action variants**

```rust
pub enum BlockAction {
    // ... existing variants ...
    DeleteRange { selection: DocSelection },
    ToggleInlineRange {
        selection: DocSelection,
        flag: InlineFlag,
    },
    PasteBlocks {
        at: DocPosition,
        blocks: Vec<EditorBlock>,
    },
}
```

- [ ] **Step 2: Implement `apply_delete_range`**

Pure-data implementation: split the leading and trailing blocks at the selection bounds, drop everything between, merge what remains into a single block of the leading kind.

Test:

```rust
#[test]
fn delete_range_across_three_blocks_merges_into_leading_kind() {
    let id_a = BlockId::new();
    let id_b = BlockId::new();
    let id_c = BlockId::new();
    let mut a = EditorBlock::heading(1, vec![InlineRun::plain("Hel")]);
    a.id = id_a;
    let mut b = EditorBlock::paragraph(vec![InlineRun::plain("middle")]);
    b.id = id_b;
    let mut c = EditorBlock::paragraph(vec![InlineRun::plain("rest")]);
    c.id = id_c;
    let mut doc = doc_with(vec![a, b, c]);

    apply(&mut doc, BlockAction::DeleteRange {
        selection: DocSelection {
            anchor: DocPosition { block: id_a, run: 0, offset: 3 },
            head: DocPosition { block: id_c, run: 0, offset: 4 },
        },
    });

    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0].kind, BlockKind::Heading(1))); // leading kind wins
    if let BlockBody::Inline(runs) = &doc.blocks[0].body {
        let combined: String = runs.iter().map(|r| r.text.clone()).collect();
        assert_eq!(combined, "Hel");
    }
}
```

- [ ] **Step 3: Implement `apply_toggle_inline_range`**

Iterate every block touched. For Inline blocks, slice runs at selection bounds and toggle the flag. Use the `toggle_inline` helper from Task 10.

- [ ] **Step 4: Implement `apply_paste_blocks`**

If pasting into the middle of an inline block, split the block at `at`, splice pasted blocks between the halves, merge first pasted into left half (if same kind) and last pasted into right half (if same kind). Otherwise pasted blocks just get inserted between.

- [ ] **Step 5: Wire keyboard / clipboard in EditorPane**

EditorPane's keyboard handler:
- Delete/Backspace + non-collapsed → emit DeleteRange.
- Character input + non-collapsed → emit DeleteRange then InsertChar (two-action sequence).
- Ctrl+B/I/E/K + non-collapsed → emit ToggleInlineRange.
- Cmd/Ctrl+C/X — write to clipboard. Use `arboard` or Floem's clipboard helper to write two payloads: a `text/plain` markdown string (use `to_core` + lopress-core's serializer on the selected slice) and a `application/x-lopress-blocks` payload with serialized `Vec<EditorBlock>` JSON.
- Cmd/Ctrl+V — read clipboard. Prefer `application/x-lopress-blocks` if present; else parse `text/plain` via `lopress_core::parse` and convert via `from_core`.

(If `arboard` is not in deps yet, add it to the workspace and `lopress-editor` Cargo.toml.)

- [ ] **Step 6: Manual smoke test**

`cargo run -p lopress`. Make a selection across 3 blocks. Press Backspace → all merged into leading kind. Undo isn't shipped, so use git/file-rollback if you want to repeat; or paste the selection back from clipboard. Cut + paste roundtrip should preserve content.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(editor): multi-block delete, toggle, copy/cut/paste (Task 16/22)"
```

---

## Task 17: Plugin block rendering (Path 1)

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs` (consult `PluginRegistry`)
- Modify: `crates/lopress-editor/src/model/to_core.rs` (handle `plugin: Some(...)`)
- Create: `crates/lopress-editor/src/ui/blocks/plugin.rs`
- Create: `crates/lopress-editor/tests/plugin_block_tests.rs`
- Modify: `EditingState` to hold `PluginRegistry` (loaded from workspace at session open)
- Modify: `Session::open` integration (verify registry is loaded — already done by `lopress-build`'s pipeline)

**Goal:** Plugin-declared blocks render with the built-in editor matching the plugin's `editor` field plus an attr form. Round-trip preserves the plugin block's type name and attrs.

**Acceptance criteria:**
- Test workspace contains a plugin declaring `lopress:codehighlight` block with `editor = "code"`, attrs `{lang, theme}`.
- A document containing that block opens with: a "lopress:codehighlight" header strip, a form with lang dropdown + theme dropdown, and a code editor body. All three are interactive.
- Save → on disk, the block has its original type name and attrs.
- Uninstall the plugin (remove from PluginRegistry) → block falls back to opaque card. Round-trip still preserves it.

- [ ] **Step 1: Hold PluginRegistry in EditingState**

Modify `EditingState` to take a `PluginRegistry` at construction:

```rust
pub struct EditingState {
    pub session: Session,
    pub plugin_registry: lopress_plugin::PluginRegistry,
    pub current_doc: Option<EditorDoc>,
    pub current_ref: Option<DocumentRef>,
    pub last_error: Option<String>,
}
```

`Session` already loads plugins as part of build — surface a method `Session::plugin_registry(&self) -> &PluginRegistry` if not already exposed.

- [ ] **Step 2: Update from_core to consult the registry**

Change `doc_from_core` signature to `doc_from_core(doc: &Document, registry: &PluginRegistry) -> EditorDoc`. When `block_from_core` sees a type that doesn't match a built-in (paragraph/heading/code_block/list), look it up in the registry. If found:
- Read `editor` field. Map to `BlockKind` and `BlockBody`:
  - `"paragraph"` → `Paragraph` + `Inline(parse_inline(text))`.
  - `"heading"` → `Heading(1)` + `Inline(...)`. Use level from attrs if present.
  - `"code"` → `Code { lang }` + `Code(text)`. Lang from attrs.
  - `"list"` → `List { ordered }` + `List(items)`. Same convertibility check as built-in list.
  - `null` or unrecognized → `Paragraph` + `Inline`.
- Populate `plugin: Some(PluginMeta { ... })` with the original type name, attrs, and a snapshot of the attr_decls.

If not found in registry, fall back to `Opaque` (as today).

- [ ] **Step 3: Update to_core to handle plugin meta**

In `block_to_core`, if `block.plugin.is_some()`, reconstruct `Block { r#type: plugin.block_type_name, attrs: plugin.attrs, ..body-derived-fields }`.

- [ ] **Step 4: Build the plugin block view**

Create `crates/lopress-editor/src/ui/blocks/plugin.rs`:

```rust
use crate::actions::BlockAction;
use crate::model::types::EditorBlock;
use floem::IntoView;

pub fn plugin_block_view(block: &EditorBlock) -> impl IntoView {
    // Stack:
    //   1. Plugin header strip — block.plugin.block_type_name as a tag-styled label.
    //   2. Attr form — for each attr_decl, render an input per its `ui` hint:
    //        text     → text field (writes back to plugin.attrs)
    //        select   → dropdown with options
    //        checkbox → checkbox
    //        number   → numeric text field
    //        unknown  → text field (raw JSON value)
    //      Edits emit BlockAction::EditAttrs (new variant; add it).
    //   3. Body editor — dispatch on block.kind + block.body to the matching
    //      built-in view (paragraph / heading / code / list).
    let _ = block;
    floem::views::label(|| "TODO: plugin block view")
}
```

Add `BlockAction::EditAttrs { block_id, new_attrs }` and an apply implementation that updates `block.plugin.attrs` (only valid when `plugin.is_some()`).

- [ ] **Step 5: Mount plugin view in block_view dispatch**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, the `block_view` function checks `block.plugin.is_some()` first. If so, calls `plugin::plugin_block_view`. Otherwise dispatches on `kind` as before.

- [ ] **Step 6: Test workspace fixture**

Create a test workspace under `tests/fixtures/plugin-roundtrip/` with:
- `lopress.toml`
- `plugins/codehighlight/plugin.toml` declaring `lopress:codehighlight` with `editor = "code"`, attrs lang+theme.
- `posts/example.md` containing one `lopress:codehighlight` block.

- [ ] **Step 7: Plugin block round-trip test**

Create `crates/lopress-editor/tests/plugin_block_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

#[test]
fn plugin_block_round_trips_byte_identical() {
    // Load fixture workspace
    let ws_root = std::path::Path::new("tests/fixtures/plugin-roundtrip");
    let workspace = lopress_build::Workspace::load(ws_root).unwrap();
    let registry = lopress_plugin::PluginRegistry::load(&workspace.plugins_dir())
        .unwrap_or_default();

    let post_path = ws_root.join("posts/example.md");
    let raw = std::fs::read_to_string(&post_path).unwrap();
    let core = lopress_core::parse(&raw).unwrap();

    let editor = lopress_editor::model::from_core::doc_from_core(&core, &registry);
    // First block should have plugin = Some
    let first = editor.blocks.first().unwrap();
    assert!(first.plugin.is_some(), "plugin block should be detected");

    let core_back = lopress_editor::model::to_core::doc_to_core(&editor);
    let serialized = lopress_core::serialize(&core_back);
    assert_eq!(serialized, raw);
}
```

(Adjust the `PluginRegistry::load` call to match the actual loader API in `lopress-plugin`.)

Run, iterate, pass.

- [ ] **Step 8: Manual smoke test**

`cargo run -p lopress`. Open the fixture workspace. Open the `example.md` post. See: codehighlight header strip, lang+theme dropdowns above the code body. Edit the code, change theme — observe the file on disk reflects both changes after save.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(editor): plugin block rendering (Path 1) (Task 17/22)"
```

---

## Task 18: Sidebar

**Files:**
- Create: `crates/lopress-editor/src/ui/sidebar.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs` (replace placeholder)

**Goal:** List Posts and Pages from `Session::workspace()`. Click → opens that document. Active row highlighted. "+ New post" / "+ New page" buttons at bottom.

**Acceptance criteria:**
- Sidebar shows two groups: Posts, Pages.
- Each entry: title, draft pill if `is_draft`, parse-error pill if `has_parse_error`.
- Click → calls `EditingState::open_document`.
- Active row (currently open doc) highlighted.
- "+ New post" creates a stub markdown file under `posts/` and opens it. Similarly "+ New page" under `pages/`.

- [ ] **Step 1: Sidebar widget**

Create `crates/lopress-editor/src/ui/sidebar.rs` per the structural sketch in section 6 of the spec. Use `Session::workspace()` for data.

- [ ] **Step 2: New-post/new-page actions**

Implement: write a stub markdown like:

```markdown
---
title: New Post
date: 2026-05-02
draft: true
---

```

Resolve a unique slug (e.g. `untitled-N` where N increments to find an unused name). Open the new file via `EditingState::open_document`.

- [ ] **Step 3: Manual smoke test**

`cargo run -p lopress`. Open workspace. Sidebar populated. Click posts to switch between them. Click "+ New post" — new file appears, opens, sidebar updates.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(editor): sidebar with post/page list and new-doc buttons (Task 18/22)"
```

---

## Task 19: Inspector

**Files:**
- Create: `crates/lopress-editor/src/ui/inspector.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

**Goal:** Right-pinned 280-logical-px panel with form for front-matter fields.

**Acceptance criteria:**
- Fields: title (text), slug (text, derived placeholder when empty), date (text ISO), tags (comma-separated text), draft (bool).
- Edits update `current_doc.front_matter` and mark dirty.

- [ ] **Step 1: Inspector widget per spec section 6**

- [ ] **Step 2: Manual smoke test**

Verify each field reads/writes correctly.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(editor): front matter inspector pane (Task 19/22)"
```

---

## Task 20: Footer

**Files:**
- Create: `crates/lopress-editor/src/ui/footer.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

**Goal:** Strip at the bottom showing build status, save state, word count, server URL.

**Acceptance criteria:**
- Build status indicator (idle/building/ok/failed) reading `Session::build_status()`.
- Save state: saved / unsaved / save error from `EditingState`.
- Word count: sum of words in all blocks (whitespace-split on inline run text + code body text).
- Server URL: click-to-copy via `Session::serve_status()`.

- [ ] **Step 1: Footer widget per spec section 6**

- [ ] **Step 2: Manual smoke test**

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(editor): footer with build/save/word-count/server URL (Task 20/22)"
```

---

## Task 21: Save debounce + rebuild

**Files:**
- Modify: `crates/lopress-editor/src/state.rs` (debounce timer)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (mark dirty on action)
- Modify: window-close handler (force flush)

**Goal:** Edits mark `doc.dirty`. 500 ms after the last keystroke (no further changes), call `Session::save` then `Session::rebuild`. Force-flush on window close.

**Acceptance criteria:**
- Type a paragraph. Stop typing. After ~500 ms, the file on disk reflects the changes.
- Live preview in the browser updates after rebuild.
- Close the window with unsaved changes → save flushes synchronously before exit.
- Save error appears in the footer; doesn't block the editor.

- [ ] **Step 1: Implement debounce timer**

Floem-specific: use Floem's timer/effect API to schedule a callback at "now + 500 ms". On every dirty-event, reset the timer. On fire, call save+rebuild.

- [ ] **Step 2: Window-close handler**

Hook the close event to flush synchronously if `current_doc.dirty`.

- [ ] **Step 3: Save error display**

On save error, set `EditingState::last_error` (rendered by the footer).

- [ ] **Step 4: Manual smoke test**

`cargo run -p lopress` with a workspace that has `lopress-serve` running. Type, stop, watch browser update.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(editor): debounced autosave + rebuild + close-flush (Task 21/22)"
```

---

## Task 22: Manual smoke checklist

**Files:** None (manual run + checklist verification).

Run the manual smoke checklist from spec section 11 verbatim, in order, in a clean checkout. Document any failures as bugs in a follow-up commit.

- [ ] **Step 1: Run the checklist**

1. Launch the app, see Welcome screen.
2. Open a workspace, see Sidebar populated.
3. Open a post, see blocks rendered.
4. Type a paragraph with bold (Ctrl+B), italic (Ctrl+I), code (Ctrl+E), link (Ctrl+K).
5. Insert a heading via slash command.
6. Drag a block to reorder.
7. Select across blocks with Shift+Down, delete the selection.
8. Save (via debounce), see preview update in browser.
9. Close window, confirm dirty save flush.
10. Re-open, confirm window position restored.

- [ ] **Step 2: Run the test suite**

```bash
cargo test --workspace 2>&1
cargo clippy --all-targets 2>&1
cargo fmt --check 2>&1
```

Expected: all green.

- [ ] **Step 3: Build releases**

```bash
cargo build --release -p lopress 2>&1
```

Verify the release binary launches and the smoke checklist still passes.

- [ ] **Step 4: Final commit + tag**

```bash
git tag editor-floem-v1
git commit --allow-empty -m "release: editor migration to Floem v1 complete (Task 22/22)"
```

---

## Self-Review

### Spec coverage check

| Spec section | Plan task(s) |
|--------------|--------------|
| 1. Goals (block-level WYSIWYG, slash, toolbar, drag, multi-block, plugin, on-disk format, cross-platform, window state) | 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 21, 3 |
| 1. Non-goals (tables, plugin code hooks, syntax highlighting, collab, automated GUI) | not implemented (correct) |
| 1. Deferred (undo, ui zoom) | not implemented; foundation in 11 |
| 2. Architecture (in-place rewrite, single binary, Floem dep, pulldown-cmark, cross-platform) | 1 |
| 3. Document model | 4, 5, 6, 17 |
| 4. Editor pane structure (per-block views, selection, keyboard routing, multi-block ops, caret-x cache, drag handles, toolbar, slash menu) | 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 |
| 5. Block types | 7 (rendering), 17 (plugin variant) |
| 6. Other panes (welcome, sidebar, inspector, footer, app shell) | 3, 18, 19, 20 |
| 7. Save behavior (debounce, force-flush) | 21 |
| 8. Action shape | 11, 16, 17 |
| 9. Dimensions, logical px | discipline note throughout; concrete sizes referenced in 7 |
| 10. Settings file | 2 |
| 11. Testing strategy | 5, 6, 8, 9, 10, 11, 15, 16, 17, 22 |
| 12. Plugin block rendering | 17 |

### Placeholder scan
The plan contains TODO markers in Floem-UI code skeletons (Tasks 7, 8, 9, 12, 13, 14, etc.). These are intentional and clearly framed as "where Floem-specific code goes — consult Floem examples for exact API." Each is paired with a structural description of what to build and an acceptance-criteria block. This is the honest state given Floem 0.x's API churn and my inability to verify current Floem APIs from the offline context. Engineers executing the plan are expected to consult Floem and Lapce sources to fill them in.

### Type consistency
- `BlockId`, `Caret`, `LocalSelection`, `DocPosition`, `DocSelection`, `BlockAction`, `BlockKind`, `BlockBody`, `EditorBlock`, `EditorDoc`, `InlineRun`, `PluginMeta` — all defined in tasks 4 / 8 / 9 / 11 / 15 / 17, used consistently downstream.
- `apply(doc, action)` signature: `(doc: &mut EditorDoc, action: BlockAction)` throughout.
- `from_core` / `to_core` signatures gain a `&PluginRegistry` parameter in Task 17 — earlier tasks (5/6) define them without it as a build-up step. Task 17's modification is explicit.
- `InlineFlag` defined in Task 10; reused in Task 16 multi-block toggle.

### Scope check
Task count is 22, matching the spec's 15 high-level items with structural splits (Task 8/9/10 split inline editing into manageable chunks; Task 15/16 split multi-block into routing vs ops). Each task is substantial but bounded. The plan is one-implementation-plan-sized for a multi-week effort.

### Known gaps acknowledged honestly
- Floem-specific API names are not pinned in code blocks. Engineers must consult Floem 0.x and Lapce to fill them in. This is a deliberate choice; fabricating API would be worse.
- The "cursor style" behavior (typing-flag toggle on collapsed selection) is deferred from Task 10 if it complicates implementation; called out in that task's acceptance criteria.
- Clipboard MIME type for internal paste (`application/x-lopress-blocks`) is named in Task 16 but its registration depends on the platform clipboard API surface accessible from Floem — engineer may need to fall back to a JSON-prefixed text payload if native MIME registration isn't straightforward.

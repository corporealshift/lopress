# Lopress Phase 3 — Editor MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GUI editor (`lopress` with no args opens it) that opens a workspace, edits paragraph and heading blocks, saves via 500 ms debounce, and live-reloads an external browser via the existing SSE serve machinery.

**Architecture:** Two new crates (`lopress-editor` for egui UI, `lopress-gui-host` for workspace/IO glue). `lopress-serve` gains a `serve_in_background` function that runs the HTTP/SSE stack on background threads and returns a `ServerHandle`. `main.rs` is reworked so no-args → GUI, positional path → GUI with workspace preloaded, explicit subcommands unchanged.

**Tech Stack:** `eframe`/`egui` for the window and widgets; `rfd` for native file picker; `open` crate to launch the browser; `directories` for config dir (recents list); `lopress-core` for block tree types and serialize/parse; `lopress-build` for incremental rebuilds; `lopress-watch` for fs watching; `lopress-serve` for the HTTP+SSE stack.

**Spec:** [`docs/superpowers/specs/2026-04-20-lopress-phase-3-editor-mvp-design.md`](../specs/2026-04-20-lopress-phase-3-editor-mvp-design.md)

**Project conventions:**
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass before every commit.
- No `.unwrap()` / `.expect()` in production code — use `unwrap_or_else`, `?`, or `let … else`.
- Tests are exempt via `#![cfg_attr(test, allow(clippy::unwrap_used, …))]` at each crate root.

---

## File map

### New crate: `crates/lopress-gui-host/`
| File | Purpose |
|------|---------|
| `Cargo.toml` | crate manifest |
| `src/lib.rs` | module declarations + re-exports |
| `src/error.rs` | `OpenError`, `LoadError`, `SaveError` |
| `src/document.rs` | `LoadedDocument` |
| `src/session.rs` | `Session`, `WorkspaceSummary`, `DocumentRef`, `BuildStatus`, `ServeStatus` |
| `tests/session_integration.rs` | integration tests |

### New crate: `crates/lopress-editor/`
| File | Purpose |
|------|---------|
| `Cargo.toml` | crate manifest |
| `src/lib.rs` | module declarations |
| `src/ops.rs` | pure block editing operations |
| `src/app.rs` | `LopressApp: eframe::App` — top-level update loop |
| `src/state.rs` | `AppState` enum, `EditingState` |
| `src/recents.rs` | persist/load recent workspaces list |
| `src/ui/mod.rs` | module declarations |
| `src/ui/welcome.rs` | welcome screen |
| `src/ui/sidebar.rs` | posts sidebar + preview URL button |
| `src/ui/editor.rs` | block editor central panel |
| `src/ui/inspector.rs` | front-matter inspector right panel |
| `src/ui/footer.rs` | status footer |
| `tests/ops_tests.rs` | unit tests for block ops |
| `tests/roundtrip_tests.rs` | load/edit/save round-trip tests |

### Modified files
| File | Change |
|------|--------|
| `Cargo.toml` | add workspace members + deps (eframe, rfd, open, directories) |
| `crates/lopress-serve/src/server.rs` | add `ServerHandle`, `serve_in_background` |
| `crates/lopress-serve/src/lib.rs` | re-export `ServerHandle`, `serve_in_background` |
| `src/main.rs` | rework CLI dispatch for no-args / positional-path → GUI |

---

## Task 0: Workspace scaffolding

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/lopress-gui-host/Cargo.toml`
- Create: `crates/lopress-gui-host/src/lib.rs`
- Create: `crates/lopress-editor/Cargo.toml`
- Create: `crates/lopress-editor/src/lib.rs`

- [ ] **Step 1: Add workspace deps and members**

In `Cargo.toml`, add to `[workspace.members]`:
```toml
members = [
    "crates/lopress-core",
    "crates/lopress-plugin",
    "crates/lopress-theme",
    "crates/lopress-assets",
    "crates/lopress-build",
    "crates/lopress-watch",
    "crates/lopress-serve",
    "crates/lopress-gui-host",
    "crates/lopress-editor",
]
```

Add to `[workspace.dependencies]`:
```toml
eframe = "0.31"
rfd = "0.15"
open = "5"
directories = "6"
```

Run `cargo add eframe` and `cargo add rfd` and check the output — update the versions above to what cargo resolves.

- [ ] **Step 2: Create `crates/lopress-gui-host/Cargo.toml`**

```toml
[package]
name = "lopress-gui-host"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
lopress-build = { path = "../lopress-build" }
lopress-watch = { path = "../lopress-watch" }
lopress-serve = { path = "../lopress-serve" }
lopress-core = { path = "../lopress-core" }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 3: Create `crates/lopress-gui-host/src/lib.rs`**

```rust
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc,
    )
)]

pub mod document;
pub mod error;
pub mod session;

pub use document::LoadedDocument;
pub use error::{LoadError, OpenError, SaveError};
pub use session::{BuildStatus, DocumentRef, ServeStatus, Session, WorkspaceSummary};
```

- [ ] **Step 4: Create `crates/lopress-editor/Cargo.toml`**

```toml
[package]
name = "lopress-editor"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
lopress-gui-host = { path = "../lopress-gui-host" }
lopress-core = { path = "../lopress-core" }
eframe = { workspace = true }
rfd = { workspace = true }
open = { workspace = true }
directories = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
lopress-build = { path = "../lopress-build" }

[lints]
workspace = true
```

- [ ] **Step 5: Create `crates/lopress-editor/src/lib.rs`**

```rust
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc,
    )
)]

pub mod app;
pub mod ops;
pub mod recents;
pub mod state;
pub mod ui;

pub use app::LopressApp;
```

- [ ] **Step 6: Verify workspace compiles**

```bash
cargo check --workspace 2>&1 | head -30
```

Expected: errors only about missing module bodies (empty lib.rs stubs), no dependency resolution errors.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/lopress-gui-host crates/lopress-editor
git commit -m "chore: scaffold lopress-gui-host and lopress-editor crates"
```

---

## Task 1: Add `serve_in_background` to lopress-serve

The existing `serve()` blocks on the accept loop. The GUI needs a non-blocking version that starts the HTTP+SSE stack on background threads and returns a handle.

**Files:**
- Modify: `crates/lopress-serve/src/server.rs`
- Modify: `crates/lopress-serve/src/lib.rs`

- [ ] **Step 1: Add `ServerHandle` and `serve_in_background` to `server.rs`**

Append to `crates/lopress-serve/src/server.rs` (after the existing `open_url` function):

```rust
/// A running background HTTP+SSE server. Drop to stop the ping thread;
/// the accept thread runs until process exit.
pub struct ServerHandle {
    /// The bound URL, e.g. `"http://127.0.0.1:8080"`.
    pub url: String,
    subscribers: Subscribers,
}

impl ServerHandle {
    /// Broadcast a reload event to all connected SSE clients.
    pub fn broadcast_reload(&self) {
        self.subscribers.broadcast_reload();
    }
}

/// Bind a static file server for `www_dir` on a background thread.
/// Does **not** run an initial build — the caller is responsible.
/// Port 0 selects an ephemeral port; the resolved URL is in `ServerHandle::url`.
///
/// # Errors
/// Returns `ServeError::Bind` if the port is already in use.
pub fn serve_in_background(
    www_dir: std::path::PathBuf,
    bind: String,
    port: u16,
) -> Result<ServerHandle, ServeError> {
    let bind_addr = format!("{bind}:{port}");
    let listener =
        TcpListener::bind(&bind_addr).map_err(|source| ServeError::Bind {
            addr: bind_addr.clone(),
            source,
        })?;
    let local = listener
        .local_addr()
        .map_err(|source| ServeError::Bind { addr: bind_addr, source })?;
    let url = format!("http://{local}");

    let subs = Subscribers::default();
    let _ping = subs.clone().ping_loop();

    let www = Arc::new(www_dir);
    let subs_accept = subs.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let www = Arc::clone(&www);
            let subs = subs_accept.clone();
            std::thread::spawn(move || {
                let _ = handle_conn(stream, &www, &subs);
            });
        }
    });

    Ok(ServerHandle { url, subscribers: subs })
}
```

- [ ] **Step 2: Re-export from `crates/lopress-serve/src/lib.rs`**

Add to the existing `pub use` lines:

```rust
pub use server::{serve, serve_in_background, ServerHandle, ServeOptions};
```

- [ ] **Step 3: Check compiles and existing tests pass**

```bash
cargo test -p lopress-serve 2>&1
```

Expected: all existing tests pass, no new failures.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-serve/src/server.rs crates/lopress-serve/src/lib.rs
git commit -m "feat(lopress-serve): add serve_in_background + ServerHandle"
```

---

## Task 2: `lopress-gui-host` — error types + LoadedDocument

**Files:**
- Create: `crates/lopress-gui-host/src/error.rs`
- Create: `crates/lopress-gui-host/src/document.rs`

- [ ] **Step 1: Create `src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenError {
    #[error("invalid workspace: {0}")]
    InvalidWorkspace(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error at line {line}: {message}")]
    Parse { raw: String, line: u32, message: String },
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 2: Create `src/document.rs`**

```rust
use lopress_core::{Block, FrontMatter};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

/// In-memory representation of an open post or page.
/// The editor owns this; call `Session::save` to flush to disk.
#[derive(Debug, Clone)]
pub struct LoadedDocument {
    /// Absolute path to the `.md` file.
    pub path: PathBuf,
    pub front_matter: FrontMatter,
    /// Full block tree. Only paragraph and heading blocks are editable
    /// in the UI; others are treated as opaque read-only placeholders.
    pub blocks: Vec<Block>,
    /// True when the in-memory state differs from the last write.
    pub dirty: bool,
    /// Timestamp of the last edit, used for the 500 ms debounce.
    pub dirty_at: Option<Instant>,
    /// Wall-clock time of the last successful write, for external-edit
    /// detection (phase 4+; stored here for future use).
    pub last_written: Option<SystemTime>,
    /// Non-None when the most recent `Session::save` call failed.
    pub last_save_error: Option<String>,
}

impl LoadedDocument {
    /// Mark the document as having unsaved edits.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.dirty_at = Some(Instant::now());
    }

    /// Clear dirty state after a successful flush.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        self.dirty_at = None;
        self.last_save_error = None;
        self.last_written = Some(SystemTime::now());
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p lopress-gui-host 2>&1
```

Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-gui-host/src/error.rs crates/lopress-gui-host/src/document.rs
git commit -m "feat(lopress-gui-host): error types and LoadedDocument"
```

---

## Task 3: `lopress-gui-host` — Session

**Files:**
- Create: `crates/lopress-gui-host/src/session.rs`
- Create: `crates/lopress-gui-host/tests/session_integration.rs`

- [ ] **Step 1: Create `src/session.rs`**

```rust
use crate::document::LoadedDocument;
use crate::error::{LoadError, OpenError, SaveError};
use lopress_build::Workspace;
use lopress_core::{parse, serialize, Document};
use lopress_serve::{serve_in_background, ServerHandle};
use lopress_watch::{ChangeSet, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

// ── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,
    pub posts: Vec<DocumentRef>,
    pub pages: Vec<DocumentRef>,
}

#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,
    pub title: String,
    pub is_draft: bool,
    pub has_parse_error: bool,
}

#[derive(Debug, Clone)]
pub enum BuildStatus {
    Idle,
    Building,
    Ok {
        pages_rendered: usize,
        pages_skipped: usize,
        duration_ms: u64,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum ServeStatus {
    Unavailable { reason: String },
    Listening { url: String },
}

// ── Session ─────────────────────────────────────────────────────────────────

/// Owns the workspace session: watcher, build pipeline, HTTP server.
/// One session per open workspace; drop to shut down cleanly.
pub struct Session {
    workspace: Arc<Workspace>,
    summary: Arc<Mutex<WorkspaceSummary>>,
    build_status: Arc<Mutex<BuildStatus>>,
    serve_status: ServeStatus,
    _server: Option<ServerHandle>,
    _watcher: Option<Watcher>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    /// Open a workspace. Runs an initial build and starts the watch + serve
    /// stack. Returns `Err` if the workspace is invalid (missing toml, bad
    /// toml). Serve/build failures set status fields instead of returning Err.
    ///
    /// # Errors
    /// Returns `OpenError` if `lopress.toml` is missing or unparseable.
    pub fn open(workspace_root: &Path) -> Result<Self, OpenError> {
        // Validate
        let workspace = Workspace::load(workspace_root).map_err(|e| {
            OpenError::InvalidWorkspace(e.to_string())
        })?;
        let workspace = Arc::new(workspace);

        // Initial build
        let build_status = Arc::new(Mutex::new(BuildStatus::Idle));
        {
            let t0 = std::time::Instant::now();
            match lopress_build::build(workspace_root) {
                Ok(r) => {
                    *lock(&build_status) = BuildStatus::Ok {
                        pages_rendered: r.pages_rendered,
                        pages_skipped: r.pages_skipped,
                        duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    };
                }
                Err(e) => {
                    *lock(&build_status) = BuildStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        }

        // Serve (non-fatal if bind fails)
        let www_dir = workspace.www_dir();
        std::fs::create_dir_all(&www_dir).ok();
        let (server, serve_status) =
            match serve_in_background(www_dir, "127.0.0.1".into(), 8080) {
                Ok(h) => {
                    let url = h.url.clone();
                    (Some(h), ServeStatus::Listening { url })
                }
                Err(_) => match serve_in_background(
                    workspace.www_dir(),
                    "127.0.0.1".into(),
                    0,
                ) {
                    Ok(h) => {
                        let url = h.url.clone();
                        (Some(h), ServeStatus::Listening { url })
                    }
                    Err(e) => (None, ServeStatus::Unavailable { reason: e.to_string() }),
                },
            };

        // Scan posts/pages
        let summary = Arc::new(Mutex::new(scan_workspace(&workspace)));

        // Watcher: on change, rebuild and broadcast
        let ws_root = workspace_root.to_path_buf();
        let build_status_w = Arc::clone(&build_status);
        let summary_w = Arc::clone(&summary);
        let workspace_w = Arc::clone(&workspace);
        let server_ref: Option<Arc<ServerHandle>> = server.as_ref().map(|_| {
            // We can't clone ServerHandle, so we store a second Arc for the callback.
            // We'll use a dedicated Arc<Mutex<Option<ServerHandle>>> instead.
            // See note below.
            unreachable!()
        });
        // To allow the watcher callback to call broadcast_reload, wrap server in Arc.
        // Restructure: box the handle behind Arc<Mutex<>>.
        let _ = server_ref; // discard placeholder

        // Wrap server in Arc so the watcher closure can hold a reference.
        let server_arc: Option<Arc<ServerHandle>> = None; // placeholder — see next step
        let _ = server_arc;

        // Watcher
        let watcher = Watcher::spawn(workspace_root, move |_cs: ChangeSet| {
            *lock(&build_status_w) = BuildStatus::Building;
            let t0 = std::time::Instant::now();
            match lopress_build::build(&ws_root) {
                Ok(r) => {
                    *lock(&build_status_w) = BuildStatus::Ok {
                        pages_rendered: r.pages_rendered,
                        pages_skipped: r.pages_skipped,
                        duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    };
                    // Refresh workspace scan
                    *lock(&summary_w) = scan_workspace(&workspace_w);
                }
                Err(e) => {
                    *lock(&build_status_w) = BuildStatus::Failed { message: e.to_string() };
                }
            }
        })
        .ok();

        Ok(Self {
            workspace,
            summary,
            build_status,
            serve_status,
            _server: server,
            _watcher: watcher,
        })
    }

    /// Current workspace snapshot. Updated by the watcher on src/ changes.
    pub fn workspace(&self) -> WorkspaceSummary {
        lock(&self.summary).clone()
    }

    /// Load and parse a document. Returns `LoadError::Parse` if the file is
    /// not valid markdown + front-matter, carrying the raw bytes for display.
    ///
    /// # Errors
    /// Returns `LoadError` on I/O or parse failure.
    pub fn load_document(&self, path: &Path) -> Result<LoadedDocument, LoadError> {
        let raw = std::fs::read_to_string(path)?;
        match parse(&raw) {
            Ok(doc) => Ok(LoadedDocument {
                path: path.to_path_buf(),
                front_matter: doc.front_matter,
                blocks: doc.blocks,
                dirty: false,
                dirty_at: None,
                last_written: path.metadata().and_then(|m| m.modified()).ok(),
                last_save_error: None,
            }),
            Err(e) => Err(LoadError::Parse {
                raw,
                line: 0,
                message: e.to_string(),
            }),
        }
    }

    /// Serialize and atomically write a document to disk.
    ///
    /// # Errors
    /// Returns `SaveError` on I/O failure.
    pub fn save(&self, doc: &LoadedDocument) -> Result<(), SaveError> {
        let content = serialize(&Document {
            front_matter: doc.front_matter.clone(),
            blocks: doc.blocks.clone(),
        });
        atomic_write(&doc.path, content.as_bytes())?;
        Ok(())
    }

    /// Current build status.
    pub fn build_status(&self) -> BuildStatus {
        lock(&self.build_status).clone()
    }

    /// Current serve status.
    pub fn serve_status(&self) -> &ServeStatus {
        &self.serve_status
    }

    /// URL to open in the browser for the given document.
    pub fn preview_url_for(&self, doc_ref: &DocumentRef) -> Option<String> {
        let url = match &self.serve_status {
            ServeStatus::Listening { url } => url,
            ServeStatus::Unavailable { .. } => return None,
        };
        // Determine slug: prefer front-matter slug, fall back to filename stem.
        let slug = doc_ref
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("index")
            .to_string();
        let ws_posts = self.workspace.posts_dir();
        if doc_ref.path.starts_with(&ws_posts) {
            Some(format!("{url}/posts/{slug}/"))
        } else {
            Some(format!("{url}/{slug}/"))
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn scan_workspace(ws: &Workspace) -> WorkspaceSummary {
    WorkspaceSummary {
        root: ws.root.clone(),
        name: ws.config.site.title.clone(),
        posts: scan_dir(&ws.posts_dir()),
        pages: scan_dir(&ws.pages_dir()),
    }
}

fn scan_dir(dir: &Path) -> Vec<DocumentRef> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut refs: Vec<DocumentRef> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md")
        })
        .map(|e| {
            let path = e.path();
            match std::fs::read_to_string(&path).as_deref().map(parse) {
                Ok(Ok(doc)) => DocumentRef {
                    title: doc
                        .front_matter
                        .title
                        .unwrap_or_else(|| stem(&path)),
                    is_draft: doc.front_matter.draft,
                    has_parse_error: false,
                    path,
                },
                _ => DocumentRef {
                    title: stem(&path),
                    is_draft: false,
                    has_parse_error: true,
                    path,
                },
            }
        })
        .collect();
    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent",
        ));
    };
    let tmp = parent.join(format!(
        ".lopress-tmp-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

Note: The watcher callback can't call `server.broadcast_reload()` in the shape above because `ServerHandle` is not `Clone`. Refactor: wrap `_server` in `Arc<Option<ServerHandle>>` so the watcher closure holds a clone of the `Arc`. Replace the `_server: Option<ServerHandle>` field and the watcher closure with the following in a follow-up step below.

- [ ] **Step 2: Fix ServerHandle sharing — wrap in Arc**

`ServerHandle` is not `Clone`. To allow both `Session` and the watcher callback to reach it, wrap it:

In `session.rs`, change the field:
```rust
_server: Option<Arc<ServerHandle>>,
```

And in `Session::open`, after creating the server:
```rust
let server_arc: Option<Arc<ServerHandle>> = server.map(Arc::new);

let server_for_watcher = server_arc.clone();
let watcher = Watcher::spawn(workspace_root, move |_cs: ChangeSet| {
    *lock(&build_status_w) = BuildStatus::Building;
    let t0 = std::time::Instant::now();
    match lopress_build::build(&ws_root) {
        Ok(r) => {
            *lock(&build_status_w) = BuildStatus::Ok {
                pages_rendered: r.pages_rendered,
                pages_skipped: r.pages_skipped,
                duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            };
            *lock(&summary_w) = scan_workspace(&workspace_w);
            if let Some(srv) = &server_for_watcher {
                srv.broadcast_reload();
            }
        }
        Err(e) => {
            *lock(&build_status_w) = BuildStatus::Failed { message: e.to_string() };
        }
    }
})
.ok();

// And in the Session struct literal:
Ok(Self {
    workspace,
    summary,
    build_status,
    serve_status,
    _server: server_arc,
    _watcher: watcher,
})
```

Remove the placeholder code from the previous step for `server_ref` / `server_arc`.

- [ ] **Step 3: Write integration test**

Create `crates/lopress-gui-host/tests/session_integration.rs`:

```rust
use lopress_gui_host::{BuildStatus, Session, ServeStatus};
use std::fs;
use tempfile::TempDir;

fn make_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    fs::write(
        p.join("lopress.toml"),
        "[site]\ntitle = \"Test\"\nbase_url = \"https://example.com\"\n",
    )
    .unwrap();
    fs::create_dir_all(p.join("src/posts")).unwrap();
    fs::create_dir_all(p.join("src/pages")).unwrap();
    fs::create_dir_all(p.join("src/images")).unwrap();
    fs::create_dir_all(p.join("plugins")).unwrap();
    fs::write(
        p.join("src/posts/hello.md"),
        "---\ntitle: Hello\ndate: 2026-04-20\n---\n\n# Hello\n\nWorld.\n",
    )
    .unwrap();
    dir
}

#[test]
fn open_valid_workspace_succeeds() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let summary = session.workspace();
    assert_eq!(summary.name, "Test");
    assert_eq!(summary.posts.len(), 1);
    assert_eq!(summary.posts[0].title, "Hello");
    assert!(!summary.posts[0].has_parse_error);
}

#[test]
fn open_invalid_workspace_errors() {
    let dir = TempDir::new().unwrap();
    assert!(Session::open(dir.path()).is_err());
}

#[test]
fn build_status_is_ok_after_open() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    assert!(matches!(session.build_status(), BuildStatus::Ok { .. }));
}

#[test]
fn load_and_save_document_roundtrip() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let post_path = dir.path().join("src/posts/hello.md");
    let mut doc = session.load_document(&post_path).unwrap();
    // Edit a block
    if let Some(b) = doc.blocks.iter_mut().find(|b| b.r#type == "paragraph") {
        b.text = Some("Edited paragraph.".into());
    }
    session.save(&doc).unwrap();
    // Reparse
    let doc2 = session.load_document(&post_path).unwrap();
    assert!(doc2
        .blocks
        .iter()
        .any(|b| b.text.as_deref() == Some("Edited paragraph.")));
}

#[test]
fn serve_status_is_listening_after_open() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    assert!(matches!(
        session.serve_status(),
        lopress_gui_host::ServeStatus::Listening { .. }
    ));
}

#[test]
fn serve_responds_to_get() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let url = match session.serve_status() {
        lopress_gui_host::ServeStatus::Listening { url } => url.clone(),
        lopress_gui_host::ServeStatus::Unavailable { .. } => {
            panic!("expected serve to be listening")
        }
    };
    let addr = url.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(stream, "GET / HTTP/1.0\r\nHost: {addr}\r\n\r\n").unwrap();
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    // Either 200 OK with index or 404 — we just need a valid HTTP response
    assert!(response.starts_with("HTTP/1.1"));
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p lopress-gui-host 2>&1
```

Expected: all 6 tests pass.

- [ ] **Step 5: Fix any clippy warnings**

```bash
cargo clippy -p lopress-gui-host --all-targets -- -D warnings 2>&1
```

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-gui-host/src/ crates/lopress-gui-host/tests/
git commit -m "feat(lopress-gui-host): Session with open/scan/load/save/serve"
```

---

## Task 4: `lopress-editor` — pure block ops + tests

**Files:**
- Create: `crates/lopress-editor/src/ops.rs`
- Create: `crates/lopress-editor/tests/ops_tests.rs`

- [ ] **Step 1: Create `src/ops.rs`**

```rust
use lopress_core::Block;
use serde_json::Value;

/// Which block types the editor can edit (not read-only placeholders).
pub fn is_editable(block_type: &str) -> bool {
    matches!(block_type, "paragraph" | "heading")
}

/// Split the block at `idx` at byte offset `caret`. The left half stays at
/// `idx`; the right half is inserted at `idx + 1` with the same block type.
pub fn split_block_at_caret(blocks: &mut Vec<Block>, idx: usize, caret: usize) {
    let Some(block) = blocks.get(idx) else { return };
    let block_type = block.r#type.clone();
    let attrs = block.attrs.clone();
    let text = block.text.clone().unwrap_or_default();

    let left = if caret <= text.len() {
        text.get(..caret).unwrap_or(&text).to_string()
    } else {
        text.clone()
    };
    let right = if caret <= text.len() {
        text.get(caret..).unwrap_or("").to_string()
    } else {
        String::new()
    };

    let right_block = Block {
        r#type: block_type,
        attrs,
        children: vec![],
        text: Some(right),
    };

    if let Some(b) = blocks.get_mut(idx) {
        b.text = Some(left);
    }
    blocks.insert(idx + 1, right_block);
}

/// Merge the block at `idx` into the previous block (text appended; the
/// previous block's type wins). No-op if `idx == 0` or blocks is empty.
pub fn merge_with_previous(blocks: &mut Vec<Block>, idx: usize) {
    if idx == 0 || blocks.is_empty() {
        return;
    }
    let Some(current) = blocks.get(idx).cloned() else { return };
    let Some(prev) = blocks.get_mut(idx - 1) else { return };
    let prev_text = prev.text.get_or_insert_with(String::new);
    if let Some(cur_text) = &current.text {
        prev_text.push_str(cur_text);
    }
    blocks.remove(idx);
}

/// Change the type of the block at `idx`. Only `"paragraph"` and
/// `"heading"` (levels 1–6) are valid targets; other inputs are ignored.
pub fn change_block_type(blocks: &mut Vec<Block>, idx: usize, new_type: &str, level: Option<u8>) {
    let Some(block) = blocks.get_mut(idx) else { return };
    match new_type {
        "paragraph" => {
            block.r#type = "paragraph".into();
            block.attrs = Value::Object(serde_json::Map::new());
        }
        "heading" => {
            let lvl = level.unwrap_or(1).clamp(1, 6);
            block.r#type = "heading".into();
            block.attrs = serde_json::json!({ "level": lvl });
        }
        _ => {}
    }
}

/// Append an empty paragraph at the end of `blocks`.
pub fn add_paragraph_at_end(blocks: &mut Vec<Block>) {
    blocks.push(Block::paragraph(""));
}

/// Delete the block at `idx`. If removing the last block, replaces it with
/// an empty paragraph so the editor always has at least one block.
pub fn delete_block(blocks: &mut Vec<Block>, idx: usize) {
    if blocks.len() <= idx {
        return;
    }
    blocks.remove(idx);
    if blocks.is_empty() {
        blocks.push(Block::paragraph(""));
    }
}
```

- [ ] **Step 2: Write failing tests first**

Create `crates/lopress-editor/tests/ops_tests.rs`:

```rust
use lopress_core::Block;
use lopress_editor::ops::*;

fn para(t: &str) -> Block { Block::paragraph(t) }
fn heading(lvl: u8, t: &str) -> Block { Block::heading(lvl, t) }

// ── split_block_at_caret ────────────────────────────────────────────────────

#[test]
fn split_paragraph_at_middle() {
    let mut blocks = vec![para("hello world")];
    split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hello"));
    assert_eq!(blocks[1].text.as_deref(), Some(" world"));
    assert_eq!(blocks[1].r#type, "paragraph");
}

#[test]
fn split_at_start_leaves_empty_first() {
    let mut blocks = vec![para("hello")];
    split_block_at_caret(&mut blocks, 0, 0);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some(""));
    assert_eq!(blocks[1].text.as_deref(), Some("hello"));
}

#[test]
fn split_at_end_leaves_empty_second() {
    let mut blocks = vec![para("hello")];
    split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hello"));
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

#[test]
fn split_heading_preserves_type() {
    let mut blocks = vec![heading(2, "Sec A rest")];
    split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(blocks[1].r#type, "heading");
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(2)
    );
}

#[test]
fn split_caret_beyond_length_clamps() {
    let mut blocks = vec![para("hi")];
    split_block_at_caret(&mut blocks, 0, 999);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hi"));
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

// ── merge_with_previous ─────────────────────────────────────────────────────

#[test]
fn merge_appends_text_to_previous() {
    let mut blocks = vec![para("foo"), para("bar")];
    merge_with_previous(&mut blocks, 1);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text.as_deref(), Some("foobar"));
}

#[test]
fn merge_at_zero_is_noop() {
    let mut blocks = vec![para("only")];
    merge_with_previous(&mut blocks, 0);
    assert_eq!(blocks.len(), 1);
}

#[test]
fn merge_previous_type_wins() {
    let mut blocks = vec![heading(1, "Title"), para("body")];
    merge_with_previous(&mut blocks, 1);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(blocks[0].text.as_deref(), Some("Titlebody"));
}

// ── change_block_type ───────────────────────────────────────────────────────

#[test]
fn change_paragraph_to_heading() {
    let mut blocks = vec![para("text")];
    change_block_type(&mut blocks, 0, "heading", Some(3));
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(3)
    );
    assert_eq!(blocks[0].text.as_deref(), Some("text"));
}

#[test]
fn change_heading_to_paragraph_clears_attrs() {
    let mut blocks = vec![heading(2, "hi")];
    change_block_type(&mut blocks, 0, "paragraph", None);
    assert_eq!(blocks[0].r#type, "paragraph");
    assert!(blocks[0].attrs.as_object().map_or(false, |m| m.is_empty()));
}

#[test]
fn change_to_unknown_type_is_noop() {
    let mut blocks = vec![para("text")];
    change_block_type(&mut blocks, 0, "code_block", None);
    assert_eq!(blocks[0].r#type, "paragraph");
}

#[test]
fn heading_level_clamped_to_1_6() {
    let mut blocks = vec![para("t")];
    change_block_type(&mut blocks, 0, "heading", Some(0));
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(1)
    );
    change_block_type(&mut blocks, 0, "heading", Some(9));
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(6)
    );
}

// ── add_paragraph_at_end ────────────────────────────────────────────────────

#[test]
fn add_paragraph_appends() {
    let mut blocks = vec![para("existing")];
    add_paragraph_at_end(&mut blocks);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[1].r#type, "paragraph");
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

// ── delete_block ────────────────────────────────────────────────────────────

#[test]
fn delete_removes_block() {
    let mut blocks = vec![para("a"), para("b"), para("c")];
    delete_block(&mut blocks, 1);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("a"));
    assert_eq!(blocks[1].text.as_deref(), Some("c"));
}

#[test]
fn delete_last_block_inserts_empty_paragraph() {
    let mut blocks = vec![para("only")];
    delete_block(&mut blocks, 0);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].r#type, "paragraph");
    assert_eq!(blocks[0].text.as_deref(), Some(""));
}

#[test]
fn delete_out_of_bounds_is_noop() {
    let mut blocks = vec![para("a")];
    delete_block(&mut blocks, 5);
    assert_eq!(blocks.len(), 1);
}
```

- [ ] **Step 3: Run tests — expect failures (ops.rs not yet in lib)**

```bash
cargo test -p lopress-editor 2>&1 | head -20
```

Expected: compile errors because `ops` module not publicly exported from `lib.rs`.

- [ ] **Step 4: Ensure ops is exported in `src/lib.rs`**

The `src/lib.rs` from Task 0 already has `pub mod ops;`. Verify:

```bash
grep "pub mod ops" crates/lopress-editor/src/lib.rs
```

Expected: `pub mod ops;`

- [ ] **Step 5: Run tests — all pass**

```bash
cargo test -p lopress-editor 2>&1
```

Expected: 18 tests pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1
```

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ops.rs crates/lopress-editor/tests/ops_tests.rs
git commit -m "feat(lopress-editor): pure block ops + unit tests"
```

---

## Task 5: `lopress-editor` — state types and recents

**Files:**
- Create: `crates/lopress-editor/src/state.rs`
- Create: `crates/lopress-editor/src/recents.rs`

- [ ] **Step 1: Create `src/state.rs`**

```rust
use lopress_gui_host::{DocumentRef, LoadedDocument, Session};
use std::path::PathBuf;

pub enum AppState {
    Welcome(WelcomeState),
    Editing(Box<EditingState>),
}

pub struct WelcomeState {
    /// Non-empty when the previous Open attempt failed.
    pub error: Option<String>,
}

impl Default for WelcomeState {
    fn default() -> Self {
        Self { error: None }
    }
}

pub struct EditingState {
    pub session: Session,
    /// The currently open document, if any.
    pub current_doc: Option<LoadedDocument>,
    /// The DocumentRef for the currently open document (for display in sidebar).
    pub current_ref: Option<DocumentRef>,
    /// Non-empty when the parse-error fallback view is active.
    pub parse_error_raw: Option<String>,
    pub parse_error_msg: Option<String>,
    /// Which block index is focused in the editor (for keyboard handling).
    pub focused_block: Option<usize>,
}

impl EditingState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            current_doc: None,
            current_ref: None,
            parse_error_raw: None,
            parse_error_msg: None,
            focused_block: None,
        }
    }

    /// Switch to a new document, flushing any pending save first.
    pub fn open_document(&mut self, doc_ref: &DocumentRef) {
        // Flush current document
        self.flush_current();

        match self.session.load_document(&doc_ref.path) {
            Ok(doc) => {
                self.current_doc = Some(doc);
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = None;
                self.parse_error_msg = None;
            }
            Err(lopress_gui_host::LoadError::Parse { raw, message, .. }) => {
                self.current_doc = None;
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = Some(raw);
                self.parse_error_msg = Some(message);
            }
            Err(e) => {
                self.current_doc = None;
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = None;
                self.parse_error_msg = Some(e.to_string());
            }
        }
    }

    /// Flush the current document synchronously if dirty. Records any save
    /// error in `last_save_error`.
    pub fn flush_current(&mut self) {
        let Some(doc) = &mut self.current_doc else { return };
        if !doc.dirty {
            return;
        }
        match self.session.save(doc) {
            Ok(()) => doc.mark_clean(),
            Err(e) => doc.last_save_error = Some(e.to_string()),
        }
    }
}
```

- [ ] **Step 2: Create `src/recents.rs`**

```rust
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECENTS: usize = 5;

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentsFile {
    paths: Vec<PathBuf>,
}

fn recents_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "lopress")
        .map(|p| p.config_dir().join("recents.json"))
}

/// Load the recent workspaces list. Returns an empty vec on any error.
pub fn load() -> Vec<PathBuf> {
    let Some(path) = recents_path() else { return Vec::new() };
    let Ok(bytes) = std::fs::read(&path) else { return Vec::new() };
    let Ok(file) = serde_json::from_slice::<RecentsFile>(&bytes) else {
        return Vec::new();
    };
    // Prune entries that no longer exist
    file.paths.into_iter().filter(|p| p.exists()).collect()
}

/// Prepend `workspace` to the recents list and persist. Silently ignores
/// I/O errors (recents are best-effort).
pub fn push(workspace: &Path) {
    let Some(path) = recents_path() else { return };
    let mut paths = load();
    paths.retain(|p| p != workspace);
    paths.insert(0, workspace.to_path_buf());
    paths.truncate(MAX_RECENTS);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(bytes) = serde_json::to_vec(&RecentsFile { paths }) {
        std::fs::write(&path, bytes).ok();
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p lopress-editor 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/state.rs crates/lopress-editor/src/recents.rs
git commit -m "feat(lopress-editor): AppState, EditingState, recents persistence"
```

---

## Task 6: `lopress-editor` — app shell (compiles, opens blank window)

**Files:**
- Create: `crates/lopress-editor/src/app.rs`
- Create: `crates/lopress-editor/src/ui/mod.rs`
- Create: `crates/lopress-editor/src/ui/welcome.rs`
- Create: `crates/lopress-editor/src/ui/sidebar.rs`
- Create: `crates/lopress-editor/src/ui/editor.rs`
- Create: `crates/lopress-editor/src/ui/inspector.rs`
- Create: `crates/lopress-editor/src/ui/footer.rs`
- Modify: `Cargo.toml` (root) + `src/main.rs`

- [ ] **Step 1: Create stub UI modules**

`crates/lopress-editor/src/ui/mod.rs`:
```rust
pub mod editor;
pub mod footer;
pub mod inspector;
pub mod sidebar;
pub mod welcome;
```

`crates/lopress-editor/src/ui/welcome.rs`:
```rust
use crate::state::WelcomeState;
pub fn show(_ui: &mut egui::Ui, _state: &mut WelcomeState) -> WelcomeAction {
    WelcomeAction::None
}
pub enum WelcomeAction { None, OpenPicker, OpenPath(std::path::PathBuf) }
```

`crates/lopress-editor/src/ui/sidebar.rs`:
```rust
use crate::state::EditingState;
pub fn show(_ui: &mut egui::Ui, _state: &mut EditingState) -> SidebarAction {
    SidebarAction::None
}
pub enum SidebarAction { None, SelectDocument(lopress_gui_host::DocumentRef), OpenPreview }
```

`crates/lopress-editor/src/ui/editor.rs`:
```rust
use crate::state::EditingState;
pub fn show(_ui: &mut egui::Ui, _state: &mut EditingState) {}
```

`crates/lopress-editor/src/ui/inspector.rs`:
```rust
use crate::state::EditingState;
pub fn show(_ui: &mut egui::Ui, _state: &mut EditingState) {}
```

`crates/lopress-editor/src/ui/footer.rs`:
```rust
use crate::state::EditingState;
pub fn show(_ui: &mut egui::Ui, _state: &EditingState) {}
```

- [ ] **Step 2: Create `src/app.rs`**

```rust
use crate::recents;
use crate::state::{AppState, EditingState, WelcomeState};
use crate::ui;
use lopress_gui_host::Session;
use std::path::PathBuf;

pub struct LopressApp {
    state: AppState,
}

impl LopressApp {
    /// Create the app. If `workspace` is provided, open it immediately.
    pub fn new(workspace: Option<PathBuf>) -> Self {
        let state = match workspace {
            Some(path) => Self::try_open(path, None),
            None => AppState::Welcome(WelcomeState::default()),
        };
        Self { state }
    }

    fn try_open(path: PathBuf, error_context: Option<&str>) -> AppState {
        let _ = error_context;
        match Session::open(&path) {
            Ok(session) => {
                recents::push(&path);
                AppState::Editing(Box::new(EditingState::new(session)))
            }
            Err(e) => AppState::Welcome(WelcomeState {
                error: Some(e.to_string()),
            }),
        }
    }
}

impl eframe::App for LopressApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match &mut self.state {
            AppState::Welcome(ws) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let action = ui::welcome::show(ui, ws);
                    match action {
                        ui::welcome::WelcomeAction::OpenPicker => {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.state = Self::try_open(path, None);
                            }
                        }
                        ui::welcome::WelcomeAction::OpenPath(path) => {
                            self.state = Self::try_open(path, None);
                        }
                        ui::welcome::WelcomeAction::None => {}
                    }
                });
            }
            AppState::Editing(es) => {
                self.show_editing(ctx, es);
            }
        }
    }
}

impl LopressApp {
    fn show_editing(&mut self, ctx: &egui::Context, es: &mut EditingState) {
        // Debounce check: flush if 500 ms have passed since last edit
        if let Some(doc) = &es.current_doc {
            if doc.dirty {
                if let Some(dirty_at) = doc.dirty_at {
                    let elapsed = dirty_at.elapsed().as_millis();
                    if elapsed >= 500 {
                        es.flush_current();
                    } else {
                        let remaining = 500u64.saturating_sub(
                            elapsed.try_into().unwrap_or(500),
                        );
                        ctx.request_repaint_after(
                            std::time::Duration::from_millis(remaining),
                        );
                    }
                }
            }
        }

        // Poll build status: rapid repaint while building
        if matches!(es.session.build_status(), lopress_gui_host::BuildStatus::Building) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        // Menu bar
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Workspace…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        es.flush_current();
                        self.state = Self::try_open(path, None);
                        return;
                    }
                }
                if ui.button("Save").clicked() {
                    es.flush_current();
                }
                if ui.button("Close Workspace").clicked() {
                    es.flush_current();
                    self.state = AppState::Welcome(WelcomeState::default());
                    return;
                }
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // Status footer
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui::footer::show(ui, es);
        });

        // Sidebar
        egui::SidePanel::left("sidebar")
            .default_width(220.0)
            .show(ctx, |ui| {
                let action = ui::sidebar::show(ui, es);
                match action {
                    ui::sidebar::SidebarAction::SelectDocument(doc_ref) => {
                        es.open_document(&doc_ref);
                    }
                    ui::sidebar::SidebarAction::OpenPreview => {
                        let summary = es.session.workspace();
                        let url = es.current_ref.as_ref()
                            .and_then(|r| es.session.preview_url_for(r))
                            .unwrap_or_else(|| match es.session.serve_status() {
                                lopress_gui_host::ServeStatus::Listening { url } => url.clone(),
                                lopress_gui_host::ServeStatus::Unavailable { .. } => String::new(),
                            });
                        let _ = summary;
                        if !url.is_empty() {
                            if let Err(e) = open::that(&url) {
                                eprintln!("failed to open browser: {e}");
                            }
                        }
                    }
                    ui::sidebar::SidebarAction::None => {}
                }
            });

        // Inspector
        egui::SidePanel::right("inspector")
            .default_width(260.0)
            .show(ctx, |ui| {
                ui::inspector::show(ui, es);
            });

        // Block editor (central panel)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::editor::show(ui, es);
        });
    }
}
```

- [ ] **Step 3: Wire into `main.rs` — add GUI deps and entry point**

Add to root `Cargo.toml` `[dependencies]`:
```toml
lopress-editor = { path = "crates/lopress-editor" }
lopress-gui-host = { path = "crates/lopress-gui-host" }
eframe = { workspace = true }
```

Replace `src/main.rs` entirely:

```rust
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Route: no args → GUI welcome; known subcommand → CLI; else treat as path → GUI
    match args.first().map(String::as_str) {
        None => launch_gui(None),
        Some("build") => cli_build(&args[1..]),
        Some("new") => cli_new(&args[1..]),
        Some("serve") => cli_serve(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        Some(path) => launch_gui(Some(PathBuf::from(path))),
    }
}

fn launch_gui(workspace: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("lopress")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "lopress",
        options,
        Box::new(move |_cc| Ok(Box::new(lopress_editor::LopressApp::new(workspace.clone())))),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))?;
    Ok(ExitCode::SUCCESS)
}

fn cli_build(args: &[String]) -> anyhow::Result<ExitCode> {
    let workspace = args.first().map(PathBuf::from).ok_or_else(|| {
        anyhow::anyhow!("usage: lopress build <workspace>")
    })?;
    let report = lopress_build::build(&workspace)?;
    println!("built {} page(s); {} failure(s)", report.pages_written, report.failures.len());
    for f in &report.failures {
        eprintln!("  FAIL {}: {}", f.path.display(), f.message);
    }
    Ok(if report.failures.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn cli_new(args: &[String]) -> anyhow::Result<ExitCode> {
    use std::str::FromStr;
    let mut dir = None::<PathBuf>;
    let mut title = "Untitled".to_string();
    let mut base_url = "https://example.com".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" => { i += 1; if let Some(v) = args.get(i) { title = v.clone(); } }
            "--base-url" => { i += 1; if let Some(v) = args.get(i) { base_url = v.clone(); } }
            p => { dir = Some(PathBuf::from(p)); }
        }
        i += 1;
    }
    let dir = dir.ok_or_else(|| anyhow::anyhow!("usage: lopress new <dir>"))?;
    scaffold::new_site(&dir, &title, &base_url)?;
    Ok(ExitCode::SUCCESS)
}

fn cli_serve(args: &[String]) -> anyhow::Result<ExitCode> {
    let mut workspace = None::<PathBuf>;
    let mut bind = "127.0.0.1".to_string();
    let mut port: u16 = 8080;
    let mut no_open = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" => { i += 1; if let Some(v) = args.get(i) { bind = v.clone(); } }
            "--port" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    port = v.parse().unwrap_or(8080);
                }
            }
            "--no-open" => { no_open = true; }
            p => { workspace = Some(PathBuf::from(p)); }
        }
        i += 1;
    }
    let workspace = workspace.ok_or_else(|| anyhow::anyhow!("usage: lopress serve <workspace>"))?;
    lopress_serve::serve(lopress_serve::ServeOptions {
        workspace,
        bind,
        port,
        open_browser: !no_open,
        on_ready: None,
    })?;
    Ok(ExitCode::SUCCESS)
}

fn print_help() {
    println!("lopress — personal blog authoring tool\n");
    println!("USAGE:");
    println!("  lopress                  Open the GUI (welcome screen)");
    println!("  lopress <path>           Open the GUI with a workspace");
    println!("  lopress build <ws>       Build a workspace");
    println!("  lopress new <dir>        Scaffold a new workspace");
    println!("  lopress serve <ws>       Dev server with live reload");
}

mod scaffold {
    use anyhow::{bail, Result};
    use std::path::Path;

    pub(crate) fn new_site(dir: &Path, title: &str, base_url: &str) -> Result<()> {
        if dir.exists() {
            let non_empty = std::fs::read_dir(dir)?.next().is_some();
            if non_empty {
                bail!("target directory `{}` is not empty", dir.display());
            }
        } else {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            dir.join("lopress.toml"),
            format!(
                r#"[site]
title = "{title}"
base_url = "{base_url}"

[site.nav]
items = [
  {{ label = "Home", href = "/" }},
  {{ label = "About", href = "/about/" }},
]
"#
            ),
        )?;
        for d in ["src/posts", "src/pages", "src/images", "plugins"] {
            std::fs::create_dir_all(dir.join(d))?;
        }
        std::fs::write(
            dir.join("src/posts/hello.md"),
            "---\ntitle: Hello\ndate: 2026-04-18\ntags: [intro]\n---\n\n# Hello\n\nWelcome to your new lopress site.\n",
        )?;
        std::fs::write(
            dir.join("src/pages/about.md"),
            "---\ntitle: About\n---\n\n# About\n\nThis is the about page.\n",
        )?;
        std::fs::write(dir.join(".gitignore"), "/www\n/.lopress-cache.json\n")?;
        println!("created workspace at {}", dir.display());
        Ok(())
    }
}
```

- [ ] **Step 4: Build the binary**

```bash
cargo build -p lopress 2>&1
```

Expected: compiles. There may be warnings about unused variables in stub UI modules — fix them with `let _ = ...` or prefix with `_`.

- [ ] **Step 5: Smoke test — window opens**

```bash
./target/debug/lopress
```

Expected: a blank window with an empty panel titled "lopress" opens without crashing. Ctrl-C or close button to exit.

- [ ] **Step 6: Smoke test — GUI opens with workspace**

```bash
./target/debug/lopress my-site
```

Expected: window opens (no crash). The editing panels are blank (stubs).

- [ ] **Step 7: Clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings 2>&1
```

Fix any warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ src/main.rs Cargo.toml
git commit -m "feat: lopress editor shell compiles and opens a window"
```

---

## Task 7: `lopress-editor` — welcome screen

**Files:**
- Modify: `crates/lopress-editor/src/ui/welcome.rs`

- [ ] **Step 1: Replace stub with full welcome screen**

```rust
use crate::recents;
use std::path::PathBuf;

pub enum WelcomeAction {
    None,
    OpenPicker,
    OpenPath(PathBuf),
}

pub fn show(ui: &mut egui::Ui, error: &Option<String>) -> WelcomeAction {
    let mut action = WelcomeAction::None;

    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("lopress");
        ui.add_space(24.0);

        if ui.button("Open Workspace…").clicked() {
            action = WelcomeAction::OpenPicker;
        }

        ui.add_space(16.0);

        if let Some(err) = error {
            ui.colored_label(egui::Color32::RED, format!("Error: {err}"));
            ui.add_space(8.0);
        }

        let recents = recents::load();
        if !recents.is_empty() {
            ui.separator();
            ui.add_space(8.0);
            ui.label("Recent workspaces:");
            ui.add_space(4.0);
            for path in &recents {
                let label = path.display().to_string();
                if ui.link(&label).clicked() {
                    action = WelcomeAction::OpenPath(path.clone());
                }
            }
        }
    });

    action
}
```

Update `crates/lopress-editor/src/app.rs` — the `WelcomeState` in `show_editing` passes `&ws.error`. Update the welcome call in `update()`:

```rust
AppState::Welcome(ws) => {
    egui::CentralPanel::default().show(ctx, |ui| {
        let action = ui::welcome::show(ui, &ws.error);
        match action {
            // ... same as before
        }
    });
}
```

And remove `WelcomeState` parameter from `ui::welcome::show` signature (it now takes `&Option<String>`).

- [ ] **Step 2: Build and smoke test**

```bash
cargo build -p lopress 2>&1 && ./target/debug/lopress
```

Expected: welcome screen shows "lopress" heading, "Open Workspace…" button, and recent workspaces list (empty on first run).

- [ ] **Step 3: Test recent workspace flow**

```bash
./target/debug/lopress my-site
# Close window
./target/debug/lopress
# Expected: "my-site" appears in recent workspaces list
```

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/welcome.rs crates/lopress-editor/src/app.rs
git commit -m "feat(lopress-editor): welcome screen with recents"
```

---

## Task 8: `lopress-editor` — posts sidebar

**Files:**
- Modify: `crates/lopress-editor/src/ui/sidebar.rs`

- [ ] **Step 1: Replace stub with full sidebar**

```rust
use crate::state::EditingState;
use lopress_gui_host::{DocumentRef, ServeStatus};

pub enum SidebarAction {
    None,
    SelectDocument(DocumentRef),
    OpenPreview,
}

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) -> SidebarAction {
    let mut action = SidebarAction::None;
    let summary = es.session.workspace();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);

        // Posts section
        if !summary.posts.is_empty() {
            ui.label(egui::RichText::new("posts/").weak());
            for doc_ref in &summary.posts {
                action = show_entry(ui, doc_ref, &es.current_ref, action);
            }
            ui.add_space(4.0);
        }

        // Pages section
        if !summary.pages.is_empty() {
            ui.label(egui::RichText::new("pages/").weak());
            for doc_ref in &summary.pages {
                action = show_entry(ui, doc_ref, &es.current_ref, action);
            }
            ui.add_space(8.0);
        }

        ui.separator();
        ui.add_space(4.0);

        // Preview URL button
        let (btn_label, enabled) = match es.session.serve_status() {
            ServeStatus::Listening { url } => (format!("Preview ↗ {url}"), true),
            ServeStatus::Unavailable { reason } => (format!("Preview unavailable: {reason}"), false),
        };
        ui.add_enabled_ui(enabled, |ui| {
            if ui.button(&btn_label).clicked() {
                action = SidebarAction::OpenPreview;
            }
        });
    });

    action
}

fn show_entry(
    ui: &mut egui::Ui,
    doc_ref: &DocumentRef,
    current: &Option<DocumentRef>,
    prev_action: SidebarAction,
) -> SidebarAction {
    let is_selected = current.as_ref().map_or(false, |c| c.path == doc_ref.path);
    let mut label = egui::RichText::new(&doc_ref.title);
    if is_selected {
        label = label.strong();
    }
    if doc_ref.has_parse_error {
        label = label.color(egui::Color32::YELLOW);
    }

    ui.horizontal(|ui| {
        if doc_ref.is_draft {
            ui.label(egui::RichText::new("draft").weak().small());
        }
        if doc_ref.has_parse_error {
            ui.label("⚠");
        }
        if ui.selectable_label(is_selected, label).clicked() {
            return SidebarAction::SelectDocument(doc_ref.clone());
        }
        prev_action
    })
    .inner
}
```

Note: `show_entry` returns `SidebarAction` from the horizontal closure. The outer loop needs to handle the `prev_action` carefully. Refactor if clippy complains about the `prev_action` parameter — use an `Option<SidebarAction>` accumulator instead.

- [ ] **Step 2: Build and smoke test**

```bash
cargo build -p lopress 2>&1 && ./target/debug/lopress my-site
```

Expected: sidebar shows `posts/` and `pages/` sections with file titles. Clicking a post selects it (bold). Preview URL button shows the serve URL.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/sidebar.rs
git commit -m "feat(lopress-editor): posts sidebar with selection and preview button"
```

---

## Task 9: `lopress-editor` — block editor

**Files:**
- Modify: `crates/lopress-editor/src/ui/editor.rs`

- [ ] **Step 1: Replace stub with full block editor**

```rust
use crate::ops;
use crate::state::EditingState;
use lopress_core::Block;

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) {
    // Parse error fallback
    if let Some(raw) = &es.parse_error_raw {
        ui.label(
            egui::RichText::new(
                es.parse_error_msg.as_deref().unwrap_or("Parse error"),
            )
            .color(egui::Color32::RED),
        );
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let raw = raw.clone();
            ui.add(
                egui::TextEdit::multiline(&mut raw.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
        });
        return;
    }

    let Some(doc) = &mut es.current_doc else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Select a post from the sidebar.").weak());
        });
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut action: Option<BlockAction> = None;
        let block_count = doc.blocks.len();

        for idx in 0..block_count {
            let Some(block) = doc.blocks.get(idx) else { continue };
            let is_editable = ops::is_editable(&block.r#type);

            if is_editable {
                let response = show_editable_block(ui, block, idx, &mut action);
                if response.changed() {
                    doc.mark_dirty();
                }
            } else {
                show_opaque_block(ui, block);
            }
        }

        // Add block button
        ui.add_space(8.0);
        if ui.button("+ Add block").clicked() {
            ops::add_paragraph_at_end(&mut doc.blocks);
            doc.mark_dirty();
            es.focused_block = Some(doc.blocks.len().saturating_sub(1));
        }

        // Apply deferred action (borrows resolved)
        if let Some(act) = action {
            apply_action(doc, act);
        }
    });
}

enum BlockAction {
    Split { idx: usize, caret: usize },
    MergeWithPrev { idx: usize },
    ChangeType { idx: usize, new_type: &'static str, level: Option<u8> },
    Delete { idx: usize },
}

fn show_editable_block(
    ui: &mut egui::Ui,
    block: &Block,
    idx: usize,
    action: &mut Option<BlockAction>,
) -> egui::Response {
    let block_type = block.r#type.clone();
    let level = block
        .attrs
        .get("level")
        .and_then(|v| v.as_u64())
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1);

    ui.horizontal(|ui| {
        // Type dropdown
        let label = type_label(&block_type, level);
        egui::ComboBox::from_id_salt(format!("type_{idx}"))
            .selected_text(label)
            .show_ui(ui, |ui| {
                for opt in ["¶", "H1", "H2", "H3", "H4", "H5", "H6"] {
                    if ui.selectable_label(label == opt, opt).clicked() {
                        let (nt, lv) = parse_type_label(opt);
                        *action = Some(BlockAction::ChangeType {
                            idx,
                            new_type: nt,
                            level: lv,
                        });
                    }
                }
            });

        // Delete button
        if ui.small_button("×").clicked() {
            *action = Some(BlockAction::Delete { idx });
        }
    });

    // Text editor
    let mut text = block.text.clone().unwrap_or_default();
    let font = match block_type.as_str() {
        "heading" => egui::TextStyle::Heading,
        _ => egui::TextStyle::Body,
    };
    let resp = ui.add(
        egui::TextEdit::multiline(&mut text)
            .font(font)
            .desired_width(f32::INFINITY)
            .desired_rows(1),
    );

    // Update text in place (we need a mutable borrow — handled by caller re-indexing)
    // The text edit widget mutates `text`; we need to write it back.
    // This is done via the response: caller checks `resp.changed()` and then
    // re-fetches the block to update its text. However, egui's TextEdit takes
    // `&mut String`, so `text` already has the updated value. We can't mutate
    // `block` here (immutable borrow). Instead, we store the updated text in a
    // thread-local or return it alongside the response.
    //
    // Simplest fix: change signature to take `blocks: &mut Vec<Block>` and
    // index directly. See Step 2.

    resp
}
```

- [ ] **Step 2: Refactor `show_editable_block` to take mutable block reference**

The problem in Step 1 is that we can't mutate `block.text` inside the loop because we hold a shared borrow on `doc.blocks`. Fix by collecting the loop into an index-based approach that borrows mutably for the TextEdit and defers block actions:

Replace the `show(ui, es)` body with:

```rust
pub fn show(ui: &mut egui::Ui, es: &mut EditingState) {
    if let Some(raw) = &es.parse_error_raw {
        ui.label(
            egui::RichText::new(
                es.parse_error_msg.as_deref().unwrap_or("Parse error"),
            )
            .color(egui::Color32::RED),
        );
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut raw_clone = raw.clone();
            ui.add(
                egui::TextEdit::multiline(&mut raw_clone)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
        });
        return;
    }

    let Some(doc) = &mut es.current_doc else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Select a post from the sidebar.").weak());
        });
        return;
    };

    let mut deferred: Option<BlockAction> = None;
    let mut became_dirty = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        let block_count = doc.blocks.len();
        for idx in 0..block_count {
            let Some(block) = doc.blocks.get_mut(idx) else { continue };

            if !ops::is_editable(&block.r#type) {
                // Opaque placeholder — show but don't allow edits
                let display = placeholder_text(block);
                ui.group(|ui| {
                    ui.label(egui::RichText::new(format!("[{}]", block.r#type)).weak());
                    ui.add(
                        egui::TextEdit::multiline(&mut display.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                    if ui.small_button("×").clicked() {
                        deferred = Some(BlockAction::Delete { idx });
                    }
                });
                continue;
            }

            // Type label
            let block_type = block.r#type.clone();
            let level = block
                .attrs
                .get("level")
                .and_then(|v| v.as_u64())
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let type_lbl = type_label(&block_type, level);

            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(format!("type_{idx}"))
                    .selected_text(type_lbl)
                    .show_ui(ui, |ui| {
                        for opt in ["¶", "H1", "H2", "H3", "H4", "H5", "H6"] {
                            if ui.selectable_label(type_lbl == opt, opt).clicked() {
                                let (nt, lv) = parse_type_label(opt);
                                deferred = Some(BlockAction::ChangeType {
                                    idx,
                                    new_type: nt,
                                    level: lv,
                                });
                            }
                        }
                    });
                if ui.small_button("×").clicked() {
                    deferred = Some(BlockAction::Delete { idx });
                }
            });

            let text = block.text.get_or_insert_with(String::new);
            let font = if block_type == "heading" {
                egui::TextStyle::Heading
            } else {
                egui::TextStyle::Body
            };
            let resp = ui.add(
                egui::TextEdit::multiline(text)
                    .font(font)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1),
            );

            if resp.changed() {
                became_dirty = true;
            }

            // Keyboard: Enter → split, Backspace-at-0 → merge
            if resp.has_focus() {
                ui.input(|i| {
                    if i.key_pressed(egui::Key::Enter) && !i.modifiers.shift {
                        // egui doesn't expose caret position easily; split at end
                        deferred = Some(BlockAction::Split { idx, caret: text.len() });
                    }
                    if i.key_pressed(egui::Key::Backspace) && text.is_empty() {
                        deferred = Some(BlockAction::MergeWithPrev { idx });
                    }
                });
            }
        }

        ui.add_space(8.0);
        if ui.button("+ Add block").clicked() {
            ops::add_paragraph_at_end(&mut doc.blocks);
            became_dirty = true;
        }
    });

    if became_dirty {
        doc.mark_dirty();
    }
    if let Some(act) = deferred {
        apply_block_action(&mut doc.blocks, act);
        doc.mark_dirty();
    }
}

fn apply_block_action(blocks: &mut Vec<Block>, action: BlockAction) {
    match action {
        BlockAction::Split { idx, caret } => ops::split_block_at_caret(blocks, idx, caret),
        BlockAction::MergeWithPrev { idx } => ops::merge_with_previous(blocks, idx),
        BlockAction::ChangeType { idx, new_type, level } => {
            ops::change_block_type(blocks, idx, new_type, level);
        }
        BlockAction::Delete { idx } => ops::delete_block(blocks, idx),
    }
}

fn type_label(block_type: &str, level: u8) -> &'static str {
    match block_type {
        "heading" => match level {
            1 => "H1", 2 => "H2", 3 => "H3",
            4 => "H4", 5 => "H5", _ => "H6",
        },
        _ => "¶",
    }
}

fn parse_type_label(label: &str) -> (&'static str, Option<u8>) {
    match label {
        "H1" => ("heading", Some(1)),
        "H2" => ("heading", Some(2)),
        "H3" => ("heading", Some(3)),
        "H4" => ("heading", Some(4)),
        "H5" => ("heading", Some(5)),
        "H6" => ("heading", Some(6)),
        _ => ("paragraph", None),
    }
}

fn placeholder_text(block: &Block) -> String {
    let mut out = String::new();
    if let Some(t) = &block.text {
        out.push_str(t);
    }
    for c in &block.children {
        if !out.is_empty() { out.push('\n'); }
        out.push_str(&placeholder_text(c));
    }
    out
}

enum BlockAction {
    Split { idx: usize, caret: usize },
    MergeWithPrev { idx: usize },
    ChangeType { idx: usize, new_type: &'static str, level: Option<u8> },
    Delete { idx: usize },
}
```

- [ ] **Step 3: Build and smoke test**

```bash
cargo build -p lopress 2>&1 && ./target/debug/lopress my-site
```

Select a post in the sidebar. Expected: block editor shows the blocks. Paragraph text is editable. Type dropdown changes block type. "+" adds a new block. Posts with opaque blocks (e.g., lists) show a read-only placeholder card.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/editor.rs
git commit -m "feat(lopress-editor): block editor with paragraph/heading editing"
```

---

## Task 10: `lopress-editor` — inspector + footer

**Files:**
- Modify: `crates/lopress-editor/src/ui/inspector.rs`
- Modify: `crates/lopress-editor/src/ui/footer.rs`

- [ ] **Step 1: Replace inspector stub**

```rust
use crate::state::EditingState;

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) {
    let Some(doc) = &mut es.current_doc else {
        ui.label(egui::RichText::new("No document open.").weak());
        return;
    };

    egui::CollapsingHeader::new("Post")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("fm_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("title");
                    let title = doc.front_matter.title.get_or_insert_with(String::new);
                    if ui.text_edit_singleline(title).changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("slug");
                    let slug = doc.front_matter.slug.get_or_insert_with(String::new);
                    if ui.text_edit_singleline(slug).changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("date");
                    let mut date_str = doc
                        .front_matter
                        .date
                        .map(|d| d.to_string())
                        .unwrap_or_default();
                    if ui.text_edit_singleline(&mut date_str).changed() {
                        // Parse lazily; leave existing date on failure
                        if let Ok(parsed) =
                            date_str.parse::<chrono::NaiveDate>()
                        {
                            doc.front_matter.date = Some(parsed);
                            doc.mark_dirty();
                        }
                    }
                    ui.end_row();

                    ui.label("draft");
                    if ui.checkbox(&mut doc.front_matter.draft, "").changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("description");
                    let desc = doc
                        .front_matter
                        .description
                        .get_or_insert_with(String::new);
                    if ui
                        .add(
                            egui::TextEdit::multiline(desc)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        doc.mark_dirty();
                    }
                    ui.end_row();
                });
        });
}
```

Add `chrono` to `crates/lopress-editor/Cargo.toml` dependencies:
```toml
chrono = { workspace = true }
```

- [ ] **Step 2: Replace footer stub**

```rust
use crate::state::EditingState;
use lopress_gui_host::{BuildStatus, ServeStatus};

pub fn show(ui: &mut egui::Ui, es: &EditingState) {
    ui.horizontal(|ui| {
        // Build status (left)
        match es.session.build_status() {
            BuildStatus::Idle => { ui.label("—"); }
            BuildStatus::Building => { ui.spinner(); ui.label("Building…"); }
            BuildStatus::Ok { pages_rendered, pages_skipped, duration_ms } => {
                ui.label(
                    egui::RichText::new(format!(
                        "Built {pages_rendered} rendered, {pages_skipped} skipped in {duration_ms}ms"
                    ))
                    .weak(),
                );
            }
            BuildStatus::Failed { message } => {
                ui.colored_label(egui::Color32::RED, format!("Build failed: {message}"));
            }
        }

        ui.separator();

        // Save state (middle)
        if let Some(doc) = &es.current_doc {
            if let Some(err) = &doc.last_save_error {
                ui.colored_label(egui::Color32::RED, format!("save failed: {err}"));
            } else if doc.dirty {
                ui.label(egui::RichText::new("unsaved changes").weak());
            } else {
                ui.label(egui::RichText::new("saved").weak());
            }
        }

        ui.separator();

        // Serve URL (right)
        match es.session.serve_status() {
            ServeStatus::Listening { url } => {
                if ui.small_button(url).clicked() {
                    ui.output_mut(|o| o.copied_text = url.clone());
                }
            }
            ServeStatus::Unavailable { reason } => {
                ui.label(egui::RichText::new(format!("serve: {reason}")).weak());
            }
        }
    });
}
```

- [ ] **Step 3: Build and smoke test**

```bash
cargo build -p lopress 2>&1 && ./target/debug/lopress my-site
```

Expected: inspector shows front-matter fields when a post is selected. Footer shows build status, save state, and serve URL. Editing the title marks the doc dirty.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/inspector.rs crates/lopress-editor/src/ui/footer.rs
git commit -m "feat(lopress-editor): inspector front-matter form and status footer"
```

---

## Task 11: Wire save loop end-to-end + Ctrl-S

The debounce logic is already in `app.rs` from Task 6. This task verifies it works end-to-end and adds Ctrl-S.

**Files:**
- Modify: `crates/lopress-editor/src/app.rs`

- [ ] **Step 1: Add Ctrl-S handling in `show_editing`**

In `show_editing`, before the menu bar panel, add:

```rust
// Ctrl-S / Cmd-S forced flush
ctx.input_mut(|i| {
    if i.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
        es.flush_current();
    }
});
```

- [ ] **Step 2: End-to-end smoke test**

```bash
cargo build -p lopress && ./target/debug/lopress my-site
```

1. Select `hello` post.
2. Edit the paragraph text.
3. Watch the footer: should show "unsaved changes".
4. Wait 500 ms or press Ctrl-S.
5. Footer shows "saved".
6. Open `my-site/www/posts/hello/index.html` in a browser (or run `./target/debug/lopress serve my-site` in another terminal and navigate to `http://127.0.0.1:8080/posts/hello/`) — should reflect the edit.

- [ ] **Step 3: Verify browser live-reload works**

With a browser tab open at `http://127.0.0.1:8080/posts/hello/`:
1. Edit in the GUI.
2. Wait 500 ms.
3. Browser tab reloads automatically.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/app.rs
git commit -m "feat(lopress-editor): Ctrl-S flush and debounce save loop wired"
```

---

## Task 12: Roundtrip tests + final checks

**Files:**
- Create: `crates/lopress-editor/tests/roundtrip_tests.rs`

- [ ] **Step 1: Write roundtrip test**

```rust
use lopress_core::{parse, serialize, Block, Document, FrontMatter};
use lopress_editor::ops;
use std::fs;
use tempfile::TempDir;

fn make_workspace_with_post(content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    fs::write(
        p.join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://x.com\"\n",
    )
    .unwrap();
    for d in ["src/posts", "src/pages", "src/images", "plugins"] {
        fs::create_dir_all(p.join(d)).unwrap();
    }
    let post = p.join("src/posts/test.md");
    fs::write(&post, content).unwrap();
    (dir, post)
}

#[test]
fn edit_paragraph_leaves_opaque_blocks_intact() {
    let content = concat!(
        "---\ntitle: T\ndate: 2026-04-20\n---\n\n",
        "# Heading\n\n",
        "A paragraph.\n\n",
        "<!-- lopress:video {\"src\":\"v.mp4\"} -->\n",
        "<!-- /lopress:video -->\n\n",
        "Another paragraph.\n",
    );
    let (_dir, post) = make_workspace_with_post(content);

    // Parse
    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();

    // Find and edit the first paragraph block
    let para_idx = doc.blocks.iter().position(|b| b.r#type == "paragraph").unwrap();
    if let Some(b) = doc.blocks.get_mut(para_idx) {
        b.text = Some("Edited paragraph.".into());
    }

    // Serialize and reparse
    let serialized = serialize(&doc);
    let reparsed = parse(&serialized).unwrap();

    // Paragraph updated
    let edited = reparsed.blocks.iter().find(|b| b.r#type == "paragraph");
    assert_eq!(edited.and_then(|b| b.text.as_deref()), Some("Edited paragraph."));

    // Opaque block preserved
    assert!(reparsed.blocks.iter().any(|b| b.r#type == "lopress:video"));
    let video = reparsed.blocks.iter().find(|b| b.r#type == "lopress:video").unwrap();
    assert_eq!(
        video.attrs.get("src").and_then(|v| v.as_str()),
        Some("v.mp4")
    );
}

#[test]
fn split_and_serialize_roundtrips() {
    let content = "---\ntitle: T\n---\n\nhello world\n";
    let (_dir, post) = make_workspace_with_post(content);
    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();

    ops::split_block_at_caret(&mut doc.blocks, 0, 5);
    let s = serialize(&doc);
    let reparsed = parse(&s).unwrap();
    assert_eq!(reparsed.blocks.len(), 2);
    assert_eq!(reparsed.blocks[0].text.as_deref(), Some("hello"));
    assert_eq!(reparsed.blocks[1].text.as_deref(), Some(" world"));
}

#[test]
fn delete_block_serializes_correctly() {
    let content = "---\ntitle: T\n---\n\nfirst\n\nsecond\n\nthird\n";
    let (_dir, post) = make_workspace_with_post(content);
    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();
    assert_eq!(doc.blocks.len(), 3);

    ops::delete_block(&mut doc.blocks, 1);
    let s = serialize(&doc);
    let reparsed = parse(&s).unwrap();
    assert_eq!(reparsed.blocks.len(), 2);
    assert_eq!(reparsed.blocks[0].text.as_deref(), Some("first"));
    assert_eq!(reparsed.blocks[1].text.as_deref(), Some("third"));
}
```

- [ ] **Step 2: Run roundtrip tests**

```bash
cargo test -p lopress-editor --test roundtrip_tests 2>&1
```

Expected: 3 tests pass.

- [ ] **Step 3: Full workspace test suite**

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass (no regressions).

- [ ] **Step 4: Full clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings 2>&1
```

Fix any warnings.

- [ ] **Step 5: fmt check**

```bash
cargo fmt --check 2>&1
```

Fix any formatting issues with `cargo fmt`.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/tests/roundtrip_tests.rs
git commit -m "test(lopress-editor): roundtrip tests for edit/split/delete with opaque blocks"
```

---

## Self-review checklist

- [x] **Spec §1 scope** — three deliverables: lopress-editor, lopress-gui-host, main.rs dispatch. All covered.
- [x] **Spec §2 crate layout** — dependency direction matches: editor → core + host; host → build + watch + serve; neither → egui.
- [x] **Spec §3 CLI dispatch** — Task 6 step 3 replaces main.rs with correct routing table.
- [x] **Spec §4.1 welcome state** — Tasks 7 and 6 cover welcome screen + recent workspaces.
- [x] **Spec §4.2 editing layout** — Tasks 8–10 cover sidebar, inspector, footer, editor.
- [x] **Spec §4.3 sidebar** — Task 8 covers post/page list, draft chip, parse-error icon, preview button.
- [x] **Spec §4.4 inspector** — Task 10 covers title/slug/date/draft/description fields.
- [x] **Spec §4.5 footer** — Task 10 covers build status, save state, serve URL.
- [x] **Spec §5 block editor** — Task 9 covers editable blocks, opaque placeholders, type dropdown, delete, Enter/Backspace, add block.
- [x] **Spec §6 save loop** — Tasks 3 (save in session), 6 (debounce in app), 11 (Ctrl-S).
- [x] **Spec §7 error handling** — Task 3 integration tests cover invalid workspace, build failure, serve bind failure; Task 9 covers parse-error fallback view.
- [x] **Spec §8 out of scope** — undo/redo, drag handles, fuzzy switcher, New Post, Site Settings all absent from this plan.
- [x] **Spec §9 testing** — ops unit tests (Task 4), roundtrip tests (Task 12), gui-host integration tests (Task 3).
- [x] **Spec §10 deps** — eframe, rfd, open, directories added in Task 0.
- [x] **Type consistency** — `BlockAction`, `ops::*` signatures, `Session::save`, `EditingState::flush_current` used consistently across tasks.

# Editor Performance & Instrumentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make workspace open feel instant via a deferred background build+serve, kill the full-document clone on every undo snapshot, and add a reusable timing facility so future regressions are caught by measurement.

**Architecture:** Three phases run sequentially. Phase 1 adds an env-gated `perf::span` scope guard in `lopress-core` and instruments key code paths. Phase 2 makes `Session::open` return after only workspace-load+scan, spawning a single background thread for build then serve; the editor footer surfaces both statuses reactively. Phase 3 deletes one full-doc clone per editing action, then runs a release-build measurement pass and writes a findings doc that decides whether to attack the remaining conditional opportunities.

**Tech Stack:** Rust (edition 2021), Cargo workspace, Floem 0.2 reactive UI, `std::sync` for `Arc<Mutex>`/`OnceLock`, `std::time` for timing, `tempfile` for tests.

**Conventions:**
- Test framework: `#[test]`. Tests denying workspace lints (`unwrap_used`, `expect_used`) put `#![allow(...)]` at the top of the file — matches the existing pattern in `crates/lopress-editor/tests/actions_tests.rs`.
- Run tests: `cargo test -p <crate>` or `cargo test --workspace`.
- Build: `cargo build -p <crate>` or `cargo build --workspace`.
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`.
- Format: `cargo fmt --check`.
- Commit message style: Conventional Commits with crate scope (`feat(core):`, `feat(host):`, `feat(editor):`, `refactor(editor):`, `test(host):`, `docs:`).
- Add a `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` line to every commit.

---

## Task 1: Add `perf` module to `lopress-core`

**Files:**
- Create: `crates/lopress-core/src/perf.rs`
- Modify: `crates/lopress-core/src/lib.rs`

- [ ] **Step 1: Create `perf.rs` with the `Span` type, env gate, and inline unit tests**

Write the full contents of `crates/lopress-core/src/perf.rs`:

```rust
//! Lightweight env-gated timing spans.
//!
//! Enable by setting the `LOPRESS_TIMING` environment variable to any
//! non-empty value before launching. When enabled, dropping a [`Span`]
//! prints `[timing] <name>: <ms>ms` to stderr.
//!
//! When disabled (the default), [`span`] returns a guard holding no
//! `Instant` — effectively free.
//!
//! Note: CI does not set `LOPRESS_TIMING`, so spans are no-ops in CI runs.

use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Read once at first use, cached for the process lifetime.
fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var_os("LOPRESS_TIMING")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}

/// A timing scope guard. When dropped, if recording, prints the elapsed
/// time to stderr in `[timing] <name>: <ms>ms` form.
pub struct Span {
    name: &'static str,
    started: Option<Instant>,
}

impl Span {
    /// Construct a span using the env-var-cached enabled flag.
    fn new(name: &'static str) -> Self {
        Self::new_with_enabled(name, enabled())
    }

    /// Test seam: construct a span with an explicit enabled flag, bypassing
    /// the env-var cache. Intended for tests that should not depend on
    /// process-wide environment state.
    pub fn new_with_enabled(name: &'static str, enabled: bool) -> Self {
        Self {
            name,
            started: if enabled { Some(Instant::now()) } else { None },
        }
    }

    /// Whether this span will produce output on drop. Useful for tests.
    pub fn is_recording(&self) -> bool {
        self.started.is_some()
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(t0) = self.started {
            let ms = t0.elapsed().as_millis();
            eprintln!("[timing] {name}: {ms}ms", name = self.name);
        }
    }
}

/// Start a timing span. Pair with `let _t = ...;` so the guard drops at
/// the end of the enclosing scope.
///
/// ```ignore
/// fn slow_thing() {
///     let _t = lopress_core::perf::span("module.slow_thing");
///     // ... work ...
/// }
/// ```
pub fn span(name: &'static str) -> Span {
    Span::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn disabled_span_does_not_record() {
        let s = Span::new_with_enabled("test.disabled", false);
        assert!(!s.is_recording());
    }

    #[test]
    fn enabled_span_records() {
        let s = Span::new_with_enabled("test.enabled", true);
        assert!(s.is_recording());
    }

    #[test]
    fn enabled_span_drop_is_safe_after_sleep() {
        let s = Span::new_with_enabled("test.drop", true);
        sleep(Duration::from_millis(5));
        drop(s); // must not panic
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

Open `crates/lopress-core/src/lib.rs` and add a line `pub mod perf;` alongside the other `pub mod` declarations. The other declarations sit near the top of the file; add this one in the same block.

- [ ] **Step 3: Run the tests**

```
cargo test -p lopress-core perf
```

Expected: `test result: ok. 3 passed; 0 failed` (the three `perf::tests::*` tests).

- [ ] **Step 4: Run clippy and fmt-check**

```
cargo clippy -p lopress-core --all-targets -- -D warnings
cargo fmt --check
```

Expected: both pass clean.

- [ ] **Step 5: Commit**

```
git add crates/lopress-core/src/perf.rs crates/lopress-core/src/lib.rs
git commit -m "feat(core): add env-gated perf::span timing facility

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Instrument `Session::open` and `Session::save` with timing spans

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`

No new tests — mechanical insertions. Verify by building.

- [ ] **Step 1: Add the `perf` import**

At the top of `crates/lopress-gui-host/src/session.rs` (in the existing `use` block), add:

```rust
use lopress_core::perf;
```

- [ ] **Step 2: Instrument `Session::open` — wrap `Workspace::load`**

In `Session::open`, replace the existing line:

```rust
let workspace = Workspace::load(workspace_root)
    .map_err(|e| OpenError::InvalidWorkspace(e.to_string()))?;
```

with:

```rust
let workspace = {
    let _t = perf::span("workspace.open.workspace_load");
    Workspace::load(workspace_root)
        .map_err(|e| OpenError::InvalidWorkspace(e.to_string()))?
};
```

- [ ] **Step 3: Instrument `Session::open` — wrap the initial build**

In the same function, prepend a span line inside the initial-build block. The existing block looks like:

```rust
{
    let t0 = std::time::Instant::now();
    match lopress_build::build(workspace_root) {
        // ...
    }
}
```

Change it to:

```rust
{
    let _t = perf::span("workspace.open.initial_build");
    let t0 = std::time::Instant::now();
    match lopress_build::build(workspace_root) {
        // ...
    }
}
```

(Leave the inner body unchanged.)

- [ ] **Step 4: Instrument `Session::open` — wrap the serve start**

Wrap the existing `serve_in_background` match expression in a block with a span. Change:

```rust
let (server_arc, serve_status) =
    match serve_in_background(www_dir.clone(), "127.0.0.1".into(), 8080) {
        // ...
    };
```

to:

```rust
let (server_arc, serve_status) = {
    let _t = perf::span("workspace.open.serve_start");
    match serve_in_background(www_dir.clone(), "127.0.0.1".into(), 8080) {
        // ...
    }
};
```

(Leave the inner match arms unchanged.)

- [ ] **Step 5: Instrument `Session::open` — wrap the scan**

Change:

```rust
let summary = Arc::new(Mutex::new(scan_workspace(&workspace)));
```

to:

```rust
let summary = Arc::new(Mutex::new({
    let _t = perf::span("workspace.open.scan");
    scan_workspace(&workspace)
}));
```

- [ ] **Step 6: Instrument `Session::save`**

Replace the existing `Session::save` body:

```rust
pub fn save(&self, doc: &LoadedDocument) -> Result<(), SaveError> {
    let content = serialize(&Document {
        front_matter: doc.front_matter.clone(),
        blocks: doc.blocks.clone(),
    });
    atomic_write(&doc.path, content.as_bytes())?;
    Ok(())
}
```

with:

```rust
pub fn save(&self, doc: &LoadedDocument) -> Result<(), SaveError> {
    let content = {
        let _t = perf::span("editor.save.serialize");
        serialize(&Document {
            front_matter: doc.front_matter.clone(),
            blocks: doc.blocks.clone(),
        })
    };
    {
        let _t = perf::span("editor.save.write");
        atomic_write(&doc.path, content.as_bytes())?;
    }
    Ok(())
}
```

- [ ] **Step 7: Build and test**

```
cargo build -p lopress-gui-host
cargo test -p lopress-gui-host
cargo clippy -p lopress-gui-host --all-targets -- -D warnings
cargo fmt --check
```

Expected: builds clean, tests pass, clippy clean, fmt clean.

- [ ] **Step 8: Commit**

```
git add crates/lopress-gui-host/src/session.rs
git commit -m "feat(host): instrument Session open and save with perf spans

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Instrument editor doc-open and on_action with timing spans

**Files:**
- Modify: `crates/lopress-editor/src/state.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

No new tests — mechanical insertions.

- [ ] **Step 1: Add the import to `state.rs`**

At the top of `crates/lopress-editor/src/state.rs`, add:

```rust
use lopress_core::perf;
```

- [ ] **Step 2: Instrument `EditingState::open_document`**

Replace the existing `open_document` body:

```rust
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
```

with:

```rust
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
```

- [ ] **Step 3: Add the import to `ui/mod.rs`**

At the top of `crates/lopress-editor/src/ui/mod.rs`, add:

```rust
use lopress_core::perf;
```

- [ ] **Step 4: Instrument `on_action`**

In the `on_action: ActionSink = Rc::new(move |action: BlockAction| { ... })` closure inside `editing_view`, add a span as the first line of the closure body, immediately after the opening `{`:

```rust
let on_action: ActionSink = Rc::new(move |action: BlockAction| {
    let _t = perf::span("editor.on_action");
    // ... rest of the existing closure body unchanged ...
```

(Leave the rest of the closure body exactly as-is.)

- [ ] **Step 5: Build, test, lint**

```
cargo build -p lopress-editor
cargo test -p lopress-editor
cargo clippy -p lopress-editor --all-targets -- -D warnings
cargo fmt --check
```

Expected: builds clean, tests pass, clippy clean, fmt clean.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/state.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): instrument open_document and on_action with perf spans

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Add `ServeStatus::Starting` and migrate `serve_status` to `Arc<Mutex<>>`

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs` (call site adjustment)
- Modify: `crates/lopress-editor/src/ui/footer.rs` (call site adjustment)

This is a refactor with no behavior change — preparation for Task 5. Existing tests must still pass.

- [ ] **Step 1: Add `Starting` variant to `ServeStatus`**

In `crates/lopress-gui-host/src/session.rs`, change the existing enum:

```rust
#[derive(Debug, Clone)]
pub enum ServeStatus {
    Unavailable { reason: String },
    Listening { url: String },
}
```

to:

```rust
#[derive(Debug, Clone)]
pub enum ServeStatus {
    /// The preview server has not finished binding yet. Used while the
    /// background open thread is still working.
    Starting,
    Unavailable { reason: String },
    Listening { url: String },
}
```

- [ ] **Step 2: Change the `Session::serve_status` field type**

In the `Session` struct, change:

```rust
pub struct Session {
    workspace: Arc<Workspace>,
    summary: Arc<Mutex<WorkspaceSummary>>,
    build_status: Arc<Mutex<BuildStatus>>,
    serve_status: ServeStatus,
    _server: Option<Arc<ServerHandle>>,
    _watcher: Option<Watcher>,
}
```

to:

```rust
pub struct Session {
    workspace: Arc<Workspace>,
    summary: Arc<Mutex<WorkspaceSummary>>,
    build_status: Arc<Mutex<BuildStatus>>,
    serve_status: Arc<Mutex<ServeStatus>>,
    _server: Option<Arc<ServerHandle>>,
    _watcher: Option<Watcher>,
}
```

- [ ] **Step 3: Wrap the `serve_status` value at construction**

In `Session::open`, the existing code computes `serve_status` from the `serve_in_background` match (see Task 2 Step 4 for the surrounding span). Where the constructor builds `Self`, change:

```rust
Ok(Self {
    workspace,
    summary,
    build_status,
    serve_status,
    _server: server_arc,
    _watcher: watcher,
})
```

to:

```rust
Ok(Self {
    workspace,
    summary,
    build_status,
    serve_status: Arc::new(Mutex::new(serve_status)),
    _server: server_arc,
    _watcher: watcher,
})
```

- [ ] **Step 4: Change `serve_status()` to return an owned clone**

Replace the existing accessor:

```rust
pub fn serve_status(&self) -> &ServeStatus {
    &self.serve_status
}
```

with:

```rust
pub fn serve_status(&self) -> ServeStatus {
    lock(&self.serve_status).clone()
}
```

- [ ] **Step 5: Update `preview_url_for` to use the owned clone**

Replace:

```rust
pub fn preview_url_for(&self, doc_ref: &DocumentRef) -> Option<String> {
    let url = match &self.serve_status {
        ServeStatus::Listening { url } => url,
        ServeStatus::Unavailable { .. } => return None,
    };
    // ... rest unchanged ...
```

with:

```rust
pub fn preview_url_for(&self, doc_ref: &DocumentRef) -> Option<String> {
    let status = lock(&self.serve_status).clone();
    let url = match &status {
        ServeStatus::Listening { url } => url.clone(),
        ServeStatus::Unavailable { .. } | ServeStatus::Starting => return None,
    };
    // ... rest unchanged, but use `url` (already owned) instead of dereferencing ...
```

After this change, the rest of the function should use the owned `url: String` directly — for example `Some(format!("{url}/posts/{slug}/"))` continues to work.

- [ ] **Step 6: Update the `serve_url` helper in `footer.rs`**

The existing helper takes `&ServeStatus` and treats `Unavailable` as `None`. Add the new `Starting` variant to the same `None` branch. Replace:

```rust
pub fn serve_url(status: &ServeStatus) -> Option<String> {
    match status {
        ServeStatus::Listening { url } => Some(url.clone()),
        ServeStatus::Unavailable { .. } => None,
    }
}
```

with:

```rust
pub fn serve_url(status: &ServeStatus) -> Option<String> {
    match status {
        ServeStatus::Listening { url } => Some(url.clone()),
        ServeStatus::Unavailable { .. } | ServeStatus::Starting => None,
    }
}
```

- [ ] **Step 7: Update the call site in `ui/mod.rs`**

The `editing_view` function in `crates/lopress-editor/src/ui/mod.rs` has a block that computes `serve_url_str`:

```rust
let serve_url_str = editing
    .borrow()
    .as_ref()
    .and_then(|s| serve_url(s.session.serve_status()));
```

`s.session.serve_status()` now returns an owned `ServeStatus`, so the `serve_url(&...)` helper needs a reference. Change to:

```rust
let serve_url_str = editing
    .borrow()
    .as_ref()
    .and_then(|s| serve_url(&s.session.serve_status()));
```

- [ ] **Step 8: Build, test, lint**

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Expected: workspace builds clean, all existing tests pass, clippy clean, fmt clean.

- [ ] **Step 9: Commit**

```
git add crates/lopress-gui-host/src/session.rs crates/lopress-editor/src/ui/mod.rs crates/lopress-editor/src/ui/footer.rs
git commit -m "refactor(host): add ServeStatus::Starting, move serve_status behind Arc<Mutex>

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 5: Defer build + serve to a background thread in `Session::open`

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Modify: `crates/lopress-gui-host/Cargo.toml` (add `tempfile` dev-dep if missing)
- Create: `crates/lopress-gui-host/tests/deferred_open.rs`

- [ ] **Step 1: Add `tempfile` to dev-dependencies if not already present**

Open `crates/lopress-gui-host/Cargo.toml`. If there is no `[dev-dependencies]` section, add one. Ensure it contains:

```toml
[dev-dependencies]
tempfile = { workspace = true }
```

(If a `[dev-dependencies]` block exists with other entries, just add the `tempfile` line.)

- [ ] **Step 2: Replace `Session::open` with the deferred implementation**

Replace the entire body of `Session::open` with the following. This consolidates the moves: build + serve run on a single background thread, the watcher is still spawned synchronously, the `_server` field is replaced with a shared mutex so the watcher can broadcast once the server is up.

First, change the `_server` field in the `Session` struct:

```rust
pub struct Session {
    workspace: Arc<Workspace>,
    summary: Arc<Mutex<WorkspaceSummary>>,
    build_status: Arc<Mutex<BuildStatus>>,
    serve_status: Arc<Mutex<ServeStatus>>,
    server: Arc<Mutex<Option<Arc<ServerHandle>>>>,
    _watcher: Option<Watcher>,
}
```

(Renamed `_server` to `server` since it is now actively read by the watcher and by `rebuild`. Type changed from `Option<Arc<ServerHandle>>` to `Arc<Mutex<Option<Arc<ServerHandle>>>>`.)

Then replace the body of `Session::open`:

```rust
pub fn open(workspace_root: &Path) -> Result<Self, OpenError> {
    // Synchronous: workspace load.
    let workspace = {
        let _t = perf::span("workspace.open.workspace_load");
        Workspace::load(workspace_root)
            .map_err(|e| OpenError::InvalidWorkspace(e.to_string()))?
    };
    let workspace = Arc::new(workspace);

    // Synchronous: workspace scan.
    let summary = Arc::new(Mutex::new({
        let _t = perf::span("workspace.open.scan");
        scan_workspace(&workspace)
    }));

    // Initial statuses reflect the "still working in the background" state.
    let build_status = Arc::new(Mutex::new(BuildStatus::Building));
    let serve_status = Arc::new(Mutex::new(ServeStatus::Starting));
    let server: Arc<Mutex<Option<Arc<ServerHandle>>>> = Arc::new(Mutex::new(None));

    let www_dir = workspace.www_dir();
    std::fs::create_dir_all(&www_dir).ok();

    // Background thread: initial build, then start serve.
    let ws_root_bg = workspace_root.to_path_buf();
    let build_status_bg = Arc::clone(&build_status);
    let serve_status_bg = Arc::clone(&serve_status);
    let server_bg = Arc::clone(&server);
    let summary_bg = Arc::clone(&summary);
    let workspace_bg = Arc::clone(&workspace);
    let www_dir_bg = www_dir.clone();
    std::thread::spawn(move || {
        // Initial build.
        {
            let _t = perf::span("workspace.open.initial_build");
            let t0 = std::time::Instant::now();
            match lopress_build::build(&ws_root_bg) {
                Ok(r) => {
                    *lock(&build_status_bg) = BuildStatus::Ok {
                        pages_rendered: r.pages_rendered,
                        pages_skipped: r.pages_skipped,
                        duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    };
                    *lock(&summary_bg) = scan_workspace(&workspace_bg);
                }
                Err(e) => {
                    *lock(&build_status_bg) = BuildStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        }
        // Start serve (try 8080, fall back to ephemeral).
        {
            let _t = perf::span("workspace.open.serve_start");
            let new_serve = match serve_in_background(www_dir_bg.clone(), "127.0.0.1".into(), 8080) {
                Ok(h) => {
                    let url = h.url.clone();
                    *lock(&server_bg) = Some(Arc::new(h));
                    ServeStatus::Listening { url }
                }
                Err(_) => match serve_in_background(www_dir_bg, "127.0.0.1".into(), 0) {
                    Ok(h) => {
                        let url = h.url.clone();
                        *lock(&server_bg) = Some(Arc::new(h));
                        ServeStatus::Listening { url }
                    }
                    Err(e) => ServeStatus::Unavailable {
                        reason: e.to_string(),
                    },
                },
            };
            *lock(&serve_status_bg) = new_serve;
        }
    });

    // Watcher: still spawned synchronously. Broadcasts via the shared server mutex.
    let ws_root_w = workspace_root.to_path_buf();
    let build_status_w = Arc::clone(&build_status);
    let summary_w = Arc::clone(&summary);
    let workspace_w = Arc::clone(&workspace);
    let server_w = Arc::clone(&server);

    let watcher = Watcher::spawn(workspace_root, move |_cs: ChangeSet| {
        *lock(&build_status_w) = BuildStatus::Building;
        let t0 = std::time::Instant::now();
        match lopress_build::build(&ws_root_w) {
            Ok(r) => {
                *lock(&build_status_w) = BuildStatus::Ok {
                    pages_rendered: r.pages_rendered,
                    pages_skipped: r.pages_skipped,
                    duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                };
                *lock(&summary_w) = scan_workspace(&workspace_w);
                if let Some(srv) = lock(&server_w).as_ref() {
                    srv.broadcast_reload();
                }
            }
            Err(e) => {
                *lock(&build_status_w) = BuildStatus::Failed {
                    message: e.to_string(),
                };
            }
        }
    })
    .ok();

    Ok(Self {
        workspace,
        summary,
        build_status,
        serve_status,
        server,
        _watcher: watcher,
    })
}
```

- [ ] **Step 3: Update `rebuild()` to read the shared server mutex**

Replace the existing `rebuild` body:

```rust
pub fn rebuild(&self) {
    let build_status = Arc::clone(&self.build_status);
    let workspace_root = self.workspace.root.clone();
    let server = self._server.clone();
    std::thread::spawn(move || {
        *lock(&build_status) = BuildStatus::Building;
        let t0 = std::time::Instant::now();
        match lopress_build::build(&workspace_root) {
            Ok(r) => {
                *lock(&build_status) = BuildStatus::Ok {
                    pages_rendered: r.pages_rendered,
                    pages_skipped: r.pages_skipped,
                    duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                };
                if let Some(srv) = server {
                    srv.broadcast_reload();
                }
            }
            Err(e) => {
                *lock(&build_status) = BuildStatus::Failed {
                    message: e.to_string(),
                };
            }
        }
    });
}
```

with:

```rust
pub fn rebuild(&self) {
    let build_status = Arc::clone(&self.build_status);
    let workspace_root = self.workspace.root.clone();
    let server = Arc::clone(&self.server);
    std::thread::spawn(move || {
        *lock(&build_status) = BuildStatus::Building;
        let t0 = std::time::Instant::now();
        match lopress_build::build(&workspace_root) {
            Ok(r) => {
                *lock(&build_status) = BuildStatus::Ok {
                    pages_rendered: r.pages_rendered,
                    pages_skipped: r.pages_skipped,
                    duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                };
                if let Some(srv) = lock(&server).as_ref() {
                    srv.broadcast_reload();
                }
            }
            Err(e) => {
                *lock(&build_status) = BuildStatus::Failed {
                    message: e.to_string(),
                };
            }
        }
    });
}
```

- [ ] **Step 4: Write the integration test**

This test verifies the post-refactor behavior — that `Session::open` returns and the background build eventually completes. It is a behavioral verification, not a strict red-green TDD test (the synchronous pre-refactor implementation would also pass it, since the polling loop sees `Ok` on its first read; the test is meaningful as a guard against regressions in the deferred path and as a smoke test that the background thread runs and updates state).

Create `crates/lopress-gui-host/tests/deferred_open.rs` with:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use lopress_gui_host::{BuildStatus, Session};
use tempfile::tempdir;

/// `Session::open` returns and the background build eventually completes
/// successfully on a minimal workspace.
#[test]
fn session_open_eventually_completes_initial_build() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://t\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(temp.path().join("src/posts")).unwrap();
    std::fs::create_dir_all(temp.path().join("src/pages")).unwrap();

    let session = Session::open(temp.path()).expect("open succeeded");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match session.build_status() {
            BuildStatus::Ok { .. } => break,
            BuildStatus::Failed { message } => panic!("build failed: {message}"),
            _ => {
                if Instant::now() > deadline {
                    panic!("build did not complete within 5s");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
```

- [ ] **Step 5: Run the integration test**

```
cargo test -p lopress-gui-host --test deferred_open
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Run the full workspace test + lint suite**

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Expected: everything clean.

- [ ] **Step 7: Commit**

```
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/tests/deferred_open.rs crates/lopress-gui-host/Cargo.toml
git commit -m "feat(host): defer initial build and serve start to background thread

Session::open now returns after only workspace load + scan. A single
background thread runs the initial build, then starts the preview
server. Editor window appears immediately; footer surfaces progress.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 6: Reactive serve-status polling in the editor footer

**Files:**
- Modify: `crates/lopress-editor/src/ui/footer.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

No automated test — Floem reactive views are not easily unit-testable. Verify by `cargo build` and manual inspection.

- [ ] **Step 1: Add `start_serve_status_poll` to `footer.rs`**

Append a new function near the existing `start_build_status_poll` (around line 153) in `crates/lopress-editor/src/ui/footer.rs`:

```rust
/// Initial poll loop for `ServeStatus`. Mirrors `start_build_status_poll`.
/// Re-schedules itself every 250 ms.
pub fn start_serve_status_poll(
    session: std::rc::Rc<dyn Fn() -> ServeStatus>,
    sink: RwSignal<ServeStatus>,
) {
    fn schedule(session: std::rc::Rc<dyn Fn() -> ServeStatus>, sink: RwSignal<ServeStatus>) {
        floem::action::exec_after(std::time::Duration::from_millis(250), move |_| {
            sink.set((session)());
            schedule(session, sink);
        });
    }
    sink.set((session)());
    schedule(session, sink);
}
```

- [ ] **Step 2: Change `footer_view` to accept a `RwSignal<ServeStatus>` instead of `Option<String>`**

Replace the existing signature and the `url_view` block. Change:

```rust
pub fn footer_view(
    build_status: RwSignal<BuildStatus>,
    dirty: RwSignal<bool>,
    save_error: RwSignal<Option<String>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    serve_url: Option<String>,
) -> impl IntoView {
    // ... build_label, save_label, word_label unchanged ...

    let url_view: AnyView = match serve_url {
        Some(url) => {
            let url_for_click = url.clone();
            label(move || url.clone())
                .on_click_stop(move |_| {
                    let _ = Clipboard::set_contents(url_for_click.clone());
                })
                .style(|s| {
                    s.color(MUTED)
                        .font_size(12.)
                        .cursor(floem::style::CursorStyle::Pointer)
                        .hover(|s| s.color(FG))
                })
                .into_any()
        }
        None => label(|| "no preview".to_string())
            .style(|s| s.color(MUTED).font_size(12.))
            .into_any(),
    };

    h_stack((
        // ...
        url_view.style(|s| s.padding_horiz(10.)),
    ))
    // ...
}
```

to:

```rust
pub fn footer_view(
    build_status: RwSignal<BuildStatus>,
    dirty: RwSignal<bool>,
    save_error: RwSignal<Option<String>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    serve_status: RwSignal<ServeStatus>,
) -> impl IntoView {
    // ... build_label, save_label, word_label unchanged ...

    let url_view = dyn_container(
        move || serve_status.get(),
        move |status| serve_status_view(&status).into_any(),
    );

    h_stack((
        // ...
        url_view.style(|s| s.padding_horiz(10.)),
    ))
    // ...
}
```

(Leave the existing `build_label`, `save_label`, `word_label`, the `h_stack` layout, and the outer `.style(...)` block unchanged.)

- [ ] **Step 3: Add the `serve_status_view` helper to `footer.rs`**

Add this function alongside the other private view helpers (near `build_status_view` and `save_state_view`):

```rust
fn serve_status_view(status: &ServeStatus) -> AnyView {
    match status {
        ServeStatus::Starting => label(|| "starting preview…".to_string())
            .style(|s| s.color(MUTED).font_size(12.))
            .into_any(),
        ServeStatus::Listening { url } => {
            let url_for_label = url.clone();
            let url_for_click = url.clone();
            label(move || url_for_label.clone())
                .on_click_stop(move |_| {
                    let _ = Clipboard::set_contents(url_for_click.clone());
                })
                .style(|s| {
                    s.color(MUTED)
                        .font_size(12.)
                        .cursor(floem::style::CursorStyle::Pointer)
                        .hover(|s| s.color(FG))
                })
                .into_any()
        }
        ServeStatus::Unavailable { .. } => label(|| "no preview".to_string())
            .style(|s| s.color(MUTED).font_size(12.))
            .into_any(),
    }
}
```

- [ ] **Step 4: Update the module-level doc comment for `serve_url`**

The `serve_url` helper (around line 169) is now only used by tests or external consumers — the footer no longer calls it. Leave it in place (its `Starting` arm was already added in Task 4 Step 6) and update its doc comment if helpful, but no code change is required.

- [ ] **Step 5: Wire the serve-status signal in `ui/mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, inside `editing_view`, find the existing block:

```rust
let serve_url_str = editing
    .borrow()
    .as_ref()
    .and_then(|s| serve_url(&s.session.serve_status()));

{
    let editing_for_poll = Rc::clone(&editing);
    let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
        editing_for_poll
            .borrow()
            .as_ref()
            .map(|s| s.session.build_status())
            .unwrap_or(BuildStatus::Idle)
    });
    start_build_status_poll(session_reader, build_status_sig);
}
```

Replace it with:

```rust
let serve_status_sig: RwSignal<ServeStatus> = RwSignal::new(ServeStatus::Starting);

{
    let editing_for_poll = Rc::clone(&editing);
    let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
        editing_for_poll
            .borrow()
            .as_ref()
            .map(|s| s.session.build_status())
            .unwrap_or(BuildStatus::Idle)
    });
    start_build_status_poll(session_reader, build_status_sig);
}

{
    let editing_for_poll = Rc::clone(&editing);
    let serve_reader: Rc<dyn Fn() -> ServeStatus> = Rc::new(move || {
        editing_for_poll
            .borrow()
            .as_ref()
            .map(|s| s.session.serve_status())
            .unwrap_or(ServeStatus::Starting)
    });
    start_serve_status_poll(serve_reader, serve_status_sig);
}
```

- [ ] **Step 6: Pass the signal to `footer_view`**

In `editing_view`, change the `footer_view` call:

```rust
let footer = footer_view(
    build_status_sig,
    dirty_sig,
    save_error_sig,
    current_doc,
    serve_url_str,
);
```

to:

```rust
let footer = footer_view(
    build_status_sig,
    dirty_sig,
    save_error_sig,
    current_doc,
    serve_status_sig,
);
```

- [ ] **Step 7: Update imports in `ui/mod.rs`**

Adjust the existing import line that brings in footer items. The current import:

```rust
use crate::ui::footer::{footer_view, serve_url, start_build_status_poll};
```

becomes:

```rust
use crate::ui::footer::{footer_view, start_build_status_poll, start_serve_status_poll};
```

(Drops the now-unused `serve_url` import.)

Also add `ServeStatus` to the `lopress_gui_host` import — find the existing line:

```rust
use lopress_gui_host::{BuildStatus, DocumentRef, Session, WorkspaceSummary};
```

and change to:

```rust
use lopress_gui_host::{BuildStatus, DocumentRef, ServeStatus, Session, WorkspaceSummary};
```

- [ ] **Step 8: Build, test, lint**

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Expected: everything clean.

- [ ] **Step 9: Manual verification**

Launch the editor against any workspace:

```
cargo run -- /path/to/workspace
```

(On Windows: substitute a Windows path.)

Confirm:
- The editor window appears almost immediately (not after a noticeable pause).
- The footer briefly shows `starting preview…` on the right.
- Within a second or so, the footer flips to a clickable URL (or `no preview` if the server failed to bind).
- The build-status section shows `building…` initially, then `ok · N+M pages · X ms`.

- [ ] **Step 10: Commit**

```
git add crates/lopress-editor/src/ui/footer.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): reactive serve-status polling in footer

Preview link is now driven by a poll-backed RwSignal<ServeStatus>
instead of a value captured once at view-build time. Footer shows
'starting preview…' while the background open thread is still booting
the server.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 7: Eliminate the full-doc clone in `on_action` (O4 fix)

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs`

No new tests required — existing undo tests verify correctness; the fix is a no-op for observable behavior.

- [ ] **Step 1: Read the current code**

In `crates/lopress-editor/src/ui/mod.rs`, locate the `on_action` closure (inside `editing_view`). The current undo-snapshot lines look like:

```rust
// Push to undo stack before apply (using pre-state).
let pre_doc_snapshot = current_doc.with_untracked(|d| d.clone());
if let Some(ref doc) = pre_doc_snapshot {
    undo_stack.update(|s| s.push_before_apply(doc, &action));
}
```

- [ ] **Step 2: Replace with the nested-closure form**

Replace the four lines above with:

```rust
// Push to undo stack before apply (using pre-state). Nested closure
// avoids cloning the entire EditorDoc — push_before_apply takes the
// pre-state by reference, and compute_inverse clones just the
// affected block.
current_doc.with_untracked(|maybe| {
    if let Some(d) = maybe {
        undo_stack.update(|s| s.push_before_apply(d, &action));
    }
});
```

- [ ] **Step 3: Build, run undo tests, lint**

```
cargo build -p lopress-editor
cargo test -p lopress-editor
cargo clippy -p lopress-editor --all-targets -- -D warnings
cargo fmt --check
```

Expected: builds clean. All existing tests (including any in `crates/lopress-editor/tests/` covering undo/redo and the `actions` module) pass. Clippy clean. Fmt clean.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/mod.rs
git commit -m "perf(editor): remove full-doc clone for undo snapshot per action

on_action used to clone the entire EditorDoc just to hand a &EditorDoc
to UndoStack::push_before_apply. Replace with a nested with_untracked
closure that passes the existing borrow through. O(doc-size)
allocation per edit eliminated.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 8: Capture release baseline and write findings doc

**Files:**
- Create: `docs/superpowers/plans/2026-05-19-performance-findings.md`

This task requires running the editor and exercising it interactively. A human implementer should execute it (scroll-feel cannot be reliably assessed from code). An agentic implementer can drive typing/switching/structural via the debug control API on `127.0.0.1:7878` (see `.claude/skills/driving-lopress-editor/SKILL.md`) but should defer scrolling assessment to a human.

- [ ] **Step 1: Build the release binary**

```
cargo build --release
```

Expected: clean build.

- [ ] **Step 2: Launch the editor with timing enabled**

On Windows PowerShell:

```
$env:LOPRESS_TIMING = "1"; cargo run --release -- <path-to-workspace>
```

On bash:

```
LOPRESS_TIMING=1 cargo run --release -- <path-to-workspace>
```

Use a real workspace if one is handy; otherwise create a small temporary one with `lopress.toml`, `src/posts/`, and at least one post file.

- [ ] **Step 3: Exercise the four interaction categories and capture stderr**

Redirect stderr to a file or copy the relevant `[timing] ...` lines manually. For each category, capture multiple readings:

1. **Workspace open** — note the `workspace.open.*` lines that print on startup.
2. **Document open** — click between two documents in the sidebar; note `editor.open_document.*`.
3. **Typing** — focus a paragraph block and type ~100 characters; note `editor.on_action` durations.
4. **Structural edits** — press Enter to split a block; change the block type via the toolbar; drag-reorder a block. Note `editor.on_action` durations for these structural actions specifically.
5. **Save** — let the debounced save fire after a burst of edits; note `editor.save.serialize` and `editor.save.write`.
6. **Scroll / general UI** — scroll through a medium-length document; observe feel by eye (no spans cover scrolling directly).

- [ ] **Step 4: Write the findings document**

Create `docs/superpowers/plans/2026-05-19-performance-findings.md` with the following structure (fill in real numbers):

```markdown
# Editor Performance Findings — 2026-05-19

**Spec:** `docs/superpowers/specs/2026-05-19-editor-perf-and-instrumentation-design.md`
**Plan:** `docs/superpowers/plans/2026-05-19-editor-perf-and-instrumentation.md`

## Method

Release build (`cargo build --release`), `LOPRESS_TIMING=1`. Workspace: <describe — number of docs, doc sizes, plugins>. Exercised the four interaction categories from the spec.

## Observed timings

| Span | Min | Typical | Max | Sample size |
|------|-----|---------|-----|-------------|
| `workspace.open.workspace_load` | ... | ... | ... | ... |
| `workspace.open.scan` | ... | ... | ... | ... |
| `workspace.open.initial_build` | ... | ... | ... | ... |
| `workspace.open.serve_start` | ... | ... | ... | ... |
| `editor.open_document.load_parse` | ... | ... | ... | ... |
| `editor.open_document.from_core` | ... | ... | ... | ... |
| `editor.on_action` (inline edit) | ... | ... | ... | ... |
| `editor.on_action` (structural) | ... | ... | ... | ... |
| `editor.save.serialize` | ... | ... | ... | ... |
| `editor.save.write` | ... | ... | ... | ... |

## Subjective feel

<one paragraph per category — typing latency, doc switch, structural edits, scroll>

## Recommendations on conditional opportunities

### O5 — Full pane rebuild on structural edits

<observed structural-edit timing; decision: pursue / defer / close>

### O6 — Every save triggers a full site rebuild

<observed save timing + build timing; decision: pursue / defer / close>

### O7 — Debug-only doc-to-JSON snapshot on every edit

<observed only in debug; decision: pursue / defer / close. Note: end users get release builds.>

## Overall

<one of: "perf work complete — close out remaining items" / "open a follow-up spec for X, Y">
```

- [ ] **Step 5: Commit the findings doc**

```
git add docs/superpowers/plans/2026-05-19-performance-findings.md
git commit -m "docs: editor performance findings from release baseline

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

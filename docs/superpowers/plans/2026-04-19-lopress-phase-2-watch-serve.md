# Lopress Phase 2 Implementation Plan — Watcher, Serve, Incremental Build

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a debounced fs watcher (`lopress-watch`), a local dev server with live reload (`lopress-serve` + `lopress serve`), and an incremental build cache to `lopress-build`.

**Architecture:** Two new crates (`lopress-watch`, `lopress-serve`). `lopress-build::build` gains a cache at `www/.lopress-cache.json`; the public API (`build(workspace) -> BuildReport`) is unchanged except for two new `BuildReport` fields. The serve command wires build + watcher + hand-rolled HTTP/1.1 server + SSE broadcast.

**Tech Stack:** `notify = "6"` for filesystem watching. Everything else uses what's already in the workspace. No `axum`/`hyper`/`tokio`/async — the dev server is thread-per-connection `std::net`.

**Spec:** [`docs/superpowers/specs/2026-04-19-lopress-phase-2-watch-serve-design.md`](../specs/2026-04-19-lopress-phase-2-watch-serve-design.md).

**Project conventions (inherited from phase 1):**
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass before every commit.
- Inline format args (`format!("{x}")` not `format!("{}", x)`).
- No unrequested refactors in bug-fix commits.

---

## Task 0: Commit Cargo.lock

Binary crates should commit their lockfile so transitive-MSRV drift (e.g., `constant_time_eq` bumping its MSRV past ours) can't silently break CI and contributor machines.

**Files:**
- Modify: `.gitignore`
- Add: `Cargo.lock`

- [ ] **Step 1: Remove Cargo.lock from .gitignore**

Delete the `Cargo.lock` line from `.gitignore` (it currently sits on its own line).

- [ ] **Step 2: Ensure a lockfile exists and is up to date**

Run: `cargo generate-lockfile` (or just `cargo build` which will create/refresh it).

Verify: `git status` shows `Cargo.lock` as untracked.

- [ ] **Step 3: Stage and commit**

```bash
git add .gitignore Cargo.lock
git commit -m "chore: commit Cargo.lock (binary crate)"
```

---

## Task 1: Add `notify` dep and scaffold `lopress-watch`

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/lopress-watch/Cargo.toml`
- Create: `crates/lopress-watch/src/lib.rs`
- Create: `crates/lopress-watch/src/error.rs`

- [ ] **Step 1: Add notify to workspace deps**

In `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
notify = "6"
```

And add `"crates/lopress-watch"` to `workspace.members`:

```toml
members = [
    "crates/lopress-core",
    "crates/lopress-plugin",
    "crates/lopress-theme",
    "crates/lopress-assets",
    "crates/lopress-build",
    "crates/lopress-watch",
]
```

- [ ] **Step 2: Create the crate manifest**

`crates/lopress-watch/Cargo.toml`:

```toml
[package]
name = "lopress-watch"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
notify = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 3: Create `src/lib.rs` with module declarations only**

```rust
pub mod error;
pub mod watcher;

pub use error::WatchError;
pub use watcher::{ChangeSet, Watcher};
```

- [ ] **Step 4: Create `src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 5: Create a placeholder `src/watcher.rs`** so the crate compiles

```rust
use crate::error::WatchError;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    pub sources: Vec<PathBuf>,
    pub theme: Vec<PathBuf>,
    pub plugins: Vec<PathBuf>,
    pub config: bool,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.theme.is_empty() && self.plugins.is_empty() && !self.config
    }
}

pub struct Watcher {
    _notify: notify::RecommendedWatcher,
    _thread: std::thread::JoinHandle<()>,
}

impl Watcher {
    pub fn spawn(
        _workspace: &std::path::Path,
        _on_change: impl FnMut(ChangeSet) + Send + 'static,
    ) -> Result<Self, WatchError> {
        unimplemented!("filled in by Task 3")
    }
}
```

- [ ] **Step 6: Build and commit**

```bash
cargo build -p lopress-watch
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml crates/lopress-watch
git commit -m "lopress-watch: scaffold crate"
```

---

## Task 2: `ChangeSet` classification helper with tests

The watcher needs a pure function that, given a workspace root and a path, tells us which bucket the path belongs to (source / theme / plugins / config / ignored). Easier to test than the full watcher.

**Files:**
- Create: `crates/lopress-watch/src/classify.rs`
- Modify: `crates/lopress-watch/src/lib.rs` (add `pub mod classify;`)

- [ ] **Step 1: Write failing tests first**

Append to `src/classify.rs`:

```rust
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Source,
    Plugins,
    Config,
    Ignored,
}

/// Classify `path` (absolute or workspace-relative) against the workspace
/// root. Ignored paths include `www/`, `target/`, dot-directories, editor
/// swap files, and anything outside the workspace.
pub fn classify(workspace: &Path, path: &Path) -> Bucket {
    // Canonicalization is expensive; we rely on the caller giving us a
    // path from notify, which is already absolute on every platform we
    // support. If it's relative, resolve against workspace.
    let abs: std::path::PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    };
    let rel = match abs.strip_prefix(workspace) {
        Ok(r) => r,
        Err(_) => return Bucket::Ignored,
    };
    let mut comps = rel.components();
    let first = match comps.next() {
        Some(std::path::Component::Normal(c)) => c.to_string_lossy().into_owned(),
        _ => return Bucket::Ignored,
    };

    if is_editor_noise(rel) {
        return Bucket::Ignored;
    }
    match first.as_str() {
        "www" | "target" => Bucket::Ignored,
        "lopress.toml" => Bucket::Config,
        "src" => Bucket::Source,
        "plugins" => Bucket::Plugins,
        s if s.starts_with('.') => Bucket::Ignored,
        _ => Bucket::Ignored,
    }
}

fn is_editor_noise(rel: &Path) -> bool {
    let name = match rel.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name.starts_with(".#") || name.starts_with('~') || name == "4913" {
        return true;
    }
    if let Some(ext) = rel.extension().and_then(|s| s.to_str()) {
        matches!(ext, "swp" | "swx" | "swo" | "tmp")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        PathBuf::from("/ws")
    }

    #[test]
    fn source_under_src() {
        assert_eq!(classify(&ws(), &ws().join("src/posts/a.md")), Bucket::Source);
    }

    #[test]
    fn plugin_file() {
        assert_eq!(
            classify(&ws(), &ws().join("plugins/callout/plugin.toml")),
            Bucket::Plugins
        );
    }

    #[test]
    fn top_level_config() {
        assert_eq!(classify(&ws(), &ws().join("lopress.toml")), Bucket::Config);
    }

    #[test]
    fn www_is_ignored() {
        assert_eq!(classify(&ws(), &ws().join("www/index.html")), Bucket::Ignored);
    }

    #[test]
    fn dotdirs_are_ignored() {
        assert_eq!(classify(&ws(), &ws().join(".git/HEAD")), Bucket::Ignored);
    }

    #[test]
    fn editor_swap_is_ignored() {
        assert_eq!(classify(&ws(), &ws().join("src/posts/.a.md.swp")), Bucket::Ignored);
    }

    #[test]
    fn emacs_lockfile_ignored() {
        assert_eq!(
            classify(&ws(), &ws().join("src/posts/.#a.md")),
            Bucket::Ignored
        );
    }

    #[test]
    fn outside_workspace_ignored() {
        assert_eq!(classify(&ws(), Path::new("/etc/passwd")), Bucket::Ignored);
    }
}
```

Add the module to `src/lib.rs`:

```rust
pub mod classify;
pub mod error;
pub mod watcher;

pub use classify::{classify, Bucket};
pub use error::WatchError;
pub use watcher::{ChangeSet, Watcher};
```

- [ ] **Step 2: Run tests**

```
cargo test -p lopress-watch
```

Expected: all 8 classify tests pass.

- [ ] **Step 3: Lint + commit**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
git add crates/lopress-watch
git commit -m "lopress-watch: path classification helper"
```

---

## Task 3: Implement the watcher with debounce

**Files:**
- Modify: `crates/lopress-watch/src/watcher.rs`
- Create: `crates/lopress-watch/tests/debounce.rs`

- [ ] **Step 1: Replace the placeholder `watcher.rs`**

```rust
use crate::classify::{classify, Bucket};
use crate::error::WatchError;
use notify::{Event, RecursiveMode, Watcher as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    pub sources: Vec<PathBuf>,
    pub theme: Vec<PathBuf>,
    pub plugins: Vec<PathBuf>,
    pub config: bool,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.theme.is_empty()
            && self.plugins.is_empty()
            && !self.config
    }
}

pub struct Watcher {
    _notify: notify::RecommendedWatcher,
    _thread: thread::JoinHandle<()>,
    _shutdown: Option<mpsc::Sender<()>>,
}

const DEBOUNCE: Duration = Duration::from_millis(200);

impl Watcher {
    pub fn spawn(
        workspace: &Path,
        mut on_change: impl FnMut(ChangeSet) + Send + 'static,
    ) -> Result<Self, WatchError> {
        let workspace = workspace.to_path_buf();
        let (tx, rx) = mpsc::channel::<Event>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let mut notify = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                let _ = tx.send(ev);
            }
        })?;

        // Watch the workspace root recursively. classify() filters per-event.
        if workspace.exists() {
            notify.watch(&workspace, RecursiveMode::Recursive)?;
        }

        let worker_ws = workspace.clone();
        let handle = thread::spawn(move || {
            debounce_loop(&worker_ws, rx, shutdown_rx, &mut on_change);
        });

        Ok(Self {
            _notify: notify,
            _thread: handle,
            _shutdown: Some(shutdown_tx),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(tx) = self._shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn debounce_loop(
    workspace: &Path,
    rx: mpsc::Receiver<Event>,
    shutdown: mpsc::Receiver<()>,
    on_change: &mut dyn FnMut(ChangeSet),
) {
    let mut pending: Option<ChangeSet> = None;
    let mut deadline: Option<Instant> = None;

    loop {
        if shutdown.try_recv().is_ok() {
            return;
        }
        let wait = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(50));
        match rx.recv_timeout(wait) {
            Ok(ev) => {
                let cs = pending.get_or_insert_with(ChangeSet::default);
                for path in ev.paths {
                    match classify(workspace, &path) {
                        Bucket::Source => push_unique(&mut cs.sources, path),
                        Bucket::Plugins => push_unique(&mut cs.plugins, path),
                        Bucket::Config => cs.config = true,
                        Bucket::Ignored => {}
                    }
                }
                deadline = Some(Instant::now() + DEBOUNCE);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let (Some(d), Some(cs)) = (deadline, pending.as_ref()) {
                    if Instant::now() >= d && !cs.is_empty() {
                        let cs = pending.take().unwrap();
                        on_change(cs);
                        deadline = None;
                    } else if cs.is_empty() {
                        pending = None;
                        deadline = None;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}
```

- [ ] **Step 2: Write the debounce integration test**

`crates/lopress-watch/tests/debounce.rs`:

```rust
use lopress_watch::{ChangeSet, Watcher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn flush(lock: &Arc<Mutex<Vec<ChangeSet>>>, timeout: Duration) -> Vec<ChangeSet> {
    let start = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let v = lock.lock().unwrap();
        if !v.is_empty() || start.elapsed() >= timeout {
            return v.clone();
        }
    }
}

#[test]
fn coalesces_rapid_writes_into_one_changeset() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();

    // Give notify time to arm.
    std::thread::sleep(Duration::from_millis(200));

    for i in 0..5 {
        let p = root.join(format!("src/posts/a{i}.md"));
        std::fs::write(&p, format!("hello {i}")).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    let batches = flush(&seen, Duration::from_secs(3));
    assert_eq!(batches.len(), 1, "expected 1 debounced batch, got {}", batches.len());
    assert!(!batches[0].sources.is_empty());
    assert!(batches[0].plugins.is_empty());
    assert!(!batches[0].config);
}

#[test]
fn separate_bursts_produce_separate_batches() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    std::fs::write(root.join("src/posts/a.md"), "burst 1").unwrap();
    std::thread::sleep(Duration::from_millis(600));
    std::fs::write(root.join("src/posts/b.md"), "burst 2").unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        if seen.lock().unwrap().len() >= 2 {
            break;
        }
    }
    let batches = seen.lock().unwrap().clone();
    assert!(batches.len() >= 2, "expected >=2 debounced batches, got {}", batches.len());
}

#[test]
fn ignores_www_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("www")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    std::fs::write(root.join("www/index.html"), "hi").unwrap();
    std::thread::sleep(Duration::from_millis(800));
    assert!(seen.lock().unwrap().is_empty(), "www write should be ignored");
}
```

- [ ] **Step 3: Run**

```
cargo test -p lopress-watch
```

Expected: all tests (classify unit tests + 3 integration tests) pass. Watcher tests can be flaky under load — if a test fails once, retry once. If it fails twice, diagnose.

- [ ] **Step 4: Lint + commit**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
git add crates/lopress-watch
git commit -m "lopress-watch: debounced fs watcher with ChangeSet classification"
```

---

## Task 4: Build-cache schema and I/O

**Files:**
- Create: `crates/lopress-build/src/cache.rs`
- Modify: `crates/lopress-build/src/lib.rs` (add `pub mod cache;`)

- [ ] **Step 1: Write the cache module**

```rust
use crate::error::BuildError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCache {
    pub version: u32,
    #[serde(default)]
    pub config_hash: String,
    #[serde(default)]
    pub theme_hash: String,
    #[serde(default)]
    pub plugins_hash: String,
    #[serde(default)]
    pub pages: BTreeMap<String, PageEntry>,
}

impl Default for BuildCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            config_hash: String::new(),
            theme_hash: String::new(),
            plugins_hash: String::new(),
            pages: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageEntry {
    pub source_hash: String,
    pub outputs: Vec<String>, // workspace-relative, forward-slash
    pub tags: Vec<String>,
    pub is_draft: bool,
    pub title: Option<String>,
    pub date: Option<String>,
}

impl BuildCache {
    pub fn load(path: &Path) -> Result<Self, BuildError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        let parsed: Self = serde_json::from_str(&s)?;
        if parsed.version != CACHE_VERSION {
            return Ok(Self::default());
        }
        Ok(parsed)
    }

    pub fn save(&self, path: &Path) -> Result<(), BuildError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }
}

/// Workspace-relative, forward-slash path, for cache keys and output lists.
pub fn rel_key(workspace: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(workspace).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Hash a list of (relative-key, bytes) pairs, order-independent.
pub fn hash_many(items: &mut [(String, Vec<u8>)]) -> String {
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    for (k, v) in items.iter() {
        hasher.update(k.as_bytes());
        hasher.update(&[0]);
        hasher.update(v);
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_cache_is_version_1() {
        assert_eq!(BuildCache::default().version, 1);
    }

    #[test]
    fn roundtrip_via_json() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        let mut c = BuildCache::default();
        c.config_hash = "abc".into();
        c.pages.insert(
            "src/posts/a.md".into(),
            PageEntry {
                source_hash: "h".into(),
                outputs: vec!["posts/a/index.html".into()],
                tags: vec!["x".into()],
                is_draft: false,
                title: Some("A".into()),
                date: None,
            },
        );
        c.save(&p).unwrap();
        let back = BuildCache::load(&p).unwrap();
        assert_eq!(back.config_hash, "abc");
        assert_eq!(back.pages.len(), 1);
    }

    #[test]
    fn version_mismatch_returns_default() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        std::fs::write(&p, r#"{"version":99,"pages":{}}"#).unwrap();
        let back = BuildCache::load(&p).unwrap();
        assert_eq!(back.version, CACHE_VERSION);
        assert!(back.pages.is_empty());
    }

    #[test]
    fn hash_many_is_order_independent() {
        let mut a = vec![("a".into(), b"1".to_vec()), ("b".into(), b"2".to_vec())];
        let mut b = vec![("b".into(), b"2".to_vec()), ("a".into(), b"1".to_vec())];
        assert_eq!(hash_many(&mut a), hash_many(&mut b));
    }
}
```

- [ ] **Step 2: Wire into lib.rs**

In `crates/lopress-build/src/lib.rs`, add `pub mod cache;` near the other module declarations, and re-export:

```rust
pub use cache::{BuildCache, PageEntry};
```

- [ ] **Step 3: Run tests, lint, commit**

```
cargo test -p lopress-build cache
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-build
git commit -m "lopress-build: cache schema with load/save and hash helpers"
```

---

## Task 5: Incremental hash computation helpers

These compute the three top-level hashes (config/theme/plugins) and the per-source hash. They live in `cache.rs` to keep all hashing in one place.

**Files:**
- Modify: `crates/lopress-build/src/cache.rs`

- [ ] **Step 1: Append helpers**

```rust
use crate::site::Workspace;
use lopress_plugin::PluginRegistry;
use lopress_theme::ResolvedTheme;

pub fn hash_config(workspace: &Workspace) -> Result<String, BuildError> {
    let bytes = std::fs::read(workspace.root.join("lopress.toml"))?;
    Ok(hash_bytes(&bytes))
}

/// Hash of every template in the resolved theme + the theme CSS.
/// For the built-in theme (`css_path` is `None`), we hash the embedded
/// templates in a stable order plus the CSS content.
pub fn hash_theme(theme: &ResolvedTheme) -> Result<String, BuildError> {
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(css_path) = &theme.css_path {
        let templates_dir = css_path.parent().unwrap().join("templates");
        if templates_dir.exists() {
            for entry in std::fs::read_dir(&templates_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("html") {
                    let name = entry
                        .path()
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    let bytes = std::fs::read(entry.path())?;
                    items.push((format!("tpl/{name}"), bytes));
                }
            }
        }
        items.push((
            "css".into(),
            std::fs::read(css_path)?,
        ));
    } else {
        for name in [
            "layout.html",
            "post.html",
            "page.html",
            "index.html",
            "tag.html",
            "404.html",
        ] {
            if let Some(src) = lopress_theme::builtin_template(name) {
                items.push((format!("tpl/{name}"), src.as_bytes().to_vec()));
            }
        }
        items.push(("css".into(), theme.css_content.as_bytes().to_vec()));
    }
    Ok(hash_many(&mut items))
}

pub fn hash_plugins(registry: &PluginRegistry) -> Result<String, BuildError> {
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();
    for plugin in &registry.plugins {
        let name = &plugin.manifest.name;
        let manifest_bytes = std::fs::read(plugin.root.join("plugin.toml"))?;
        items.push((format!("{name}/plugin.toml"), manifest_bytes));
        for block in &plugin.manifest.blocks {
            let tpl_rel = &block.template;
            let tpl_bytes = std::fs::read(plugin.root.join(tpl_rel))?;
            items.push((format!("{name}/{tpl_rel}"), tpl_bytes));
        }
        let assets = plugin.root.join("assets");
        if assets.exists() {
            for entry in walkdir::WalkDir::new(&assets) {
                let entry = entry.map_err(std::io::Error::other)?;
                if entry.file_type().is_file() {
                    let rel = entry.path().strip_prefix(&assets).unwrap();
                    let key = format!(
                        "{name}/assets/{}",
                        rel.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    );
                    let bytes = std::fs::read(entry.path())?;
                    items.push((key, bytes));
                }
            }
        }
    }
    Ok(hash_many(&mut items))
}

pub fn hash_file(path: &Path) -> Result<String, BuildError> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}
```

- [ ] **Step 2: Add a unit test for hash_config determinism**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn hash_config_is_stable_and_changes_with_content() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        let ws = crate::site::Workspace::load(d.path()).unwrap();
        let h1 = hash_config(&ws).unwrap();
        let h2 = hash_config(&ws).unwrap();
        assert_eq!(h1, h2);

        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"B\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        let ws2 = crate::site::Workspace::load(d.path()).unwrap();
        assert_ne!(h1, hash_config(&ws2).unwrap());
    }
```

- [ ] **Step 3: Run, lint, commit**

```
cargo test -p lopress-build cache
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-build
git commit -m "lopress-build: hash helpers for config, theme, plugins"
```

---

## Task 6: Incremental build — cache-aware render path

Rewrite the render phase of `build.rs` to consult the cache. Full rebuild is the fallback; per-file incremental is the happy path.

This task is the most substantial. Take it in small commits if needed.

**Files:**
- Modify: `crates/lopress-build/src/build.rs`
- Modify: `crates/lopress-build/src/pages.rs`

- [ ] **Step 1: Expand `BuildReport`**

In `build.rs`:

```rust
pub struct BuildReport {
    pub pages_written: usize,
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
}
```

Update the existing return construction site.

- [ ] **Step 2: Change `pages::render_all` to accept the cache and return per-page stats**

Change the signature:

```rust
pub fn render_all(
    workspace: &Workspace,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
    posts: &[DiscoveredPost],
    pages: &[DiscoveredPost],
    cache: &mut crate::cache::BuildCache,
    force_full: bool,
) -> Result<RenderStats, BuildError>

pub struct RenderStats {
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
}
```

Inside `render_all`, for each post and each page:

1. Compute `key = cache::rel_key(&workspace.root, &p.source_path)`.
2. Compute `source_hash = cache::hash_file(&p.source_path)?`.
3. Look up `cache.pages.get(&key)`.
4. If `force_full` is true → render.
5. Else if the cache entry exists, `source_hash` matches, **and all `outputs` exist on disk** → skip rendering, bump `pages_skipped`, keep the cache entry as-is.
6. Else → render, then update the cache entry with `{ source_hash, outputs: [..], tags, is_draft, title, date }`. Bump `pages_rendered`.

For posts, the outputs list is `[format!("posts/{slug}/index.html")]`. For pages, `[format!("{slug}/index.html")]`. (Use forward slashes always; these are URL-like cache keys, not OS paths.)

After rendering all posts/pages:

- Always rebuild the aggregate pages (index, feed, sitemap, tags) if **any** post was rendered/added/deleted, or on `force_full`. Easiest: track a `post_set_changed` bool in `render_all`, return it in `RenderStats`, and let `build()` decide.

Replace the post-rendering loop accordingly. Keep `render_one_post`/`render_one_page` unchanged.

- [ ] **Step 3: Orphan cleanup**

After the render loop, compute the set of keys present in `posts ∪ pages` and remove cache entries (and their outputs on disk) that aren't in that set. When removing outputs, also remove the now-empty parent directory if the directory is empty.

Implement as a helper inside `render_all`:

```rust
fn prune_orphans(
    workspace: &Workspace,
    cache: &mut crate::cache::BuildCache,
    live_keys: &std::collections::BTreeSet<String>,
) -> Result<bool, BuildError> {
    let stale: Vec<String> = cache
        .pages
        .keys()
        .filter(|k| !live_keys.contains(*k))
        .cloned()
        .collect();
    let changed = !stale.is_empty();
    for key in stale {
        if let Some(entry) = cache.pages.remove(&key) {
            for output in &entry.outputs {
                let p = workspace.www_dir().join(output);
                let _ = std::fs::remove_file(&p);
                if let Some(parent) = p.parent() {
                    let _ = std::fs::remove_dir(parent); // removes only if empty
                }
            }
        }
    }
    Ok(changed)
}
```

- [ ] **Step 4: Rewrite `build()` to drive the cache**

In `build.rs::build`:

1. Load workspace. Build plugins registry and theme as before.
2. Load cache from `workspace.cache_path()`.
3. Compute `config_hash`, `theme_hash`, `plugins_hash`.
4. `let force_full = cache.config_hash != cfg_hash || cache.theme_hash != theme_hash || cache.plugins_hash != plugins_hash;`
5. If `force_full` is true → clear `cache.pages` AND delete the existing `www/` tree (except the image variant cache file) so stale files don't linger.

   ```rust
   if force_full {
       cache.pages.clear();
       if ws.www_dir().exists() {
           for entry in std::fs::read_dir(ws.www_dir())? {
               let entry = entry?;
               let name = entry.file_name();
               if name == std::ffi::OsString::from(".lopress-image-cache.json") {
                   continue;
               }
               let p = entry.path();
               if p.is_dir() {
                   std::fs::remove_dir_all(&p)?;
               } else {
                   std::fs::remove_file(&p)?;
               }
           }
       }
   }
   ```
6. Build Tera (unchanged).
7. Discover posts/pages.
8. Call `render_all` with the cache and `force_full`.
9. Decide whether to regenerate aggregate pages: `force_full || stats.pages_rendered > 0 || stats.post_set_changed`.
10. Write feed/sitemap/robots/404 only if regenerating aggregates.
11. Always write `assets/theme.css` on `force_full` and skip otherwise (theme_hash already guarantees it's unchanged).
12. Always copy plugin assets on `force_full` only (same reasoning).
13. Image pipeline: unchanged (it has its own per-variant cache).
14. Update `cache.config_hash`, `cache.theme_hash`, `cache.plugins_hash` to the freshly computed values.
15. Save cache.
16. Build `BuildReport { pages_written, pages_rendered, pages_skipped, failures }`.

`pages_written` semantics stay: total number of page HTML files on disk after the build. Easiest: `cache.pages.len() + pages_src.len() + tag_count + 1` is no longer right because `cache.pages` counts both posts and pages. Simpler: `pages_written = cache.pages.values().filter(|e| !e.is_draft).map(|e| e.outputs.len()).sum::<usize>() + tag_count + 1`. Compute after cache is finalized.

- [ ] **Step 5: Run phase-1 integration tests**

```
cargo test -p lopress-build --test build_integration
```

Expected: all 4 phase-1 integration tests pass unchanged.

- [ ] **Step 6: Lint, commit**

```
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-build
git commit -m "lopress-build: incremental cache-driven render path"
```

---

## Task 7: Integration tests for incrementality

**Files:**
- Modify: `crates/lopress-build/tests/build_integration.rs`

- [ ] **Step 1: Append these tests**

```rust
#[test]
fn incremental_skips_unchanged_posts() {
    let (_tmp, root) = copy_fixture("minimal");
    let r1 = build(&root).unwrap();
    assert!(r1.failures.is_empty());
    let first_rendered = r1.pages_rendered;
    assert!(first_rendered >= 1);

    let r2 = build(&root).unwrap();
    assert!(r2.failures.is_empty());
    assert_eq!(r2.pages_rendered, 0, "second build should render nothing");
    assert!(r2.pages_skipped >= 1);
}

#[test]
fn editing_one_post_rerenders_only_that_post() {
    let (_tmp, root) = copy_fixture("minimal");
    build(&root).unwrap();

    let hello = root.join("src/posts/hello.md");
    let src = fs::read_to_string(&hello).unwrap();
    fs::write(&hello, format!("{src}\nextra content\n")).unwrap();

    let r2 = build(&root).unwrap();
    assert_eq!(r2.pages_rendered, 1, "only hello.md should re-render");
    assert!(r2.pages_skipped >= 1);
}

#[test]
fn editing_config_triggers_full_rebuild() {
    let (_tmp, root) = copy_fixture("minimal");
    let r1 = build(&root).unwrap();
    let rendered_first = r1.pages_rendered;

    let cfg = root.join("lopress.toml");
    let src = fs::read_to_string(&cfg).unwrap();
    fs::write(&cfg, format!("{src}\n# comment\n")).unwrap();

    let r2 = build(&root).unwrap();
    assert_eq!(r2.pages_rendered, rendered_first, "config change should rerender everything");
    assert_eq!(r2.pages_skipped, 0);
}

#[test]
fn deleted_post_is_removed_from_output() {
    let (_tmp, root) = copy_fixture("minimal");
    build(&root).unwrap();
    let out = root.join("www/posts/hello/index.html");
    assert!(out.exists());

    fs::remove_file(root.join("src/posts/hello.md")).unwrap();
    build(&root).unwrap();
    assert!(!out.exists(), "deleted post should be pruned from www/");
}
```

- [ ] **Step 2: Run, lint, commit**

```
cargo test -p lopress-build --test build_integration
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-build/tests
git commit -m "lopress-build: integration tests for incremental cache"
```

---

## Task 8: Scaffold `lopress-serve` crate

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/lopress-serve/Cargo.toml`
- Create: `crates/lopress-serve/src/{lib.rs,error.rs,http.rs,mime.rs,router.rs,inject.rs,sse.rs,server.rs}`

- [ ] **Step 1: Workspace deps**

In root `Cargo.toml`, add `"crates/lopress-serve"` to members.

- [ ] **Step 2: Crate manifest**

`crates/lopress-serve/Cargo.toml`:

```toml
[package]
name = "lopress-serve"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
thiserror = { workspace = true }
lopress-build = { path = "../lopress-build" }
lopress-watch = { path = "../lopress-watch" }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 3: `src/lib.rs`**

```rust
pub mod error;
pub mod http;
pub mod inject;
pub mod mime;
pub mod router;
pub mod server;
pub mod sse;

pub use error::ServeError;
pub use server::{serve, ServeOptions};
```

- [ ] **Step 4: `src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("build: {0}")]
    Build(#[from] lopress_build::BuildError),
    #[error("watch: {0}")]
    Watch(#[from] lopress_watch::WatchError),
    #[error("bind {addr}: {source}")]
    Bind {
        addr: String,
        #[source]
        source: std::io::Error,
    },
}
```

- [ ] **Step 5: Empty placeholder modules**

Create these files with just `// filled in subsequent tasks`:

- `src/http.rs`
- `src/mime.rs`
- `src/router.rs`
- `src/inject.rs`
- `src/sse.rs`

For `src/server.rs`:

```rust
use crate::error::ServeError;
use std::path::PathBuf;

pub struct ServeOptions {
    pub workspace: PathBuf,
    pub bind: String,
    pub port: u16,
    pub open_browser: bool,
}

pub fn serve(_opts: ServeOptions) -> Result<(), ServeError> {
    unimplemented!("filled in by Task 13")
}
```

- [ ] **Step 6: Build + commit**

```
cargo build -p lopress-serve
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add Cargo.toml crates/lopress-serve
git commit -m "lopress-serve: scaffold crate"
```

---

## Task 9: HTTP/1.1 request parser

**Files:**
- Modify: `crates/lopress-serve/src/http.rs`

- [ ] **Step 1: Fill in `http.rs`**

```rust
use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;

pub struct Request {
    pub method: String,
    pub path: String,
    #[allow(dead_code)]
    pub headers: Vec<(String, String)>,
}

pub fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.trim_end().split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    if method.is_empty() || path.is_empty() {
        return Ok(None);
    }

    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h)?;
        if n == 0 {
            break;
        }
        let trimmed = h.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Ok(Some(Request {
        method,
        path,
        headers,
    }))
}

pub fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;
    write!(stream, "HTTP/1.1 {status} {reason}\r\n")?;
    let mut has_len = false;
    let mut has_type = false;
    for (k, v) in headers {
        write!(stream, "{k}: {v}\r\n")?;
        if k.eq_ignore_ascii_case("content-length") {
            has_len = true;
        }
        if k.eq_ignore_ascii_case("content-type") {
            has_type = true;
        }
    }
    if !has_len {
        write!(stream, "content-length: {}\r\n", body.len())?;
    }
    if !has_type {
        write!(stream, "content-type: application/octet-stream\r\n")?;
    }
    write!(stream, "connection: close\r\n\r\n")?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Read and discard the request body (if any) so we can close cleanly.
#[allow(dead_code)]
pub fn drain(stream: &TcpStream) {
    let mut buf = [0u8; 1024];
    let mut s = stream.try_clone().unwrap();
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(50)));
    while s.read(&mut buf).unwrap_or(0) > 0 {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn roundtrip_request(raw: &[u8]) -> Option<(String, String)> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_request(&stream).unwrap()
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(raw).unwrap();
        drop(client);
        let req = handle.join().unwrap()?;
        Some((req.method, req.path))
    }

    #[test]
    fn parses_simple_get() {
        let (m, p) = roundtrip_request(b"GET /foo HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
        assert_eq!(m, "GET");
        assert_eq!(p, "/foo");
    }

    #[test]
    fn empty_connection_returns_none() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_request(&stream).unwrap()
        });
        drop(TcpStream::connect(addr).unwrap());
        assert!(handle.join().unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run, lint, commit**

```
cargo test -p lopress-serve http
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/src/http.rs
git commit -m "lopress-serve: minimal HTTP/1.1 request parser"
```

---

## Task 10: MIME types and reload script injection

**Files:**
- Modify: `crates/lopress-serve/src/mime.rs`
- Modify: `crates/lopress-serve/src/inject.rs`

- [ ] **Step 1: `mime.rs`**

```rust
pub fn guess(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("xml") => "application/xml; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn html_is_utf8() {
        assert!(guess(Path::new("a.html")).contains("text/html"));
    }

    #[test]
    fn unknown_falls_back() {
        assert_eq!(guess(Path::new("a.xyz")), "application/octet-stream");
    }
}
```

- [ ] **Step 2: `inject.rs`**

```rust
pub const RELOAD_SCRIPT: &str = "<script>\n\
(() => {\n\
  const es = new EventSource('/__lopress/reload');\n\
  es.addEventListener('reload', () => location.reload());\n\
})();\n\
</script>\n";

pub fn inject_reload_script(html: &[u8]) -> Vec<u8> {
    let s = match std::str::from_utf8(html) {
        Ok(s) => s,
        Err(_) => return html.to_vec(),
    };
    if let Some(idx) = s.rfind("</body>") {
        let mut out = String::with_capacity(s.len() + RELOAD_SCRIPT.len());
        out.push_str(&s[..idx]);
        out.push_str(RELOAD_SCRIPT);
        out.push_str(&s[idx..]);
        out.into_bytes()
    } else {
        let mut v = html.to_vec();
        v.extend_from_slice(RELOAD_SCRIPT.as_bytes());
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_before_body_close() {
        let html = b"<html><body><h1>Hi</h1></body></html>";
        let out = String::from_utf8(inject_reload_script(html)).unwrap();
        assert!(out.contains("EventSource"));
        assert!(out.find("EventSource").unwrap() < out.find("</body>").unwrap());
    }

    #[test]
    fn appends_when_no_body_close() {
        let html = b"<h1>plain</h1>";
        let out = String::from_utf8(inject_reload_script(html)).unwrap();
        assert!(out.ends_with("</script>\n"));
    }

    #[test]
    fn leaves_invalid_utf8_untouched() {
        let html = &[0xffu8, 0xfe, 0xfd];
        assert_eq!(inject_reload_script(html), html);
    }
}
```

- [ ] **Step 3: Run, lint, commit**

```
cargo test -p lopress-serve mime inject
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/src/{mime.rs,inject.rs}
git commit -m "lopress-serve: MIME guessing and reload script injection"
```

---

## Task 11: Static file router

Given a request path and the `www/` root, resolve to a response: file bytes + content-type + status, or a redirect, or 404.

**Files:**
- Modify: `crates/lopress-serve/src/router.rs`

- [ ] **Step 1: Fill in `router.rs`**

```rust
use crate::inject::inject_reload_script;
use crate::mime;
use std::path::{Path, PathBuf};

pub enum Resolved {
    File {
        content_type: &'static str,
        body: Vec<u8>,
    },
    Redirect {
        location: String,
    },
    NotFound {
        body: Vec<u8>,
    },
    Forbidden,
}

pub fn resolve(www: &Path, req_path: &str) -> std::io::Result<Resolved> {
    // Drop query string and fragment.
    let path = req_path.split('?').next().unwrap_or("");
    let path = path.split('#').next().unwrap_or("");

    // Percent-decoding: we keep it minimal. Reject anything containing `..`
    // in any form before join.
    if path.contains("..") {
        return Ok(Resolved::Forbidden);
    }

    let rel = path.trim_start_matches('/');
    let candidate: PathBuf = if rel.is_empty() {
        www.join("index.html")
    } else if path.ends_with('/') {
        www.join(rel).join("index.html")
    } else {
        www.join(rel)
    };

    // Ensure the canonical path stays under www/.
    let abs_www = www.canonicalize().unwrap_or_else(|_| www.to_path_buf());
    if let Ok(abs) = candidate.canonicalize() {
        if !abs.starts_with(&abs_www) {
            return Ok(Resolved::Forbidden);
        }
    }

    if candidate.is_file() {
        let bytes = std::fs::read(&candidate)?;
        let ct = mime::guess(&candidate);
        let body = if ct.starts_with("text/html") {
            inject_reload_script(&bytes)
        } else {
            bytes
        };
        return Ok(Resolved::File {
            content_type: ct,
            body,
        });
    }

    // If /foo lacks a trailing slash but /foo/index.html exists, redirect.
    if !path.ends_with('/') && !rel.is_empty() {
        let with_index = www.join(rel).join("index.html");
        if with_index.is_file() {
            return Ok(Resolved::Redirect {
                location: format!("{path}/"),
            });
        }
    }

    // 404 with the site's 404.html if present.
    let custom = www.join("404.html");
    let body = if custom.is_file() {
        let bytes = std::fs::read(&custom)?;
        inject_reload_script(&bytes)
    } else {
        b"404 Not Found".to_vec()
    };
    Ok(Resolved::NotFound { body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let d = TempDir::new().unwrap();
        let www = d.path();
        std::fs::write(www.join("index.html"), "<body>home</body>").unwrap();
        std::fs::create_dir_all(www.join("posts/hello")).unwrap();
        std::fs::write(www.join("posts/hello/index.html"), "<body>hi</body>").unwrap();
        std::fs::write(www.join("style.css"), "x{}").unwrap();
        std::fs::write(www.join("404.html"), "<body>missing</body>").unwrap();
        d
    }

    #[test]
    fn root_serves_index() {
        let d = setup();
        match resolve(d.path(), "/").unwrap() {
            Resolved::File { body, content_type } => {
                assert!(content_type.starts_with("text/html"));
                let s = String::from_utf8(body).unwrap();
                assert!(s.contains("home"));
                assert!(s.contains("EventSource"));
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn directory_path_with_slash_serves_index() {
        let d = setup();
        match resolve(d.path(), "/posts/hello/").unwrap() {
            Resolved::File { body, .. } => {
                assert!(String::from_utf8(body).unwrap().contains("hi"));
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn directory_path_without_slash_redirects() {
        let d = setup();
        match resolve(d.path(), "/posts/hello").unwrap() {
            Resolved::Redirect { location } => assert_eq!(location, "/posts/hello/"),
            _ => panic!("expected redirect"),
        }
    }

    #[test]
    fn css_not_injected() {
        let d = setup();
        match resolve(d.path(), "/style.css").unwrap() {
            Resolved::File { body, content_type } => {
                assert!(content_type.starts_with("text/css"));
                assert_eq!(body, b"x{}");
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn missing_path_serves_404() {
        let d = setup();
        match resolve(d.path(), "/no/such").unwrap() {
            Resolved::NotFound { body } => {
                assert!(String::from_utf8(body).unwrap().contains("missing"));
            }
            _ => panic!("expected 404"),
        }
    }

    #[test]
    fn dotdot_rejected() {
        let d = setup();
        matches!(resolve(d.path(), "/../etc/passwd").unwrap(), Resolved::Forbidden);
    }
}
```

- [ ] **Step 2: Run, lint, commit**

```
cargo test -p lopress-serve router
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/src/router.rs
git commit -m "lopress-serve: static file router with MIME and reload injection"
```

---

## Task 12: SSE subscriber registry and broadcast

**Files:**
- Modify: `crates/lopress-serve/src/sse.rs`

- [ ] **Step 1: Fill in `sse.rs`**

```rust
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct Subscribers {
    inner: Arc<Mutex<Vec<TcpStream>>>,
}

impl Subscribers {
    pub fn add(&self, mut stream: TcpStream) -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\n\
             content-type: text/event-stream\r\n\
             cache-control: no-cache\r\n\
             connection: keep-alive\r\n\
             \r\n\
             retry: 1000\n\n"
        )?;
        stream.flush()?;
        self.inner.lock().unwrap().push(stream);
        Ok(())
    }

    pub fn broadcast_reload(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.retain_mut(|s| {
            write!(s, "event: reload\ndata: {{}}\n\n")
                .and_then(|_| s.flush())
                .is_ok()
        });
    }

    pub fn ping_loop(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if last.elapsed() < Duration::from_secs(15) {
                    continue;
                }
                last = Instant::now();
                let mut guard = self.inner.lock().unwrap();
                guard.retain_mut(|s| {
                    write!(s, ":ping\n\n")
                        .and_then(|_| s.flush())
                        .is_ok()
                });
            }
        })
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn add_writes_sse_headers_and_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (server_side, _) = listener.accept().unwrap();
            let subs = Subscribers::default();
            subs.add(server_side).unwrap();
            subs
        });
        let mut client = TcpStream::connect(addr).unwrap();
        // Drain everything the server writes immediately.
        let mut buf = [0u8; 512];
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let n = client.read(&mut buf).unwrap();
        let s = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(s.contains("text/event-stream"));
        assert!(s.contains("retry: 1000"));
        let subs = handle.join().unwrap();
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn broadcast_writes_reload_event() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_thread = std::thread::spawn(move || {
            let (server_side, _) = listener.accept().unwrap();
            let subs = Subscribers::default();
            subs.add(server_side).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            subs.broadcast_reload();
            std::thread::sleep(Duration::from_millis(100));
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let mut all = Vec::new();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0u8; 512];
        // Read twice: once for handshake, once for broadcast.
        for _ in 0..2 {
            match client.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => all.extend_from_slice(&buf[..n]),
            }
        }
        let s = String::from_utf8_lossy(&all);
        assert!(s.contains("event: reload"), "got: {s}");
        server_thread.join().unwrap();
    }
}
```

- [ ] **Step 2: Run, lint, commit**

```
cargo test -p lopress-serve sse
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/src/sse.rs
git commit -m "lopress-serve: SSE subscriber registry and reload broadcast"
```

---

## Task 13: Server assembly

Tie everything together: build once, bind the listener, spawn the watcher, accept connections on a thread per request, rebuild and broadcast on change events.

**Files:**
- Modify: `crates/lopress-serve/src/server.rs`

- [ ] **Step 1: Fill in `server.rs`**

```rust
use crate::error::ServeError;
use crate::http::{read_request, write_response};
use crate::router::{resolve, Resolved};
use crate::sse::Subscribers;
use lopress_watch::Watcher;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ServeOptions {
    pub workspace: PathBuf,
    pub bind: String,
    pub port: u16,
    pub open_browser: bool,
}

pub fn serve(opts: ServeOptions) -> Result<(), ServeError> {
    // 1. Initial full build.
    let report = lopress_build::build(&opts.workspace)?;
    eprintln!(
        "initial build: {} rendered, {} skipped, {} failure(s)",
        report.pages_rendered,
        report.pages_skipped,
        report.failures.len()
    );

    // 2. Bind HTTP listener.
    let addr = format!("{}:{}", opts.bind, opts.port);
    let listener = TcpListener::bind(&addr).map_err(|source| ServeError::Bind {
        addr: addr.clone(),
        source,
    })?;
    eprintln!("serving http://{addr}/  (watching {})", opts.workspace.display());

    let subs = Subscribers::default();
    let _ping = subs.clone().ping_loop();

    // 3. Watcher: on change, rebuild and broadcast.
    let ws = opts.workspace.clone();
    let subs_for_watch = subs.clone();
    let _watcher = Watcher::spawn(&opts.workspace, move |_cs| {
        match lopress_build::build(&ws) {
            Ok(r) => {
                eprintln!(
                    "rebuild: {} rendered, {} skipped, {} failure(s)",
                    r.pages_rendered,
                    r.pages_skipped,
                    r.failures.len()
                );
                subs_for_watch.broadcast_reload();
            }
            Err(e) => eprintln!("rebuild failed: {e}"),
        }
    })?;

    // 4. Optionally open the default browser.
    if opts.open_browser {
        let url = format!("http://{addr}/");
        std::thread::spawn(move || open_url(&url));
    }

    // 5. Accept loop.
    let www = Arc::new(opts.workspace.join("www"));
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let www = Arc::clone(&www);
        let subs = subs.clone();
        std::thread::spawn(move || {
            let _ = handle_conn(stream, &www, &subs);
        });
    }
    Ok(())
}

fn handle_conn(
    mut stream: std::net::TcpStream,
    www: &std::path::Path,
    subs: &Subscribers,
) -> std::io::Result<()> {
    let req = match read_request(&stream)? {
        Some(r) => r,
        None => return Ok(()),
    };
    if req.method != "GET" {
        return write_response(&mut stream, 405, "Method Not Allowed", &[], b"");
    }
    if req.path.starts_with("/__lopress/reload") {
        // Hand the stream to the SSE subscribers; return without closing.
        return subs.add(stream);
    }

    match resolve(www, &req.path)? {
        Resolved::File { content_type, body } => write_response(
            &mut stream,
            200,
            "OK",
            &[("content-type", content_type)],
            &body,
        ),
        Resolved::Redirect { location } => {
            write_response(&mut stream, 301, "Moved Permanently", &[("location", &location)], b"")
        }
        Resolved::NotFound { body } => write_response(
            &mut stream,
            404,
            "Not Found",
            &[("content-type", "text/html; charset=utf-8")],
            &body,
        ),
        Resolved::Forbidden => write_response(&mut stream, 403, "Forbidden", &[], b""),
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}
```

- [ ] **Step 2: Lint + commit** (no new tests yet — integration test in next task)

```
cargo build -p lopress-serve
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/src/server.rs
git commit -m "lopress-serve: wire build, watcher, HTTP server, SSE"
```

---

## Task 14: Integration test for the serve command

Bind to port 0, GET `/`, check reload script is present. GET `/__lopress/reload`, check headers.

**Files:**
- Create: `crates/lopress-serve/tests/serve_integration.rs`

- [ ] **Step 1: Write the test**

```rust
use lopress_serve::{serve, ServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn make_minimal_workspace(root: &std::path::Path) {
    std::fs::write(
        root.join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://example.com\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();
    std::fs::write(
        root.join("src/posts/hi.md"),
        "---\ntitle: Hi\ndate: 2026-04-19\n---\n\n# Hi\n",
    )
    .unwrap();
}

fn start_server(root: std::path::PathBuf) -> u16 {
    // Bind a listener just to grab an unused port, then drop it.
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    std::thread::spawn(move || {
        let _ = serve(ServeOptions {
            workspace: root,
            bind: "127.0.0.1".into(),
            port,
            open_browser: false,
        });
    });
    // Wait for bind.
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("server never came up on {port}");
}

fn get(port: u16, path: &str) -> (String, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).into_owned();
    (head, buf[split + 4..].to_vec())
}

#[test]
fn index_has_reload_script() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let (head, body) = get(port, "/");
    assert!(head.contains("200 OK"));
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("EventSource"), "missing reload script: {body_str}");
}

#[test]
fn sse_endpoint_returns_event_stream_headers() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    write!(
        s,
        "GET /__lopress/reload HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n"
    )
    .unwrap();
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).unwrap();
    let head = String::from_utf8_lossy(&buf[..n]);
    assert!(head.contains("text/event-stream"), "got: {head}");
    assert!(head.contains("retry: 1000"), "got: {head}");
}

#[test]
fn missing_path_returns_404_body() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let (head, _body) = get(port, "/not/found");
    assert!(head.contains("404"));
}
```

- [ ] **Step 2: Run, lint, commit**

```
cargo test -p lopress-serve --test serve_integration
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add crates/lopress-serve/tests
git commit -m "lopress-serve: integration tests for server"
```

Note: these tests start a real server thread and leak it (the accept loop never returns). That's fine for a test suite — the thread dies with the process. If this becomes a problem later, add a shutdown channel to `serve`.

---

## Task 15: `lopress serve` CLI subcommand

**Files:**
- Modify: `Cargo.toml` (root)
- Modify: `src/main.rs`

- [ ] **Step 1: Add `lopress-serve` to root dep**

In `Cargo.toml`:

```toml
[dependencies]
anyhow = { workspace = true }
clap = { workspace = true }
lopress-build = { path = "crates/lopress-build" }
lopress-serve = { path = "crates/lopress-serve" }
```

- [ ] **Step 2: Add the subcommand to `src/main.rs`**

In the `Command` enum, add:

```rust
/// Start a dev server with live reload.
Serve {
    /// Workspace directory.
    workspace: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
    #[arg(long, default_value_t = 8080)]
    port: u16,
    #[arg(long)]
    no_open: bool,
},
```

In the match, add:

```rust
Command::Serve { workspace, bind, port, no_open } => {
    lopress_serve::serve(lopress_serve::ServeOptions {
        workspace,
        bind,
        port,
        open_browser: !no_open,
    })?;
    Ok(())
}
```

- [ ] **Step 3: Smoke-test**

```
cargo run --quiet --bin lopress -- new /tmp/lopress-serve-smoke
# In another terminal, or background-then-kill:
cargo run --quiet --bin lopress -- serve /tmp/lopress-serve-smoke --port 0 --no-open &
SERVE_PID=$!
sleep 2
kill $SERVE_PID
rm -rf /tmp/lopress-serve-smoke
```

`--port 0` asks the OS for an ephemeral port. We don't check behavior here; we're just verifying `lopress serve` starts without crashing. If `--port 0` ever conflicts with anything, replace with any known-free port.

- [ ] **Step 4: Run full suite, lint, commit**

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "lopress: serve subcommand wired through CLI"
```

---

## Task 16: README update

Append a "Live preview" section describing `lopress serve`.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add section after the existing "Usage" section**

```markdown
## Live preview

While authoring, run:

```
./target/release/lopress serve my-site
```

This serves `my-site/www/` on `http://127.0.0.1:8080/`, watches the workspace, rebuilds incrementally on every write, and reloads open browser tabs via Server-Sent Events. Flags:

- `--port <n>` — bind port (default 8080).
- `--bind <addr>` — bind address (default `127.0.0.1`; use `0.0.0.0` to reach it from other devices on your LAN).
- `--no-open` — skip opening the default browser on startup.
```

- [ ] **Step 2: Update the status block**

Replace the phase-1 status sentence with:

```markdown
**Status: CLI works with live-reload dev server; GUI in progress.** `lopress build`, `lopress new`, and `lopress serve` are implemented. The egui-based block editor and webview preview are planned for a later phase. See [`docs/superpowers/specs/2026-04-18-lopress-design.md`](docs/superpowers/specs/2026-04-18-lopress-design.md) for the full design and [`docs/superpowers/plans/`](docs/superpowers/plans/) for implementation plans.
```

- [ ] **Step 3: Commit**

```
git add README.md
git commit -m "docs: README — phase 2 live-reload dev server"
```

---

## Verification checklist

At the end of phase 2, all of these should be true:

- [ ] `cargo test --workspace` passes (including the new watcher, incremental, and serve integration tests).
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `lopress serve <workspace>` serves the site and reloads the browser tab on a source edit.
- [ ] Editing one post re-renders only that post (`pages_rendered == 1`).
- [ ] Editing `lopress.toml`, the theme, or any plugin file triggers a full rebuild (`pages_skipped == 0`).
- [ ] Deleting a post removes its HTML from `www/` on the next build.
- [ ] The watcher ignores `www/`, `target/`, `.git/`, and editor swap files.
- [ ] `Cargo.lock` is committed.
- [ ] CI (Linux + macOS + Windows) passes.

When all boxes are ticked, phase 2 is complete. Phase 3 (GUI) remains.

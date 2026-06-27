# Navigation Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GUI panel to add/remove/reorder site nav links (label + href) with page and tag pickers, persisting to a machine-owned `nav.toml` (the only nav source) and rebuilding the live preview.

**Architecture:** Nav moves out of `lopress.toml` entirely: `Workspace` loads `nav.toml` (empty if absent), warns about leftover `[site.nav]` via `BuildReport.warnings`, and `write_nav` serializes with the existing `toml` crate + atomic write — zero new dependencies. `lopress-gui-host` exposes `Session::nav_items()`/`update_nav()` plus page slugs and tags for pickers; the editor adds a "Site settings" modal whose Save calls `update_nav` → rebuild.

**Tech Stack:** Rust, `toml` 0.8 serialization, Floem 0.2 (modal + inputs), the workspace's strict clippy lints (AGENTS.md).

**Spec:** `docs/superpowers/specs/2026-06-12-nav-editor-design.md`

> **Gate:** run `bash scripts/check.sh` before declaring done.

## Scope

Scope check done by Claude: this spec is a single plan's worth of work (one feature,
three crates touched in a strict dependency order: lopress-build → lopress-gui-host →
lopress-editor). Do not re-litigate scope.

## Conventions

- Test framework: built-in cargo test (unit tests in `#[cfg(test)] mod tests`,
  integration tests under `crates/<crate>/tests/`). Run a focused test with
  `cargo test -p <crate> <filter>`.
- Full gate: `bash scripts/check.sh` (fmt + clippy with strict lints + all tests).
- Commit style: short imperative subject, optionally `feat(editor):`/`fix(build):`
  prefixes, body explaining why; end with
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- TDD: every task starts with a failing test where the change is testable.

---

## File Structure Map

| File | Change | Responsibility |
|------|--------|----------------|
| `crates/lopress-build/src/site.rs` | **Modify** | Remove `nav` from `Site`; add `nav: Nav` + `warnings: Vec<String>` to `Workspace`; add `write_nav()` function; detect leftover `[site.nav]` via raw `toml::Value` peek |
| `crates/lopress-build/src/build.rs` | **Modify** | `BuildReport` gains `warnings: Vec<String>`; `build()` reads `ws.nav.items` (not `ws.config.site.nav`); copies `ws.warnings` into report and logs to stderr |
| `crates/lopress-build/src/cache.rs` | **Modify** | `hash_config()` extends to hash `nav.toml` bytes when present |
| `crates/lopress-build/src/scaffold.rs` | **Modify** | `new_site()` writes `nav.toml` instead of `[site.nav]` in `lopress.toml` |
| `crates/lopress-build/tests/build_integration.rs` | **Modify** | Update fixtures that use nav to work with `nav.toml`; add tests for migration warning, empty nav, cache invalidation |
| `crates/lopress-gui-host/src/session.rs` | **Modify** | Add `slug: String` to `DocumentRef`; add `tags: Vec<String>` to `WorkspaceSummary`; add `nav_items()` and `update_nav()` methods; extend `scan_workspace`/`scan_dir` for slug+tags |
| `crates/lopress-editor/src/ui/nav_editor.rs` | **Create** | New module: nav-editor panel with working model (`NavRow`, add/remove/reorder, page/tag picker integration) |
| `crates/lopress-editor/src/ui/mod.rs` | **Modify** | Add `nav_editor` module; add `nav_editor_open` signal; wire modal overlay in `editing_view` |
| `crates/lopress-editor/src/ui/sidebar.rs` | **Modify** | Add "Site settings" gear button in sidebar header |

## Task Decomposition

### Task 1: `write_nav` — TOML serialization + atomic write (with tests)

**Files:**
- Modify: `crates/lopress-build/src/site.rs`

**Goal:** A standalone function that serializes `Vec<NavItem>` to `nav.toml` at the workspace root, dropping rows with empty label or href. Uses the existing `toml` crate (0.8) and an atomic write pattern (temp file + rename).

**Steps:**

- [ ] **Step 1: Write the failing test** — append to `site.rs` `mod tests`:

```rust
    #[test]
    fn write_nav_creates_nav_toml() {
        let d = TempDir::new().unwrap();
        let items = vec![
            NavItem { label: "Home".into(), href: "/".into() },
            NavItem { label: "About".into(), href: "/about/".into() },
        ];
        write_nav(d.path(), &items).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("items"));
        assert!(content.contains("Home"));
        assert!(content.contains("/about/"));
    }

    #[test]
    fn write_nav_drops_empty_rows() {
        let d = TempDir::new().unwrap();
        let items = vec![
            NavItem { label: "Home".into(), href: "/".into() },
            NavItem { label: "".into(), href: "/empty/".into() },
            NavItem { label: "X".into(), href: "".into() },
        ];
        write_nav(d.path(), &items).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("Home"));
        assert!(!content.contains("/empty/"));
        assert!(!content.contains("X"));
    }

    #[test]
    fn write_nav_empty_items_writes_empty_array() {
        let d = TempDir::new().unwrap();
        write_nav(d.path(), &[]).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("items = []"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build write_nav`
Expected: FAIL (function does not exist yet).

- [ ] **Step 3: Implement `write_nav`** — add to `site.rs` after the `Workspace` impl:

```rust
/// Serialize `items` to TOML and write atomically to `nav.toml` at `root`.
///
/// Items with an empty `label` or empty `href` are dropped before writing.
/// An empty `items` list writes `items = []`.
pub fn write_nav(root: &Path, items: &[NavItem]) -> Result<(), BuildError> {
    // Drop rows with empty label or href.
    let filtered: Vec<NavItem> = items
        .iter()
        .filter(|n| !n.label.is_empty() && !n.href.is_empty())
        .cloned()
        .collect();

    let nav = Nav { items: filtered };

    // `BuildError` has no `From<toml::ser::Error>` — map into Config.
    let serialized = toml::to_string(&nav)
        .map_err(|e| BuildError::Config(format!("nav.toml: {e}")))?;

    // Atomic write: temp file + rename.
    let tmp = root.join(".nav.toml.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, root.join("nav.toml"))?;
    Ok(())
}
```

Note: `toml::to_string` on `Nav { items: vec![] }` emits `items = []`; with items it
emits an `[[items]]` array-of-tables — both parse back into `Nav` fine. If the
empty-array assertion fails on formatting, special-case empty:
`if nav.items.is_empty() { "items = []\n".to_string() } else { ... }`.

- [ ] **Step 4: Export the new API from `crates/lopress-build/src/lib.rs`** — gui-host
will need `Nav`, `NavItem`, and `write_nav`:

**Before:**
```rust
pub use site::{SiteConfig, Workspace};
```

**After:**
```rust
pub use site::{write_nav, Nav, NavItem, SiteConfig, Workspace};
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-build write_nav`
Expected: PASS (all three tests).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/site.rs crates/lopress-build/src/lib.rs
git commit -m "feat(build): add write_nav for atomic TOML serialization to nav.toml"
```

---

### Task 2: `Workspace` — remove `nav` from `Site`, add `nav` + `warnings` fields, migration warning

**Files:**
- Modify: `crates/lopress-build/src/site.rs`

**Goal:** Remove `nav: Nav` from `Site` struct. Add `nav: Nav` and `warnings: Vec<String>` to `Workspace`. During `load`, peek at the raw parsed `toml::Value` of `lopress.toml` to detect a leftover `[site.nav]` table. If found, push a migration warning string.

**Steps:**

- [ ] **Step 1: Write the failing test** — append to `site.rs` `mod tests`:

```rust
    #[test]
    fn workspace_loads_nav_from_nav_toml() {
        let d = TempDir::new().unwrap();
        // Write minimal config.
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();
        // Write nav.toml.
        write_nav(d.path(), &[
            NavItem { label: "Home".into(), href: "/".into() },
            NavItem { label: "About".into(), href: "/about/".into() },
        ]).unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert_eq!(ws.nav.items.len(), 2);
        assert_eq!(ws.nav.items[0].label, "Home");
        assert_eq!(ws.nav.items[1].href, "/about/");
        assert!(ws.warnings.is_empty());
    }

    #[test]
    fn workspace_has_empty_nav_when_no_nav_toml() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert!(ws.nav.items.is_empty());
        assert!(ws.warnings.is_empty());
    }

    #[test]
    fn workspace_warns_on_leftover_site_nav() {
        let d = TempDir::new().unwrap();
        // Write config WITH [site.nav] — this is the legacy format.
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Old", href = "/old/" }]
"#,
        )
        .unwrap();
        // No nav.toml — the old block should trigger a warning.

        let ws = Workspace::load(d.path()).unwrap();
        assert!(ws.nav.items.is_empty()); // nav.toml doesn't exist
        assert!(!ws.warnings.is_empty());
        assert!(ws.warnings[0].contains("[site.nav]"));
        assert!(ws.warnings[0].contains("ignored"));
    }

    #[test]
    fn workspace_warns_even_when_nav_toml_exists() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Old", href = "/old/" }]
"#,
        )
        .unwrap();
        write_nav(d.path(), &[
            NavItem { label: "New".into(), href: "/new/".into() },
        ]).unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert_eq!(ws.nav.items.len(), 1);
        assert_eq!(ws.nav.items[0].label, "New");
        // Warning still fires because [site.nav] is present in lopress.toml.
        assert!(!ws.warnings.is_empty());
        assert!(ws.warnings[0].contains("[site.nav]"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build workspace_loads_nav`
Expected: FAIL (Workspace struct doesn't have `nav` or `warnings` fields).

- [ ] **Step 3: Modify `Site` struct** — remove the `nav` field:

**Before:**
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Site {
    pub title: String,
    pub base_url: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub nav: Nav,
    #[serde(default)]
    pub og_image: Option<String>,
}
```

**After:**
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Site {
    pub title: String,
    pub base_url: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub og_image: Option<String>,
}
```

- [ ] **Step 4: Modify `Workspace` struct and `load` method:**

**Before:**
```rust
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub config: SiteConfig,
}

impl Workspace {
    pub fn load(root: &Path) -> Result<Self, BuildError> {
        let config_path = root.join("lopress.toml");
        if !config_path.exists() {
            return Err(BuildError::Config(format!(
                "no lopress.toml at {}",
                config_path.display()
            )));
        }
        let src = std::fs::read_to_string(&config_path)?;
        let config: SiteConfig = toml::from_str(&src)?;
        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }
```

**After:**
```rust
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub config: SiteConfig,
    pub nav: Nav,
    pub warnings: Vec<String>,
}

impl Workspace {
    pub fn load(root: &Path) -> Result<Self, BuildError> {
        let config_path = root.join("lopress.toml");
        if !config_path.exists() {
            return Err(BuildError::Config(format!(
                "no lopress.toml at {}",
                config_path.display()
            )));
        }
        let src = std::fs::read_to_string(&config_path)?;
        let config: SiteConfig = toml::from_str(&src)?;

        // Load nav from nav.toml (empty if absent).
        let nav_path = root.join("nav.toml");
        let nav = if nav_path.exists() {
            let nav_src = std::fs::read_to_string(&nav_path)?;
            let nav: Nav = toml::from_str(&nav_src)?;
            nav
        } else {
            Nav::default()
        };

        // Detect leftover [site.nav] in lopress.toml via raw toml::Value peek.
        let raw_value: toml::Value = toml::from_str(&src)?;
        let mut warnings = Vec::new();
        if let Some(site) = raw_value.get("site").and_then(|v| v.as_table()) {
            if site.contains_key("nav") {
                warnings.push(
                    "[site.nav] in lopress.toml is no longer supported and is ignored — move the items to nav.toml and delete the old block.".into(),
                );
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            config,
            nav,
            warnings,
        })
    }
```

- [ ] **Step 5: Fix the one consumer of `Site.nav`** — removing the field breaks
`build.rs`, which reads it (multi-line) when assembling `SiteCtx`. Switch it to
`ws.nav` now so the crate compiles (the `warnings` plumbing stays in Task 3):

**Before (`crates/lopress-build/src/build.rs`):**
```rust
        nav: ws
            .config
            .site
            .nav
            .items
            .iter()
            .map(|n| lopress_theme::NavItem {
                label: n.label.clone(),
                href: n.href.clone(),
            })
            .collect(),
```

**After:**
```rust
        nav: ws
            .nav
            .items
            .iter()
            .map(|n| lopress_theme::NavItem {
                label: n.label.clone(),
                href: n.href.clone(),
            })
            .collect(),
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p lopress-build workspace`
Expected: PASS (all four new tests). Note: existing integration tests that rely on
fixture `[site.nav]` nav links may now fail (nav is empty until Task 6 migrates the
fixtures) — check which assertions break and, if any do, do Task 6's fixture
migration as part of this commit instead of waiting.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-build/src/site.rs crates/lopress-build/src/build.rs
git commit -m "fix(build): move nav from Site to Workspace; add migration warning for leftover [site.nav]"
```

---

### Task 3: `BuildReport` — add `warnings` field; `build()` reads `ws.nav`, copies warnings

**Files:**
- Modify: `crates/lopress-build/src/build.rs`

**Goal:** `BuildReport` gains `warnings: Vec<String>`. The `build()` function switches from `ws.config.site.nav.items` to `ws.nav.items` for `SiteCtx`. It copies `ws.warnings` into the report and logs them to stderr.

**Steps:**

- [ ] **Step 1: Write the failing test** — append to `build.rs` `mod tests`:

```rust
    #[test]
    fn build_report_contains_warnings_from_workspace() {
        let d = TempDir::new().unwrap();
        // Scaffold a site with [site.nav] in lopress.toml (triggers warning).
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "W"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Old", href = "/old/" }]
"#,
        )
        .unwrap();
        for sub in ["src/posts", "src/pages", "src/images", "plugins"] {
            std::fs::create_dir_all(d.path().join(sub)).unwrap();
        }

        let report = build(d.path()).unwrap();
        assert!(!report.warnings.is_empty());
        assert!(report.warnings[0].contains("[site.nav]"));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-build build_report_contains_warnings`
Expected: FAIL (`warnings` field does not exist on `BuildReport`).

- [ ] **Step 3: Add `warnings` to `BuildReport` struct:**

**Before:**
```rust
pub struct BuildReport {
    pub pages_written: usize,
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
}
```

**After:**
```rust
pub struct BuildReport {
    pub pages_written: usize,
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
    pub warnings: Vec<String>,
}
```

- [ ] **Step 4: Add stderr logging for warnings in `build()`** — right after `Workspace::load`:

```rust
    let ws = Workspace::load(workspace)?;

    // Log migration warnings to stderr.
    for warning in &ws.warnings {
        eprintln!("warning: {warning}");
    }
```

(The `SiteCtx.nav` switch to `ws.nav` already happened in Task 2 — `build()` compiles.)

- [ ] **Step 5: Copy `ws.warnings` into `BuildReport` at the end of `build()`:**

**Before:**
```rust
    Ok(BuildReport {
        pages_written,
        pages_rendered: stats.pages_rendered,
        pages_skipped: stats.pages_skipped,
        failures,
    })
```

**After:**
```rust
    Ok(BuildReport {
        pages_written,
        pages_rendered: stats.pages_rendered,
        pages_skipped: stats.pages_skipped,
        failures,
        warnings: ws.warnings,
    })
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p lopress-build build_report_contains_warnings`
Expected: PASS. (The test needs `use tempfile::TempDir;` in `build.rs`'s test
module — add it; `tempfile` is already a dev-dependency.)

- [ ] **Step 7: Run the full build test suite to confirm nothing broke**

Run: `cargo test -p lopress-build`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-build/src/build.rs
git commit -m "fix(build): BuildReport.warnings; build() reads ws.nav and logs migration warnings"
```

---

### Task 4: `cache::hash_config` — extend to hash `nav.toml` bytes

**Files:**
- Modify: `crates/lopress-build/src/cache.rs`

**Goal:** `hash_config` currently hashes only `lopress.toml` bytes. Extend it to also hash `nav.toml` bytes when present (with a separator key so presence/absence changes the hash).

**Steps:**

- [ ] **Step 1: Write the failing test** — append to `cache.rs` `mod tests`:

```rust
    #[test]
    fn hash_config_changes_with_nav_toml() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        let ws = crate::site::Workspace::load(d.path()).unwrap();
        let h1 = hash_config(&ws).unwrap();

        // Now write nav.toml — hash should change.
        write_nav(d.path(), &[NavItem {
            label: "Home".into(),
            href: "/".into(),
        }]).unwrap();
        let ws2 = crate::site::Workspace::load(d.path()).unwrap();
        let h2 = hash_config(&ws2).unwrap();
        assert_ne!(h1, h2, "hash should change when nav.toml is added");

        // Changing nav.toml content changes the hash.
        write_nav(d.path(), &[NavItem {
            label: "About".into(),
            href: "/about/".into(),
        }]).unwrap();
        let ws3 = crate::site::Workspace::load(d.path()).unwrap();
        let h3 = hash_config(&ws3).unwrap();
        assert_ne!(h2, h3, "hash should change when nav.toml content changes");

        // Deleting nav.toml changes the hash back.
        std::fs::remove_file(d.path().join("nav.toml")).unwrap();
        let ws4 = crate::site::Workspace::load(d.path()).unwrap();
        let h4 = hash_config(&ws4).unwrap();
        assert_ne!(h3, h4, "hash should change when nav.toml is deleted");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-build hash_config_changes_with_nav_toml`
Expected: FAIL (current `hash_config` only hashes `lopress.toml`).

- [ ] **Step 3: Modify `hash_config`** — extend to include `nav.toml`:

**Before:**
```rust
pub fn hash_config(workspace: &Workspace) -> Result<String, BuildError> {
    let bytes = std::fs::read(workspace.root.join("lopress.toml"))?;
    Ok(hash_bytes(&bytes))
}
```

**After:**
```rust
pub fn hash_config(workspace: &Workspace) -> Result<String, BuildError> {
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();

    let lpress_bytes = std::fs::read(workspace.root.join("lopress.toml"))?;
    items.push(("lopress.toml".into(), lpress_bytes));

    let nav_path = workspace.root.join("nav.toml");
    if nav_path.exists() {
        let nav_bytes = std::fs::read(&nav_path)?;
        items.push(("nav.toml".into(), nav_bytes));
    }

    Ok(hash_many(&mut items))
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-build hash_config_changes_with_nav_toml`
Expected: PASS.

- [ ] **Step 5: Run full build test suite**

Run: `cargo test -p lopress-build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/cache.rs
git commit -m "fix(build): hash_config includes nav.toml for cache invalidation"
```

---

### Task 5: `scaffold.rs` — write `nav.toml` instead of `[site.nav]`

**Files:**
- Modify: `crates/lopress-build/src/scaffold.rs`

**Goal:** `new_site()` writes `nav.toml` with default Home/About nav instead of embedding `[site.nav]` in `lopress.toml`. The generated `lopress.toml` has no `[site.nav]` block.

**Steps:**

- [ ] **Step 1: Write the failing test** — append to `scaffold_tests.rs`:

```rust
#[test]
fn new_site_writes_nav_toml_with_default_items() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("s");

    new_site(&dir, "T", "http://localhost:8080").unwrap();

    let nav_path = dir.join("nav.toml");
    assert!(nav_path.exists(), "nav.toml must be created");
    let content = std::fs::read_to_string(&nav_path).unwrap();
    assert!(content.contains("Home"));
    assert!(content.contains("/"));
    assert!(content.contains("About"));
    assert!(content.contains("/about/"));
}

#[test]
fn new_site_does_not_write_site_nav_in_lopress_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("s");

    new_site(&dir, "T", "http://localhost:8080").unwrap();

    let toml_content = std::fs::read_to_string(dir.join("lopress.toml")).unwrap();
    assert!(
        !toml_content.contains("[site.nav]"),
        "lopress.toml must not contain [site.nav]"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build new_site_writes_nav_toml`
Expected: FAIL (`nav.toml` does not exist).

- [ ] **Step 3: Modify `new_site()`** — replace the `[site.nav]` block in `lopress.toml` and add `nav.toml` write:

**Before:**
```rust
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
```

**After:**
```rust
    std::fs::write(
        dir.join("lopress.toml"),
        format!(
            r#"[site]
title = "{title}"
base_url = "{base_url}"
"#
        ),
    )?;

    // Write default nav to nav.toml.
    std::fs::write(
        dir.join("nav.toml"),
        "items = [\n  { label = \"Home\", href = \"/\" },\n  { label = \"About\", href = \"/about/\" },\n]\n",
    )?;
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-build new_site_writes_nav_toml`
Expected: PASS.

- [ ] **Step 5: Run full build test suite**

Run: `cargo test -p lopress-build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/scaffold.rs crates/lopress-build/tests/scaffold_tests.rs
git commit -m "fix(build): scaffold writes nav.toml instead of [site.nav] in lopress.toml"
```

---

### Task 6: Update integration test fixtures — migrate nav to `nav.toml`

**Files:**
- Modify: `crates/lopress-build/tests/fixtures/minimal/lopress.toml`
- Create: `crates/lopress-build/tests/fixtures/minimal/nav.toml`
- Modify: `crates/lopress-build/tests/fixtures/with-plugin-theme/lopress.toml`
- Create: `crates/lopress-build/tests/fixtures/with-plugin-theme/nav.toml`

**Goal:** Fixtures that currently have `[site.nav]` in `lopress.toml` need `nav.toml` so they build without the migration warning (which would clutter test output). The `with-plugin` fixture has no nav and needs no change.

**Steps:**

- [ ] **Step 1: Migrate `minimal` fixture**

Create `crates/lopress-build/tests/fixtures/minimal/nav.toml`:
```toml
items = [
  { label = "Home", href = "/" },
  { label = "About", href = "/about/" },
]
```

Modify `crates/lopress-build/tests/fixtures/minimal/lopress.toml`:

**Before:**
```toml
[site]
title = "Test Site"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Home", href = "/" }, { label = "About", href = "/about/" }]
```

**After:**
```toml
[site]
title = "Test Site"
base_url = "https://example.com"
```

- [ ] **Step 2: Migrate `with-plugin-theme` fixture**

Create `crates/lopress-build/tests/fixtures/with-plugin-theme/nav.toml`:
```toml
items = [
  { label = "Home", href = "/" },
  { label = "About", href = "/about/" },
]
```

Modify `crates/lopress-build/tests/fixtures/with-plugin-theme/lopress.toml`:

**Before:**
```toml
[site]
title = "Plugin Theme Site"
base_url = "https://example.com"
theme = "custom-theme"

[site.nav]
items = [{ label = "Home", href = "/" }, { label = "About", href = "/about/" }]
```

**After:**
```toml
[site]
title = "Plugin Theme Site"
base_url = "https://example.com"
theme = "custom-theme"
```

- [ ] **Step 3: Run full build test suite**

Run: `cargo test -p lopress-build`
Expected: PASS (all integration tests still pass).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/tests/fixtures/minimal/lopress.toml crates/lopress-build/tests/fixtures/minimal/nav.toml crates/lopress-build/tests/fixtures/with-plugin-theme/lopress.toml crates/lopress-build/tests/fixtures/with-plugin-theme/nav.toml
git commit -m "test(build): migrate fixtures to nav.toml, remove [site.nav] from lopress.toml"
```

---

### Task 7: `DocumentRef` gains `slug`; `WorkspaceSummary` gains `tags`; scan changes

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Test: `crates/lopress-gui-host/tests/session_integration.rs`

**Goal:** `DocumentRef` gains a `slug: String` field (computed from front-matter `slug` or file stem). `WorkspaceSummary` gains `tags: Vec<String>` (sorted, de-duplicated union of post front-matter tags). The `scan_workspace` function populates both.

**Steps:**

- [ ] **Step 1: Write the failing test** — append to
`crates/lopress-gui-host/tests/session_integration.rs` (it already has a
`make_workspace()` helper that hand-rolls a workspace `Session::open` accepts):

```rust
#[test]
fn workspace_summary_has_slugs_and_tags() {
    let dir = make_workspace();
    let p = dir.path();
    // Page with explicit front-matter slug.
    fs::write(
        p.join("src/pages/about.md"),
        "---\ntitle: About Me\nslug: about\n---\n\nHi.\n",
    )
    .unwrap();
    // Page without slug — the file stem is the slug.
    fs::write(
        p.join("src/pages/contact.md"),
        "---\ntitle: Contact\n---\n\nHi.\n",
    )
    .unwrap();
    // Posts with overlapping tags to prove sort + de-dup.
    fs::write(
        p.join("src/posts/tagged.md"),
        "---\ntitle: Tagged\ndate: 2026-04-21\ntags: [web, rust]\n---\n\nBody.\n",
    )
    .unwrap();
    fs::write(
        p.join("src/posts/tagged2.md"),
        "---\ntitle: Tagged Two\ndate: 2026-04-22\ntags: [rust]\n---\n\nBody.\n",
    )
    .unwrap();

    let session = Session::open(p).unwrap();
    let ws = session.workspace();

    let about = ws.pages.iter().find(|d| d.title == "About Me").unwrap();
    assert_eq!(about.slug, "about", "front-matter slug wins");
    let contact = ws.pages.iter().find(|d| d.title == "Contact").unwrap();
    assert_eq!(contact.slug, "contact", "file stem is the fallback slug");
    assert_eq!(ws.tags, vec!["rust".to_string(), "web".to_string()]);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-gui-host workspace_summary_has_slugs`
Expected: FAIL to compile (`DocumentRef` has no `slug`, `WorkspaceSummary` has no `tags`).

- [ ] **Step 3: Add `slug` to `DocumentRef` struct:**

**Before:**
```rust
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,
    pub title: String,
    pub is_draft: bool,
    pub has_parse_error: bool,
}
```

**After:**
```rust
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,
    pub title: String,
    pub slug: String,
    pub is_draft: bool,
    pub has_parse_error: bool,
}
```

- [ ] **Step 4: Add `tags` to `WorkspaceSummary` struct:**

**Before:**
```rust
#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,
    pub posts: Vec<DocumentRef>,
    pub pages: Vec<DocumentRef>,
}
```

**After:**
```rust
#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,
    pub posts: Vec<DocumentRef>,
    pub pages: Vec<DocumentRef>,
    pub tags: Vec<String>,
}
```

- [ ] **Step 5: Modify `scan_dir` to compute slug and pass it:**

**Before:**
```rust
fn scan_dir(dir: &Path) -> Vec<DocumentRef> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut refs: Vec<DocumentRef> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .map(|e| {
            let path = e.path();
            match std::fs::read_to_string(&path).as_deref().map(parse) {
                Ok(Ok(doc)) => DocumentRef {
                    title: doc.front_matter.title.unwrap_or_else(|| stem(&path)),
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
```

**After:**
```rust
fn scan_dir(dir: &Path) -> Vec<DocumentRef> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut refs: Vec<DocumentRef> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .map(|e| {
            let path = e.path();
            match std::fs::read_to_string(&path).as_deref().map(parse) {
                Ok(Ok(doc)) => {
                    let slug = doc.front_matter.slug.unwrap_or_else(|| {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("untitled")
                            .to_string()
                    });
                    DocumentRef {
                        title: doc.front_matter.title.unwrap_or_else(|| slug.clone()),
                        slug,
                        is_draft: doc.front_matter.draft,
                        has_parse_error: false,
                        path,
                    }
                }
                _ => DocumentRef {
                    title: stem(&path),
                    slug: stem(&path),
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
```

- [ ] **Step 6: Modify `scan_workspace` to collect tags:**

**Before:**
```rust
fn scan_workspace(ws: &Workspace) -> WorkspaceSummary {
    WorkspaceSummary {
        root: ws.root.clone(),
        name: ws.config.site.title.clone(),
        posts: scan_dir(&ws.posts_dir()),
        pages: scan_dir(&ws.pages_dir()),
    }
}
```

**After:**
```rust
fn scan_workspace(ws: &Workspace) -> WorkspaceSummary {
    let posts = scan_dir(&ws.posts_dir());
    let pages = scan_dir(&ws.pages_dir());

    // Collect tags from post front-matter (sorted, de-duplicated).
    let mut tags_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&ws.posts_dir()).ok().into_iter().flatten() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(&path) {
            if let Ok(doc) = parse(&src) {
                for tag in &doc.front_matter.tags {
                    tags_set.insert(tag.clone());
                }
            }
        }
    }
    let tags: Vec<String> = tags_set.into_iter().collect();

    WorkspaceSummary {
        root: ws.root.clone(),
        name: ws.config.site.title.clone(),
        posts,
        pages,
        tags,
    }
}
```

- [ ] **Step 7: Run to verify everything compiles and passes**

Run: `cargo test -p lopress-gui-host`
Expected: PASS.

- [ ] **Step 8: Run full build test suite**

Run: `cargo test -p lopress-build`
Expected: PASS (the build crate doesn't depend on gui-host types).

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/tests/session_integration.rs
git commit -m "feat(gui-host): add slug to DocumentRef, tags to WorkspaceSummary"
```

---

### Task 8: `Session::nav_items()` and `Session::update_nav()`

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Modify: `crates/lopress-gui-host/src/error.rs`
- Test: `crates/lopress-gui-host/tests/session_integration.rs`

**Goal:** Add two new methods to `Session`:
- `nav_items()` — re-reads `nav.toml` from disk and returns `Vec<NavItem>`
- `update_nav()` — writes `nav.toml` via `write_nav`, then triggers a rebuild + SSE broadcast

**Steps:**

- [ ] **Step 1: Write failing tests** — append to
`crates/lopress-gui-host/tests/session_integration.rs`:

```rust
#[test]
fn nav_items_reads_from_disk() {
    let dir = make_workspace();
    lopress_build::write_nav(
        dir.path(),
        &[lopress_build::NavItem {
            label: "Home".into(),
            href: "/".into(),
        }],
    )
    .unwrap();

    let session = Session::open(dir.path()).unwrap();
    let items = session.nav_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Home");
}

#[test]
fn update_nav_writes_nav_toml() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();

    session
        .update_nav(vec![lopress_build::NavItem {
            label: "New".into(),
            href: "/new/".into(),
        }])
        .unwrap();

    // The file was written and nav_items reflects it.
    assert!(dir.path().join("nav.toml").exists());
    let items = session.nav_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "New");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-gui-host nav_items_reads`
Expected: FAIL to compile (`nav_items` does not exist on `Session`).

- [ ] **Step 3: Add a `Build` variant to `SaveError`** — `write_nav` returns
`lopress_build::BuildError`, and `SaveError` only has `Io(#[from] std::io::Error)`.
In `crates/lopress-gui-host/src/error.rs`:

```rust
#[derive(Debug, Error)]
pub enum SaveError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("build: {0}")]
    Build(#[from] lopress_build::BuildError),
}
```

- [ ] **Step 4: Add `nav_items()` and `update_nav()` to `Session`:**

```rust
    /// Current nav items, read fresh from nav.toml on disk so repeated
    /// edits in one session reflect the latest saved state. Empty when
    /// the file doesn't exist or doesn't parse.
    pub fn nav_items(&self) -> Vec<lopress_build::NavItem> {
        let nav_path = self.workspace.root.join("nav.toml");
        let Ok(src) = std::fs::read_to_string(&nav_path) else {
            return Vec::new();
        };
        toml::from_str::<lopress_build::Nav>(&src)
            .map(|nav| nav.items)
            .unwrap_or_default()
    }

    /// Write nav items to nav.toml, then trigger a rebuild + SSE reload.
    ///
    /// # Errors
    /// Returns an error if nav.toml can't be serialized or written.
    pub fn update_nav(&self, items: Vec<lopress_build::NavItem>) -> Result<(), SaveError> {
        lopress_build::write_nav(&self.workspace.root, &items)?;
        self.rebuild();
        Ok(())
    }
```

Note: `toml` must be a dependency of `lopress-gui-host` for the `from_str` call —
check its `Cargo.toml`; if absent, add `toml = { workspace = true }`.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-gui-host nav_items`
Expected: PASS.

- [ ] **Step 6: Run full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/src/error.rs crates/lopress-gui-host/tests/session_integration.rs crates/lopress-gui-host/Cargo.toml
git commit -m "feat(gui-host): add nav_items() and update_nav() to Session"
```

---

### Task 9: Editor — `nav_editor.rs` panel with working model

**Files:**
- Create: `crates/lopress-editor/src/ui/nav_editor.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs` (add `pub mod nav_editor;`)
- Modify: `crates/lopress-editor/src/ui/sidebar.rs` (add "Site settings" entry point)

**Goal:** Create the nav-editor panel module with a pure working model (independent of Floem views) and the Floem UI. The working model is a `Vec<NavRow>` where each row has `label` and `href` strings. Operations: add row, remove row, reorder up/down, update label/href.

**Steps:**

- [ ] **Step 1: Write the working model tests** — in the new `nav_editor.rs` file, add tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_row_appends_empty_row() {
        let mut model = NavModel::new(vec![]);
        model.add_row();
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.rows[0].label, "");
        assert_eq!(model.rows[0].href, "");
    }

    #[test]
    fn remove_row_at_index() {
        let mut model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "B".into(), href: "/b/".into() },
        ]);
        model.remove_row(0);
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.rows[0].label, "B");
    }

    #[test]
    fn move_up_at_index_1_moves_to_0() {
        let mut model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "B".into(), href: "/b/".into() },
            NavRow { label: "C".into(), href: "/c/".into() },
        ]);
        model.move_up(2); // C moves up
        assert_eq!(model.rows[1].label, "C");
        assert_eq!(model.rows[2].label, "B");
    }

    #[test]
    fn move_up_at_index_0_does_nothing() {
        let mut model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "B".into(), href: "/b/".into() },
        ]);
        model.move_up(0);
        assert_eq!(model.rows[0].label, "A");
    }

    #[test]
    fn move_down_at_index_0_moves_to_1() {
        let mut model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "B".into(), href: "/b/".into() },
        ]);
        model.move_down(0);
        assert_eq!(model.rows[1].label, "A");
    }

    #[test]
    fn move_down_at_last_index_does_nothing() {
        let mut model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "B".into(), href: "/b/".into() },
        ]);
        model.move_down(1);
        assert_eq!(model.rows[1].label, "B");
    }

    #[test]
    fn to_nav_items_drops_empty_rows() {
        let model = NavModel::new(vec![
            NavRow { label: "A".into(), href: "/a/".into() },
            NavRow { label: "".into(), href: "/empty/".into() },
            NavRow { label: "B".into(), href: "".into() },
        ]);
        let items = model.to_nav_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "A");
    }

    #[test]
    fn fill_href_from_page() {
        let mut model = NavModel::new(vec![
            NavRow { label: "".into(), href: "".into() },
        ]);
        model.fill_href_from_page(0, "about", "About Page");
        assert_eq!(model.rows[0].href, "/about/");
        assert_eq!(model.rows[0].label, "About Page"); // pre-filled because label was empty
    }

    #[test]
    fn fill_href_from_tag() {
        let mut model = NavModel::new(vec![
            NavRow { label: "".into(), href: "".into() },
        ]);
        model.fill_href_from_tag(0, "rust");
        assert_eq!(model.rows[0].href, "/tags/rust/");
        assert_eq!(model.rows[0].label, "rust"); // pre-filled because label was empty
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor nav_editor`
Expected: FAIL (module does not exist yet).

- [ ] **Step 3: Create `crates/lopress-editor/src/ui/nav_editor.rs`:**

```rust
//! Navigation editor panel — working model and Floem view.
//!
//! The working model (`NavModel`) is a pure data structure independent of
//! Floem views. It manages a list of `NavRow` items with add/remove/reorder
//! operations. The Floem view (`nav_editor_view`) binds the model to inputs.

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{button, h_stack, label, text_input, v_stack, Decorators};
use floem::{AnyView, IntoView};
use lopress_build::NavItem;
use std::rc::Rc;

// ── Working model (pure data, testable without Floem) ───────────────────────

/// A single nav row in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavRow {
    pub label: String,
    pub href: String,
}

/// The pure working model for the nav editor panel.
///
/// This is the single source of truth for the panel's state. It is
/// initialized from `session.nav_items()` when the modal opens and is
/// used to build `Vec<NavItem>` on save.
#[derive(Debug, Clone)]
pub struct NavModel {
    pub rows: Vec<NavRow>,
}

impl NavModel {
    /// Create a new model from the current nav items.
    pub fn new(items: Vec<NavItem>) -> Self {
        Self {
            rows: items
                .into_iter()
                .map(|n| NavRow {
                    label: n.label,
                    href: n.href,
                })
                .collect(),
        }
    }

    /// Add an empty row at the end.
    pub fn add_row(&mut self) {
        self.rows.push(NavRow {
            label: String::new(),
            href: String::new(),
        });
    }

    /// Remove the row at the given index. Panics if out of bounds.
    pub fn remove_row(&mut self, index: usize) {
        self.rows.remove(index);
    }

    /// Move the row at `index` up by one position. No-op at index 0.
    pub fn move_up(&mut self, index: usize) {
        if index > 0 && index < self.rows.len() {
            self.rows.swap(index, index - 1);
        }
    }

    /// Move the row at `index` down by one position. No-op at the last index.
    pub fn move_down(&mut self, index: usize) {
        if index + 1 < self.rows.len() {
            self.rows.swap(index, index + 1);
        }
    }

    /// Update the label of the row at `index`. Panics if out of bounds.
    pub fn set_label(&mut self, index: usize, label: String) {
        self.rows[index].label = label;
    }

    /// Update the href of the row at `index`. Panics if out of bounds.
    pub fn set_href(&mut self, index: usize, href: String) {
        self.rows[index].href = href;
    }

    /// Fill the href (and label if empty) from a page slug.
    pub fn fill_href_from_page(&mut self, index: usize, slug: &str, title: &str) {
        if index < self.rows.len() {
            self.rows[index].href = format!("/{slug}/");
            if self.rows[index].label.is_empty() {
                self.rows[index].label = title.to_string();
            }
        }
    }

    /// Fill the href (and label if empty) from a tag name.
    pub fn fill_href_from_tag(&mut self, index: usize, tag: &str) {
        if index < self.rows.len() {
            self.rows[index].href = format!("/tags/{tag}/");
            if self.rows[index].label.is_empty() {
                self.rows[index].label = tag.to_string();
            }
        }
    }

    /// Convert the current rows to `NavItem`, dropping empty rows.
    pub fn to_nav_items(&self) -> Vec<NavItem> {
        self.rows
            .iter()
            .filter(|r| !r.label.is_empty() && !r.href.is_empty())
            .map(|r| NavItem {
                label: r.label.clone(),
                href: r.href.clone(),
            })
            .collect()
    }
}

// ── Floem view ──────────────────────────────────────────────────────────────

/// A single nav row view (label input + href input + controls).
fn nav_row_view(
    row: &NavRow,
    index: usize,
    total: usize,
    on_label: impl Fn(String) + 'static,
    on_href: impl Fn(String) + 'static,
    on_remove: impl Fn() + 'static,
    on_move_up: impl Fn() + 'static,
    on_move_down: impl Fn() + 'static,
) -> AnyView {
    let label_buf: RwSignal<String> = RwSignal::new(row.label.clone());
    let href_buf: RwSignal<String> = RwSignal::new(row.href.clone());

    let on_label_for_commit = on_label;
    let label_input = text_input(label_buf)
        .label(|| "Label".to_string())
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            on_label_for_commit(label_buf.get_untracked());
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.min_width(120.).font_size(12.));

    let on_href_for_commit = on_href;
    let href_input = text_input(href_buf)
        .label(|| "Href".to_string())
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            on_href_for_commit(href_buf.get_untracked());
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.min_width(120.).font_size(12.));

    let up_btn = button(label(|| "↑".to_string()))
        .action(move || (on_move_up)())
        .disabled(move || index == 0)
        .style(|s| s.padding(4.).font_size(12.));

    let down_btn = button(label(|| "↓".to_string()))
        .action(move || (on_move_down)())
        .disabled(move || index + 1 >= total)
        .style(|s| s.padding(4.).font_size(12.));

    let remove_btn = button(label(|| "✕".to_string()))
        .action(move || (on_remove)())
        .style(|s| s.padding(4.).font_size(12.).color(Color::rgb8(200, 60, 60)));

    let controls = h_stack((up_btn, down_btn, remove_btn))
        .style(|s| s.gap(2.).items_center());

    let row_stack = v_stack((label_input, href_input, controls))
        .style(|s| s.gap(4.).padding(6.).border(1.).border_color(Color::rgb8(220, 220, 220)).border_radius(4.));

    row_stack.into_any()
}

/// Build the nav-editor panel view.
///
/// `on_save` is called with the collected `Vec<NavItem>`; the caller decides
/// whether to close the modal (it stays open on a save error, which the
/// caller displays — see Task 10).
/// `on_cancel` closes the modal without saving.
/// (Task 11 extends this signature with page/tag picker data.)
pub fn nav_editor_view(
    model: RwSignal<NavModel>,
    on_save: impl Fn(Vec<NavItem>) + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl IntoView {
    let on_save_for_btn = on_save;
    let save_btn = button(label(|| "Save".to_string()))
        .action(move || {
            let m = model.get_untracked();
            let items = m.to_nav_items();
            on_save_for_btn(items);
        })
        .style(|s| s.padding_horiz(16.).padding_vert(6.).font_size(14.).font_weight(Weight::SEMIBOLD));

    let on_cancel_for_btn = on_cancel;
    let cancel_btn = button(label(|| "Cancel".to_string()))
        .action(move || (on_cancel_for_btn)())
        .style(|s| s.padding_horiz(16.).padding_vert(6.).font_size(14.));

    let on_add = model;
    let add_btn = button(label(|| "+ Add link".to_string()))
        .action(move || {
            on_add.update(|m| m.add_row());
        })
        .style(|s| s.padding_vert(4.).font_size(12.));

    let footer = h_stack((save_btn, cancel_btn))
        .style(|s| s.gap(8.).justify_end().padding_top(8.));

    v_stack((
        add_btn,
        floem::views::scroll(
            dyn_container(
                move || model.get(),
                move |m| {
                    let mut rows: Vec<AnyView> = Vec::with_capacity(m.rows.len());
                    for (i, row) in m.rows.iter().enumerate() {
                        let idx = i;
                        let total = m.rows.len();
                        let on_label = {
                            let model = model;
                            move |label: String| {
                                model.update(|m| m.set_label(idx, label));
                            }
                        };
                        let on_href = {
                            let model = model;
                            move |href: String| {
                                model.update(|m| m.set_href(idx, href));
                            }
                        };
                        let on_remove = {
                            let model = model;
                            move || {
                                model.update(|m| m.remove_row(idx));
                            }
                        };
                        let on_move_up = {
                            let model = model;
                            move || {
                                model.update(|m| m.move_up(idx));
                            }
                        };
                        let on_move_down = {
                            let model = model;
                            move || {
                                model.update(|m| m.move_down(idx));
                            }
                        };
                        rows.push(nav_row_view(
                            row, idx, total, on_label, on_href, on_remove, on_move_up, on_move_down,
                        ));
                    }
                    v_stack_from_iter(rows).style(|s| s.gap(4.))
                },
            )
            .style(|s| s.min_height(150.).max_height(300.))
        ),
        footer,
    ))
    .style(|s| s.gap(8.).padding(16.).width(480.))
}
```

- [ ] **Step 4: Register the module in `mod.rs`**

Add `pub mod nav_editor;` to the module list in `crates/lopress-editor/src/ui/mod.rs`.

- [ ] **Step 5: Run to verify tests pass**

Run: `cargo test -p lopress-editor nav_editor`
Expected: PASS (all working model tests).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/nav_editor.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): add nav_editor.rs panel with working model"
```

---

### Task 10: Sidebar "Site settings" entry point + modal wiring in `mod.rs`

**Files:**
- Modify: `crates/lopress-editor/src/ui/sidebar.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

**Goal:** Add a "Site settings" gear button to the sidebar header. Wire the modal overlay in `editing_view` using a `RwSignal<bool>` (`nav_editor_open`).

**Steps:**

- [ ] **Step 1: Add "Site settings" button to sidebar** — the `nav_editor_open`
signal is owned by `editing_view` in `mod.rs` (Step 2); the sidebar only gets a
plain callback parameter.

**Modify `sidebar_view` signature in `sidebar.rs`:**

**Before:**
```rust
pub fn sidebar_view(
    workspace: RwSignal<WorkspaceSummary>,
    current_path: RwSignal<Option<PathBuf>>,
    on_open: Rc<dyn Fn(DocumentRef)>,
    on_new_post: Rc<dyn Fn()>,
    on_new_page: Rc<dyn Fn()>,
) -> impl IntoView {
```

**After:**
```rust
pub fn sidebar_view(
    workspace: RwSignal<WorkspaceSummary>,
    current_path: RwSignal<Option<PathBuf>>,
    on_open: Rc<dyn Fn(DocumentRef)>,
    on_new_post: Rc<dyn Fn()>,
    on_new_page: Rc<dyn Fn()>,
    on_site_settings: Rc<dyn Fn()>,
) -> impl IntoView {
```

**Add the gear button in `sidebar_view`** — after the `lists` variable, before `footer`:

```rust
    let on_site_settings_for_btn = on_site_settings;
    let site_settings_btn = button(label(|| "⚙ Site settings".to_string()))
        .action(move || on_site_settings_for_btn())
        .style(|s| s.width_full().padding_vert(4.).padding_horiz(8.).font_size(12.));
```

Append `site_settings_btn` to the sidebar's existing footer stack — match how the
new-post/new-page buttons are grouped in the current `sidebar.rs` rather than
restructuring it.

**Update the call site in `mod.rs`:**

**Before:**
```rust
    let sidebar = sidebar_view(
        workspace_signal,
        current_path,
        on_open,
        on_new_post,
        on_new_page,
    );
```

**After:**
```rust
    let nav_editor_open: RwSignal<bool> = RwSignal::new(false);

    let on_site_settings = Rc::new(move || {
        nav_editor_open.set(true);
    });

    let sidebar = sidebar_view(
        workspace_signal,
        current_path,
        on_open,
        on_new_post,
        on_new_page,
        on_site_settings.clone(),
    );
```

- [ ] **Step 2: Add modal overlay to `editing_view`** in `mod.rs`. `editing` is the
existing `Rc<RefCell<Option<EditingState>>>` (the session lives at
`state.session`). All signals are `Copy`, so closures capturing them are `Clone`:

```rust
    // ── Nav editor modal ─────────────────────────────────────────────────
    let nav_editor_open: RwSignal<bool> = RwSignal::new(false);
    let nav_model: RwSignal<NavModel> = RwSignal::new(NavModel::new(Vec::new()));
    let nav_save_error: RwSignal<Option<String>> = RwSignal::new(None);

    let editing_for_nav_save = Rc::clone(&editing);
    let on_nav_save = move |items: Vec<NavItem>| {
        let borrowed = editing_for_nav_save.borrow();
        let Some(state) = borrowed.as_ref() else {
            return;
        };
        match state.session.update_nav(items) {
            Ok(()) => {
                nav_save_error.set(None);
                nav_editor_open.set(false);
            }
            // Error: surface inline, keep the modal open.
            Err(e) => nav_save_error.set(Some(e.to_string())),
        }
    };
    let on_nav_cancel = move || nav_editor_open.set(false);

    let editing_for_modal = Rc::clone(&editing);
    let modal = dyn_container(
        move || nav_editor_open.get(),
        move |open| {
            if !open {
                return empty().into_any();
            }
            // (Re)initialize the working copy from disk on every open.
            if let Some(state) = editing_for_modal.borrow().as_ref() {
                nav_model.set(NavModel::new(state.session.nav_items()));
            }
            nav_save_error.set(None);

            let error_line = dyn_container(
                move || nav_save_error.get(),
                move |err| match err {
                    Some(e) => label(move || e.clone())
                        .style(|s| s.color(Color::rgb8(200, 60, 60)).font_size(12.))
                        .into_any(),
                    None => empty().into_any(),
                },
            );

            let panel = v_stack((
                label(|| "Site settings — navigation".to_string())
                    .style(|s| s.font_size(14.).font_weight(Weight::SEMIBOLD)),
                error_line,
                nav_editor::nav_editor_view(nav_model, on_nav_save.clone(), on_nav_cancel),
            ))
            .style(|s| {
                s.gap(8.)
                    .padding(16.)
                    .background(Color::WHITE)
                    .border_radius(8.)
            });

            // Modal overlay: dimmed backdrop + centered panel. Per the floem
            // overlay caveat: keep all insets non-negative or the panel's
            // buttons won't be hit-tested.
            stack((
                empty()
                    .style(|s| {
                        s.absolute()
                            .inset(0.)
                            .background(Color::rgba8(0, 0, 0, 120))
                    })
                    .on_click_stop(move |_| nav_editor_open.set(false)),
                panel,
            ))
            .style(|s| {
                s.absolute()
                    .inset(0.)
                    .items_center()
                    .justify_center()
            })
            .into_any()
        },
    );
```

Add `modal` as the **last child** of the editing view's root stack so it paints
above everything else. Mirror how existing overlays in this file (e.g. the slash
menu wiring) are layered if the structure differs.

Imports to add in `mod.rs`:
```rust
use crate::ui::nav_editor::{self, NavModel};
use lopress_build::NavItem;
```
(`dyn_container`, `empty`, `stack`, `label`, `v_stack`, `Color`, and `Weight` are
already imported or available — match the file's existing imports.)

> Floem API details (`rgba8`, `inset`, `on_click_stop`, style methods) should be
> cross-checked against their use elsewhere in `crates/lopress-editor/src/ui/` —
> follow the local idiom when names differ.

- [ ] **Step 3: Update the `editing_view` call site for `sidebar_view`** (already shown above).

- [ ] **Step 4: Run to verify compilation**

Run: `cargo test -p lopress-editor`
Expected: May fail on compile — fix any issues.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/sidebar.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): add 'Site settings' sidebar entry + nav editor modal overlay"
```

---

### Task 11: Page/tag pickers — popup-button pattern

**Files:**
- Modify: `crates/lopress-editor/src/ui/nav_editor.rs`

**Goal:** Implement the page picker and tag picker as popup-button lists (same pattern as `attr_select` in `plugin.rs` — a row of small toggle buttons). The picker opens when the "Link to page ▾" or "Link to tag ▾" button is clicked, showing a list of page slugs / tag names. Clicking one fills the last-added row's href (and label if empty).

**Steps:**

- [ ] **Step 1: Add picker types and functions to `nav_editor.rs`:**

Add to the existing module:

```rust
/// A page choice for the page picker.
#[derive(Debug, Clone)]
pub struct PageChoice {
    pub slug: String,
    pub title: String,
}

/// A tag choice for the tag picker.
#[derive(Debug, Clone)]
pub struct TagChoice {
    pub name: String,
}

/// Build a popup list of page choices. Returns a view that, when a page
/// is clicked, calls `on_select` with the slug and title.
pub fn page_picker_view(
    pages: Vec<PageChoice>,
    on_select: impl Fn(String, String) + 'static,
) -> impl IntoView {
    let mut buttons: Vec<AnyView> = Vec::with_capacity(pages.len());
    for page in pages {
        let slug = page.slug.clone();
        let title = page.title.clone();
        let on_select_for_btn = on_select.clone();
        let btn = floem::views::button(label(move || title.clone()))
            .action(move || {
                on_select_for_btn(slug.clone(), title.clone());
            })
            .style(|s| s.font_size(11.).padding_horiz(6.).padding_vert(2.).width_full());
        buttons.push(btn.into_any());
    }
    v_stack_from_iter(buttons)
        .style(|s| s.gap(2.).border(1.).border_color(Color::rgb8(200, 200, 200)).border_radius(4.).background(Color::rgb8(255, 255, 255)).z_index(10))
}

/// Build a popup list of tag choices. Returns a view that, when a tag
/// is clicked, calls `on_select` with the tag name.
pub fn tag_picker_view(
    tags: Vec<TagChoice>,
    on_select: impl Fn(String) + 'static,
) -> impl IntoView {
    let mut buttons: Vec<AnyView> = Vec::with_capacity(tags.len());
    for tag in tags {
        let name = tag.name.clone();
        let on_select_for_btn = on_select.clone();
        let btn = floem::views::button(label(move || name.clone()))
            .action(move || {
                on_select_for_btn(name.clone());
            })
            .style(|s| s.font_size(11.).padding_horiz(6.).padding_vert(2.).width_full());
        buttons.push(btn.into_any());
    }
    v_stack_from_iter(buttons)
        .style(|s| s.gap(2.).border(1.).border_color(Color::rgb8(200, 200, 200)).border_radius(4.).background(Color::rgb8(255, 255, 255)).z_index(10))
}
```

- [ ] **Step 2: Extend `nav_editor_view` with picker data and internal picker
state.** The caller passes the page/tag lists in; the panel owns the popup
open/close state. `PageChoice`/`TagChoice` derive `Clone` so the popup builders
can be re-run by `dyn_container`.

**New signature (replaces Task 9's):**

```rust
pub fn nav_editor_view(
    model: RwSignal<NavModel>,
    pages: Vec<PageChoice>,
    tags: Vec<TagChoice>,
    on_save: impl Fn(Vec<NavItem>) + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl IntoView {
```

**Inside the function**, add picker state, toggle buttons, and conditionally
rendered popups. A pick fills the **last** row (adding one if the list is empty)
— this matches the spec's "focused/last row" with the simpler "last" choice,
since floem 0.2 gives no easy focused-row tracking across inputs:

```rust
    let page_picker_open: RwSignal<bool> = RwSignal::new(false);
    let tag_picker_open: RwSignal<bool> = RwSignal::new(false);

    let page_picker_btn = button(label(|| "Link to page ▾".to_string()))
        .action(move || {
            page_picker_open.update(|v| *v = !*v);
            tag_picker_open.set(false);
        })
        .style(|s| s.padding_vert(4.).padding_horiz(8.).font_size(12.));

    let tag_picker_btn = button(label(|| "Link to tag ▾".to_string()))
        .action(move || {
            tag_picker_open.update(|v| *v = !*v);
            page_picker_open.set(false);
        })
        .style(|s| s.padding_vert(4.).padding_horiz(8.).font_size(12.));

    let picker_row = h_stack((page_picker_btn, tag_picker_btn)).style(|s| s.gap(4.));

    let page_popup = dyn_container(
        move || page_picker_open.get(),
        move |open| {
            if !open {
                return floem::views::empty().into_any();
            }
            page_picker_view(pages.clone(), move |slug, title| {
                model.update(|m| {
                    if m.rows.is_empty() {
                        m.add_row();
                    }
                    let last = m.rows.len() - 1;
                    m.fill_href_from_page(last, &slug, &title);
                });
                page_picker_open.set(false);
            })
            .into_any()
        },
    );

    let tag_popup = dyn_container(
        move || tag_picker_open.get(),
        move |open| {
            if !open {
                return floem::views::empty().into_any();
            }
            tag_picker_view(tags.clone(), move |tag| {
                model.update(|m| {
                    if m.rows.is_empty() {
                        m.add_row();
                    }
                    let last = m.rows.len() - 1;
                    m.fill_href_from_tag(last, &tag);
                });
                tag_picker_open.set(false);
            })
            .into_any()
        },
    );
```

Insert `picker_row`, `page_popup`, and `tag_popup` into the panel's `v_stack`
just above `add_btn`. Update Task 9's `NavRow`-derive on `PageChoice`/`TagChoice`
to include `Clone` (already shown in Step 1).

- [ ] **Step 3: Update the `mod.rs` call site** — build the choices from the
session inside the modal's `open` branch (right where `nav_model` is
initialized in Task 10), then pass them through:

```rust
            let (pages, tags) = match editing_for_modal.borrow().as_ref() {
                Some(state) => {
                    let ws = state.session.workspace();
                    (
                        ws.pages
                            .iter()
                            .map(|p| nav_editor::PageChoice {
                                slug: p.slug.clone(),
                                title: p.title.clone(),
                            })
                            .collect::<Vec<_>>(),
                        ws.tags
                            .iter()
                            .map(|t| nav_editor::TagChoice { name: t.clone() })
                            .collect::<Vec<_>>(),
                    )
                }
                None => (Vec::new(), Vec::new()),
            };
```

and change the panel construction to:

```rust
                nav_editor::nav_editor_view(
                    nav_model,
                    pages,
                    tags,
                    on_nav_save.clone(),
                    on_nav_cancel,
                ),
```

- [ ] **Step 4: Run full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/nav_editor.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): wire page/tag pickers into nav editor modal"
```

---

### Task 12: Integration tests — migration warning end-to-end, cache invalidation

**Files:**
- Modify: `crates/lopress-build/tests/build_integration.rs`

**Goal:** Add integration tests for:
1. Migration warning appears in `BuildReport.warnings` when `[site.nav]` is present
2. Nav.toml changes trigger cache invalidation (full rebuild)
3. Empty nav.toml works correctly

**Steps:**

- [ ] **Step 1: Add integration tests** — append to `build_integration.rs`:

```rust
#[test]
fn migration_warning_appears_when_site_nav_present() {
    let (_tmp, root) = copy_fixture("minimal");
    // The minimal fixture no longer has [site.nav] — it uses nav.toml.
    // Add [site.nav] back to lopress.toml to trigger the warning.
    let toml_path = root.join("lopress.toml");
    let src = fs::read_to_string(&toml_path).unwrap();
    fs::write(
        &toml_path,
        format!("{src}\n\n[site.nav]\nitems = [{{ label = \"Old\", href = \"/old/\" }}]\n"),
    )
    .unwrap();

    let report = build(&root).unwrap();
    assert!(!report.warnings.is_empty(), "expected migration warning");
    assert!(
        report.warnings[0].contains("[site.nav]"),
        "warning should mention [site.nav]"
    );
}

#[test]
fn nav_toml_change_triggers_full_rebuild() {
    let (_tmp, root) = copy_fixture("minimal");
    let r1 = build(&root).unwrap();
    assert!(r1.failures.is_empty());
    assert!(r1.pages_rendered >= 1);

    // Change nav.toml.
    let nav_path = root.join("nav.toml");
    let src = fs::read_to_string(&nav_path).unwrap();
    fs::write(&nav_path, format!("{src}\n")).unwrap();

    let r2 = build(&root).unwrap();
    assert!(r2.failures.is_empty());
    assert_eq!(
        r2.pages_rendered, r1.pages_rendered,
        "nav.toml change should trigger full rebuild"
    );
}

#[test]
fn nav_toml_creation_triggers_full_rebuild() {
    let (_tmp, root) = copy_fixture("minimal");
    // Remove nav.toml.
    fs::remove_file(root.join("nav.toml")).unwrap();

    let r1 = build(&root).unwrap();
    assert!(r1.failures.is_empty());

    // Create nav.toml.
    fs::write(
        root.join("nav.toml"),
        "items = [{ label = \"NewLink\", href = \"/new/\" }]\n",
    )
    .unwrap();

    let r2 = build(&root).unwrap();
    assert!(r2.failures.is_empty());
    assert_eq!(
        r2.pages_rendered, r1.pages_rendered,
        "creating nav.toml should trigger a full rebuild"
    );
    // The new nav link actually lands in the rendered output.
    let index = fs::read_to_string(root.join("www/index.html")).unwrap();
    assert!(
        index.contains("<a href=\"/new/\">NewLink</a>"),
        "rebuilt pages should render the new nav link"
    );
}

#[test]
fn empty_nav_builds_without_nav_links() {
    let (_tmp, root) = copy_fixture("minimal");
    // Write empty nav.toml.
    fs::write(root.join("nav.toml"), "items = []\n").unwrap();

    let report = build(&root).unwrap();
    assert!(report.failures.is_empty());

    // The rendered pages should not contain nav links (the default theme
    // renders nav items as `<a href="...">label</a>` inside .site-nav).
    let index = fs::read_to_string(root.join("www/index.html")).unwrap();
    assert!(
        !index.contains("<a href=\"/about/\">About</a>"),
        "index should not render nav links when nav is empty"
    );
}
```

- [ ] **Step 2: Run to verify they pass**

Run: `cargo test -p lopress-build migration_warning`
Run: `cargo test -p lopress-build nav_toml_change`
Run: `cargo test -p lopress-build nav_toml_creation`
Run: `cargo test -p lopress-build empty_nav`
Expected: PASS.

- [ ] **Step 3: Run full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/tests/build_integration.rs
git commit -m "test(build): add integration tests for migration warning, cache invalidation, empty nav"
```

---

### Task 13: End-to-end verification via the control server

**Files:** none (verification only; fix whatever it surfaces)

**Goal:** Drive the real editor GUI and prove the whole path works: open Site
settings, add a link, save, and confirm `nav.toml` + the rebuilt pages.

**Steps:**

- [ ] **Step 1: Scaffold a real workspace** — per the project's e2e workflow,
hand-rolled workspaces 404 on `/open`; scaffold instead:

```bash
cargo run -- new /tmp/nav-e2e --title "Nav E2E" --base-url http://localhost:8080
```

(Check `lopress_build::cli` for the exact flag names if this errors.)

- [ ] **Step 2: Launch the editor and drive it** — use the
`driving-lopress-editor` skill (debug HTTP control server on `127.0.0.1:7878`):
open the workspace, click "⚙ Site settings", add a row (label `GitHub`, href
`https://github.com/corporealshift`), pick a page from "Link to page ▾", click
Save. Verify with `/screenshot` at each stage — the modal must be clickable
(floem hit-test caveat).

- [ ] **Step 3: Assert on disk** — `/tmp/nav-e2e/nav.toml` contains both items;
`/tmp/nav-e2e/www/index.html` renders both `<a>` links in `.site-nav`; saving
again with a row removed updates both.

- [ ] **Step 4: Run the full gate one final time**

Run: `bash scripts/check.sh`
Expected: PASS.

---

## Summary of Changes

| Task | What changes | Files |
|------|-------------|-------|
| 1 | `write_nav` — TOML serialization + atomic write | `site.rs` |
| 2 | `Site` removes `nav`; `Workspace` gains `nav` + `warnings`; migration warning | `site.rs` |
| 3 | `BuildReport` gains `warnings`; `build()` reads `ws.nav` | `build.rs` |
| 4 | `hash_config` extends to `nav.toml` | `cache.rs` |
| 5 | Scaffold writes `nav.toml` instead of `[site.nav]` | `scaffold.rs`, `scaffold_tests.rs` |
| 6 | Migrate fixtures to `nav.toml` | `fixtures/*/lopress.toml`, `fixtures/*/nav.toml` |
| 7 | `DocumentRef` gains `slug`; `WorkspaceSummary` gains `tags` | `session.rs` |
| 8 | `Session::nav_items()` + `Session::update_nav()` | `session.rs` |
| 9 | `nav_editor.rs` — panel with working model | `nav_editor.rs` (new), `mod.rs` |
| 10 | Sidebar "Site settings" button + modal overlay | `sidebar.rs`, `mod.rs` |
| 11 | Page/tag pickers (popup-button pattern) | `nav_editor.rs` |
| 12 | Integration tests for migration, cache, empty nav | `build_integration.rs` |

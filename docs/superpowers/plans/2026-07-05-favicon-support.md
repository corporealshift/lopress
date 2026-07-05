# Favicon Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pick a favicon in the Site settings dialog; lopress copies it to `src/favicon.<ext>`, serves it at `/favicon.<ext>`, and emits a `<link rel="icon">` tag on every page.

**Architecture:** Convention file (`src/favicon.<ext>`, no config writes) discovered by the build (svg → png → ico priority), copied to `www/`, exposed to templates as `SiteCtx.favicon`, and managed from the editor via three new `Session` methods with staged-on-save semantics in the Site settings modal.

**Tech Stack:** Rust workspace — lopress-build, lopress-theme, lopress-gui-host, lopress-editor (floem), rfd for the native file dialog, Tera templates.

---

## Scope

Scope Check done by Claude: this spec is a single plan's worth of work — one feature
threading four crates in a straight dependency line (build → theme → gui-host →
editor). Do not split into multiple plans and do not re-litigate scope.

**Spec:** `docs/superpowers/specs/2026-07-05-favicon-support-design.md`. Branch: `feature/favicon-support` (already created).

## Conventions

- Tests: standard `cargo test`; unit tests in-module under `#[cfg(test)]`,
  integration tests in `crates/<crate>/tests/`. Session tests follow
  `crates/lopress-gui-host/tests/session_integration.rs` (`make_workspace()`
  helper + tempdir).
- **The gate:** every task's final verification step is `bash scripts/check.sh`
  (fmt, taplo, clippy `--workspace -D warnings`, suppression check, tests). Never
  a per-crate clippy. After running other cargo commands, `touch` changed `.rs`
  files before trusting a green clippy (cache false-pass).
- Lints (AGENTS.md): no `unwrap`/`expect`/`panic`/indexing in `src/`; no lossy
  `as` casts; every `#[allow]` needs an adjacent justification comment; pattern
  matching over is_some/unwrap ladders. (Tests are exempt from the panic set.)
- Commits: short imperative summary line, **no type prefix** (match
  `git log --oneline`, e.g. "Fix body for callouts"). Stage **named files
  only** — never `git add -A`, including in the final gate-pass step.

---

## File Structure Map

| File | Change | Responsibility |
|------|--------|----------------|
| `crates/lopress-build/src/site.rs` | Modify | Add `Workspace::favicon()` helper returning `Option<(PathBuf, String)>` |
| `crates/lopress-build/src/cache.rs` | Modify | Add `hash_favicon()`; add `favicon_hash` field to `BuildCache` |
| `crates/lopress-build/src/build.rs` | Modify | OR `favicon_hash` into `force_full`; copy favicon to `www/` in the `force_full` block; duplicate-favicon warning |
| `crates/lopress-theme/src/context.rs` | Modify | Add `favicon: Option<String>` to `SiteCtx` |
| `crates/lopress-theme/assets/default-theme/templates/layout.html` | Modify | Conditional `<link rel="icon">` after the stylesheet link |
| `crates/lopress-theme/src/builtin.rs` | Modify | Update test `SiteCtx` sites; add favicon render tests |
| `crates/lopress-theme/src/engine.rs` | Modify | Update `fn site()` test helper |
| `crates/lopress-build/src/pages.rs` | Modify | Populate `favicon` on its `SiteCtx` |
| `crates/lopress-build/src/feed.rs` | Modify | Update test `SiteCtx` site |
| `docs/themes.md` | Modify | Add `site.favicon` row to the context table |
| `crates/lopress-gui-host/src/session.rs` | Modify | Add `favicon()`, `set_favicon()`, `remove_favicon()` |
| `crates/lopress-gui-host/tests/session_integration.rs` | Modify | Integration tests for the three methods |
| `crates/lopress-editor/src/ui/nav_editor.rs` | Modify | `FaviconChange` enum (pure data) + unit tests |
| `crates/lopress-editor/src/ui/mod.rs` | Modify | Favicon section in the modal; staged-on-save wiring; title change |

**Note on stale files:** a `force_full` build wipes `www/` before writing
(`build.rs:55-73`), so no explicit stale-favicon cleanup is needed anywhere —
any favicon change flips `favicon_hash` (Task 2), which forces a full rebuild,
which wipes the old copy.

---

### Task 1: `Workspace::favicon()` helper with unit tests

**Files:**
- Modify: `crates/lopress-build/src/site.rs`

**Goal:** Add `Workspace::favicon()` that checks `src/` for `favicon.svg`, `favicon.png`, `favicon.ico` in priority order and returns `(source PathBuf, web path String)` or `None`.

- [ ] **Step 1: Write the failing tests** — append inside `site.rs`'s existing `mod tests`. `Workspace::load` requires a `lopress.toml`, so each test scaffolds one (same minimal config the `cache.rs` tests use):

```rust
    fn favicon_workspace(d: &TempDir) -> Workspace {
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        Workspace::load(d.path()).unwrap()
    }

    #[test]
    fn favicon_returns_svg_when_present() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.svg"), b"<svg/>").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.svg"));
        assert_eq!(web, "/favicon.svg");
    }

    #[test]
    fn favicon_prefers_svg_over_png() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.svg"), b"<svg/>").unwrap();
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.svg"));
        assert_eq!(web, "/favicon.svg");
    }

    #[test]
    fn favicon_falls_back_to_png() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.png"));
        assert_eq!(web, "/favicon.png");
    }

    #[test]
    fn favicon_falls_back_to_ico() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.ico"), b"ICO").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.ico"));
        assert_eq!(web, "/favicon.ico");
    }

    #[test]
    fn favicon_returns_none_when_no_file_exists() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        assert!(ws.favicon().is_none());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build favicon`
Expected: FAIL to compile (`favicon` method does not exist on `Workspace`).

- [ ] **Step 3: Implement `Workspace::favicon()`** — add to the `impl Workspace` block, near `src_dir()` (site.rs:121):

```rust
    /// Find the favicon in `src/` by priority order (svg → png → ico).
    ///
    /// Returns `(source_path, web_path)` — e.g. `(…/src/favicon.png,
    /// "/favicon.png")` — or `None` when no favicon file exists.
    pub fn favicon(&self) -> Option<(PathBuf, String)> {
        let src = self.src_dir();
        for ext in ["svg", "png", "ico"] {
            let path = src.join(format!("favicon.{ext}"));
            if path.exists() {
                return Some((path, format!("/favicon.{ext}")));
            }
        }
        None
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-build favicon`
Expected: PASS (all five tests).

- [ ] **Step 5: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/site.rs
git commit -m "Add Workspace::favicon helper with priority discovery"
```

---

### Task 2: Cache invalidation — `hash_favicon()` + `favicon_hash`

**Files:**
- Modify: `crates/lopress-build/src/cache.rs`
- Modify: `crates/lopress-build/src/build.rs`

**Goal:** Any favicon change (add, remove, rename, content edit) must force a full rebuild, because every page's HTML changes. Add `hash_favicon(&Workspace)` to `cache.rs`, a `favicon_hash` field to `BuildCache`, and OR the comparison into `force_full` in `build.rs`.

This task comes **before** the copy step (Task 3) because Task 3's
remove-favicon test only passes once favicon changes trigger `force_full`
(which wipes `www/`).

**Verified:** `hash_bytes` (cache.rs:89), `hash_file` (cache.rs:204), and
`hash_many` (cache.rs:94) already exist. `hash_theme` stays untouched — the
favicon is workspace state, not theme state, so it gets its own hash.

- [ ] **Step 1: Write the failing test** — append inside `cache.rs`'s `mod tests`:

```rust
    #[test]
    fn hash_favicon_changes_with_presence_and_content() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        let ws = crate::site::Workspace::load(d.path()).unwrap();

        let h_none = hash_favicon(&ws).unwrap();

        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        let h_added = hash_favicon(&ws).unwrap();
        assert_ne!(h_none, h_added, "adding a favicon must change the hash");

        std::fs::write(d.path().join("src/favicon.png"), b"PNG2").unwrap();
        let h_edited = hash_favicon(&ws).unwrap();
        assert_ne!(h_added, h_edited, "editing favicon bytes must change the hash");

        std::fs::remove_file(d.path().join("src/favicon.png")).unwrap();
        let h_removed = hash_favicon(&ws).unwrap();
        assert_eq!(h_none, h_removed, "no favicon must hash to the stable sentinel");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-build hash_favicon`
Expected: FAIL to compile (`hash_favicon` does not exist).

- [ ] **Step 3: Add `hash_favicon()` to `cache.rs`** — after `hash_file` (cache.rs:204):

```rust
/// Hash the workspace's favicon so any change (add/remove/rename/content
/// edit) invalidates the page cache — the favicon link tag appears in every
/// page's HTML. No favicon hashes to a stable sentinel (empty bytes).
pub fn hash_favicon(workspace: &Workspace) -> Result<String, BuildError> {
    match workspace.favicon() {
        Some((path, web)) => {
            let mut items = vec![(web, std::fs::read(&path)?)];
            Ok(hash_many(&mut items))
        }
        None => Ok(hash_bytes(&[])),
    }
}
```

(Including the web path in the hashed items makes an extension rename — same
bytes, `favicon.png` → `favicon.ico` — change the hash too.)

- [ ] **Step 4: Add `favicon_hash` to `BuildCache`** — in `cache.rs`:

**Before (cache.rs:11-34):**
```rust
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
```

**After:**
```rust
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
    pub favicon_hash: String,
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
            favicon_hash: String::new(),
            pages: BTreeMap::new(),
        }
    }
}
```

(`#[serde(default)]` means an existing on-disk cache loads with an empty
`favicon_hash`, which differs from the computed hash → one forced full rebuild
after upgrade. Correct and self-healing; no `CACHE_VERSION` bump needed.)

- [ ] **Step 5: Wire into `force_full` in `build.rs`**

**Before (build.rs:44-52):**
```rust
    // Load cache and compute global hashes
    let mut build_cache = BuildCache::load(&ws.cache_path())?;
    let cfg_hash = cache::hash_config(&ws)?;
    let theme_hash = cache::hash_theme(&theme)?;
    let plugins_hash = cache::hash_plugins(&registry)?;

    let force_full = build_cache.config_hash != cfg_hash
        || build_cache.theme_hash != theme_hash
        || build_cache.plugins_hash != plugins_hash;
```

**After:**
```rust
    // Load cache and compute global hashes
    let mut build_cache = BuildCache::load(&ws.cache_path())?;
    let cfg_hash = cache::hash_config(&ws)?;
    let theme_hash = cache::hash_theme(&theme)?;
    let plugins_hash = cache::hash_plugins(&registry)?;
    let favicon_hash = cache::hash_favicon(&ws)?;

    let force_full = build_cache.config_hash != cfg_hash
        || build_cache.theme_hash != theme_hash
        || build_cache.plugins_hash != plugins_hash
        || build_cache.favicon_hash != favicon_hash;
```

**Before (build.rs:235-239, the cache-persist block):**
```rust
    // Update and persist cache
    build_cache.config_hash = cfg_hash;
    build_cache.theme_hash = theme_hash;
    build_cache.plugins_hash = plugins_hash;
    build_cache.save(&ws.cache_path())?;
```

**After:**
```rust
    // Update and persist cache
    build_cache.config_hash = cfg_hash;
    build_cache.theme_hash = theme_hash;
    build_cache.plugins_hash = plugins_hash;
    build_cache.favicon_hash = favicon_hash;
    build_cache.save(&ws.cache_path())?;
```

- [ ] **Step 6: Run to verify tests pass**

Run: `cargo test -p lopress-build`
Expected: PASS (new hash_favicon test + all existing tests).

- [ ] **Step 7: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-build/src/cache.rs crates/lopress-build/src/build.rs
git commit -m "Invalidate build cache when the favicon changes"
```

---

### Task 3: Copy favicon to `www/` + duplicate warning

**Files:**
- Modify: `crates/lopress-build/src/build.rs`

**Goal:** On a full rebuild, copy `src/favicon.<ext>` to `www/favicon.<ext>`. Warn (via `BuildReport.warnings`) when more than one `src/favicon.*` exists. No stale-file cleanup is needed: `force_full` wipes `www/` first (build.rs:55-73), and any favicon change forces full via Task 2.

- [ ] **Step 1: Write the failing tests** — append inside `build.rs`'s `mod tests` (scaffold pattern copied from the existing `build_report_contains_warnings_from_workspace` test):

```rust
    fn favicon_site(d: &TempDir) {
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();
        for sub in ["src/posts", "src/pages", "src/images", "plugins"] {
            std::fs::create_dir_all(d.path().join(sub)).unwrap();
        }
    }

    #[test]
    fn favicon_is_copied_to_www_on_full_build() {
        let d = TempDir::new().unwrap();
        favicon_site(&d);
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();

        build(d.path()).unwrap();
        assert!(d.path().join("www/favicon.png").exists());
    }

    #[test]
    fn removed_favicon_disappears_from_www_on_rebuild() {
        let d = TempDir::new().unwrap();
        favicon_site(&d);
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        build(d.path()).unwrap();
        assert!(d.path().join("www/favicon.png").exists());

        // Removing the favicon flips favicon_hash → force_full → www/ wiped.
        std::fs::remove_file(d.path().join("src/favicon.png")).unwrap();
        build(d.path()).unwrap();
        assert!(!d.path().join("www/favicon.png").exists());
    }

    #[test]
    fn duplicate_favicons_emit_warning() {
        let d = TempDir::new().unwrap();
        favicon_site(&d);
        std::fs::write(d.path().join("src/favicon.svg"), b"<svg/>").unwrap();
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();

        let report = build(d.path()).unwrap();
        assert!(
            report.warnings.iter().any(|w| w.contains("favicon")),
            "expected a duplicate-favicon warning, got: {:?}",
            report.warnings
        );
        // Priority order: svg wins.
        assert!(d.path().join("www/favicon.svg").exists());
        assert!(!d.path().join("www/favicon.png").exists());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build favicon_is_copied`
Expected: FAIL (`www/favicon.png` does not exist — no copy logic yet).

- [ ] **Step 3: Make `build()`'s warnings extensible** — the report currently moves `ws.warnings` verbatim. Collect them into a local so the favicon check can append:

**Before (build.rs:24-29):**
```rust
    let ws = Workspace::load(workspace)?;

    // Log migration warnings to stderr.
    for warning in &ws.warnings {
        eprintln!("warning: {warning}");
    }
```

**After:**
```rust
    let ws = Workspace::load(workspace)?;

    // Log migration warnings to stderr; collected into the report below.
    let mut warnings = ws.warnings.clone();
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    // Favicon sanity: more than one src/favicon.* is almost certainly a
    // hand-editing mistake — say which one the priority order picked.
    let favicon_variants: Vec<String> = ["svg", "png", "ico"]
        .iter()
        .map(|ext| format!("favicon.{ext}"))
        .filter(|name| ws.src_dir().join(name).exists())
        .collect();
    if favicon_variants.len() > 1 {
        if let Some(used) = favicon_variants.first() {
            let msg = format!(
                "multiple favicon files in src/ ({}); using {used}",
                favicon_variants.join(", ")
            );
            eprintln!("warning: {msg}");
            warnings.push(msg);
        }
    }
```

**Before (build.rs:251-257, the report construction):**
```rust
    Ok(BuildReport {
        pages_written,
        pages_rendered: stats.pages_rendered,
        pages_skipped: stats.pages_skipped,
        failures,
        warnings: ws.warnings,
    })
```

**After:**
```rust
    Ok(BuildReport {
        pages_written,
        pages_rendered: stats.pages_rendered,
        pages_skipped: stats.pages_skipped,
        failures,
        warnings,
    })
```

- [ ] **Step 4: Copy the favicon in the `force_full` theme-assets block**

**Before (build.rs:223-233):**
```rust
    // Theme assets: only on full rebuild
    if force_full {
        write_theme_css(&ws, &theme)?;
        for plugin in &registry.plugins {
            let assets = plugin.root.join("assets");
            if assets.exists() {
                let target = ws.www_dir().join("assets").join(&plugin.manifest.name);
                copy_dir(&assets, &target)?;
            }
        }
    }
```

**After:**
```rust
    // Theme assets: only on full rebuild
    if force_full {
        write_theme_css(&ws, &theme)?;
        for plugin in &registry.plugins {
            let assets = plugin.root.join("assets");
            if assets.exists() {
                let target = ws.www_dir().join("assets").join(&plugin.manifest.name);
                copy_dir(&assets, &target)?;
            }
        }

        // Favicon: copied as-is to the www root. No stale cleanup needed —
        // any favicon change forces a full rebuild, which wiped www/ above.
        if let Some((src_path, web_path)) = ws.favicon() {
            let target = ws.www_dir().join(web_path.trim_start_matches('/'));
            std::fs::copy(&src_path, &target)?;
        }
    }
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-build favicon`
Expected: PASS (Task 1's five tests + Task 2's hash test + the three new tests).

- [ ] **Step 6: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-build/src/build.rs
git commit -m "Copy favicon to www on full rebuild and warn on duplicates"
```

---

### Task 4: `SiteCtx.favicon` + `layout.html` conditional + docs

**Files:**
- Modify: `crates/lopress-theme/src/context.rs`
- Modify: `crates/lopress-theme/assets/default-theme/templates/layout.html`
- Modify: `crates/lopress-theme/src/builtin.rs` (test sites + new render tests)
- Modify: `crates/lopress-theme/src/engine.rs` (test helper)
- Modify: `crates/lopress-build/src/build.rs` (populate `favicon`)
- Modify: `crates/lopress-build/src/pages.rs` (populate `favicon`)
- Modify: `crates/lopress-build/src/feed.rs` (test site)
- Modify: `docs/themes.md`

**Goal:** Add `favicon: Option<String>` to `SiteCtx`, populate it from `Workspace::favicon()`, and emit the conditional link tag in the default theme. `SiteCtx` is a shared type — every construction site must gain the field or the workspace stops compiling.

**Verified construction sites** (`grep -rn "SiteCtx {"`): `build.rs:160`,
`pages.rs:124`, `feed.rs:63` (test), `builtin.rs:49` (test), `builtin.rs:~100`
(test), `engine.rs:34` (test helper). If the grep finds more at implementation
time, update those too.

- [ ] **Step 1: Add the field** — in `context.rs`:

**Before:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub nav: Vec<NavItem>,
    pub posts: Vec<PostSummary>,
}
```

**After:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub nav: Vec<NavItem>,
    pub posts: Vec<PostSummary>,
    /// Web path of the site favicon (e.g. `"/favicon.png"`), or `None`.
    pub favicon: Option<String>,
}
```

- [ ] **Step 2: Populate in `build.rs`** — in the `site_ctx` construction (build.rs:160), add one field after `posts`:

```rust
        posts: summaries.clone(),
        favicon: ws.favicon().map(|(_, web)| web),
    };
```

- [ ] **Step 3: Populate in `pages.rs`** — in the `site_ctx` construction (pages.rs:124), add one field after `posts`:

```rust
        posts: summaries.clone(),
        favicon: workspace.favicon().map(|(_, web)| web),
    };
```

- [ ] **Step 4: Fix the test construction sites** — in each of `feed.rs:63`, `builtin.rs:49`, `builtin.rs:~100`, and `engine.rs:34` (`fn site()`), add `favicon: None,` immediately after the `nav: vec![],` line of the `SiteCtx` literal. Example (`engine.rs`):

**Before:**
```rust
    fn site() -> SiteCtx {
        SiteCtx {
            title: "T".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![],
        }
    }
```

**After:**
```rust
    fn site() -> SiteCtx {
        SiteCtx {
            title: "T".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            favicon: None,
            posts: vec![],
        }
    }
```

(Struct literal field order is free in Rust; the `nav: vec![],` line is a
unique anchor in all four test sites, including `feed.rs` where the `posts`
value spans many lines.)

- [ ] **Step 5: Update `layout.html`** — in `crates/lopress-theme/assets/default-theme/templates/layout.html`:

**Before:**
```html
<link rel="stylesheet" href="/assets/theme.css">
<meta property="og:title" content="{{ page.title }}">
```

**After:**
```html
<link rel="stylesheet" href="/assets/theme.css">
{% if site.favicon %}<link rel="icon" href="{{ site.favicon | safe }}">{% endif %}
<meta property="og:title" content="{{ page.title }}">
```

(`| safe` is deliberate: Tera entity-escapes `/` in interpolated values on
`.html` templates, which would render `href="&#x2F;favicon.png"`. The value is
program-generated by `Workspace::favicon()` — one of exactly three strings —
never user input, so bypassing escaping is safe and keeps the output and the
render tests readable.)

- [ ] **Step 6: Add render tests** — append inside `builtin.rs`'s `mod tests` (the existing `default_engine_renders_post` test is the model; `page()`-style values copied from it):

```rust
    #[test]
    fn layout_emits_favicon_link_when_set() {
        let engine = default_engine().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            favicon: Some("/favicon.png".into()),
            posts: vec![],
        };
        let page = PageCtx {
            kind: PageKind::Post,
            title: "Hi".into(),
            slug: "hi".into(),
            url: "/posts/hi/".into(),
            canonical: "https://example.com/posts/hi/".into(),
            description: None,
            og_image: None,
            date: None,
            tags: vec![],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        };
        let html = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site,
                    page: &page,
                },
            )
            .unwrap();
        assert!(
            html.contains(r#"<link rel="icon" href="/favicon.png">"#),
            "favicon link tag missing from rendered head"
        );
    }

    #[test]
    fn layout_omits_favicon_link_when_none() {
        let engine = default_engine().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            favicon: None,
            posts: vec![],
        };
        let page = PageCtx {
            kind: PageKind::Post,
            title: "Hi".into(),
            slug: "hi".into(),
            url: "/posts/hi/".into(),
            canonical: "https://example.com/posts/hi/".into(),
            description: None,
            og_image: None,
            date: None,
            tags: vec![],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        };
        let html = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site,
                    page: &page,
                },
            )
            .unwrap();
        assert!(
            !html.contains(r#"rel="icon""#),
            "no favicon link tag expected when favicon is None"
        );
    }
```

- [ ] **Step 7: Update `docs/themes.md`** — in the `site` context table, after the `site.posts` row:

**Before:**
```
| `site.posts` | array of PostSummary | all non-draft posts, for archives |
```

**After:**
```
| `site.posts` | array of PostSummary | all non-draft posts, for archives |
| `site.favicon` | string or null | web path like `/favicon.png`; null when the site has no favicon |
```

- [ ] **Step 8: Run to verify**

Run: `cargo test -p lopress-theme -p lopress-build`
Expected: PASS (the two new render tests + all existing tests compile with the new field).

- [ ] **Step 9: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-theme/src/context.rs crates/lopress-theme/src/builtin.rs crates/lopress-theme/src/engine.rs crates/lopress-theme/assets/default-theme/templates/layout.html crates/lopress-build/src/build.rs crates/lopress-build/src/pages.rs crates/lopress-build/src/feed.rs docs/themes.md
git commit -m "Add favicon to SiteCtx and emit link tag in the default theme"
```

---

### Task 5: `Session` methods — `favicon()`, `set_favicon()`, `remove_favicon()`

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Modify: `crates/lopress-gui-host/tests/session_integration.rs`

**Goal:** Three methods mirroring `import_image` (session.rs:325) and `update_nav` (session.rs:391). `Session::rebuild()` (session.rs:281) takes `&self`, returns `()` — call it after each mutation.

- [ ] **Step 1: Write the failing tests** — append to `session_integration.rs` (the file already imports `std::fs` and has `make_workspace()`):

```rust
#[test]
fn favicon_returns_none_when_absent() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    assert!(session.favicon().is_none());
}

#[test]
fn favicon_returns_filename_when_present() {
    let dir = make_workspace();
    fs::write(dir.path().join("src/favicon.png"), b"PNG").unwrap();
    let session = Session::open(dir.path()).unwrap();
    assert_eq!(session.favicon(), Some("favicon.png".to_string()));
}

#[test]
fn favicon_prefers_svg_over_png() {
    let dir = make_workspace();
    fs::write(dir.path().join("src/favicon.svg"), b"<svg/>").unwrap();
    fs::write(dir.path().join("src/favicon.png"), b"PNG").unwrap();
    let session = Session::open(dir.path()).unwrap();
    assert_eq!(session.favicon(), Some("favicon.svg".to_string()));
}

#[test]
fn set_favicon_copies_file_to_src() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let picked = dir.path().join("src").join("original.png");
    fs::write(&picked, b"\x89PNG\r\n\x1a\nfake_png").unwrap();
    session.set_favicon(&picked).unwrap();
    assert_eq!(session.favicon(), Some("favicon.png".to_string()));
    assert!(dir.path().join("src/favicon.png").exists());
}

#[test]
fn set_favicon_evicts_other_extensions() {
    let dir = make_workspace();
    fs::write(dir.path().join("src/favicon.png"), b"OLD").unwrap();
    let session = Session::open(dir.path()).unwrap();
    assert_eq!(session.favicon(), Some("favicon.png".to_string()));

    let picked = dir.path().join("src").join("new.ico");
    fs::write(&picked, b"ICO").unwrap();
    session.set_favicon(&picked).unwrap();
    assert_eq!(session.favicon(), Some("favicon.ico".to_string()));
    assert!(
        !dir.path().join("src/favicon.png").exists(),
        "old favicon.png must be evicted (at-most-one invariant)"
    );
}

#[test]
fn set_favicon_rejects_invalid_extension() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let picked = dir.path().join("src").join("photo.jpg");
    fs::write(&picked, b"JPEG").unwrap();
    assert!(session.set_favicon(&picked).is_err());
    assert!(session.favicon().is_none(), "rejected set must not leave a favicon");
}

#[test]
fn remove_favicon_deletes_file() {
    let dir = make_workspace();
    fs::write(dir.path().join("src/favicon.svg"), b"<svg/>").unwrap();
    let session = Session::open(dir.path()).unwrap();
    assert_eq!(session.favicon(), Some("favicon.svg".to_string()));
    session.remove_favicon().unwrap();
    assert!(session.favicon().is_none());
    assert!(!dir.path().join("src/favicon.svg").exists());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-gui-host favicon`
Expected: FAIL to compile (the three methods do not exist).

- [ ] **Step 3: Add the three methods** — in `session.rs`, directly after `update_nav`. Note: extension validation happens **before** the eviction loop, so a rejected set never deletes the existing favicon (the `set_favicon_rejects_invalid_extension` test covers this):

```rust
    /// Current favicon filename (e.g. `"favicon.png"`), read fresh from disk
    /// so repeated calls observe external edits. `None` when no favicon
    /// file exists.
    pub fn favicon(&self) -> Option<String> {
        let (path, _) = self.workspace.favicon()?;
        let name = path.file_name()?.to_str()?;
        Some(name.to_string())
    }

    /// Set the site favicon: validate the extension, evict any existing
    /// `src/favicon.*`, copy `src` to `src/favicon.<ext>`, then trigger a
    /// rebuild.
    ///
    /// # Errors
    /// Returns `SaveError::Io` when the extension is not ico/png/svg or on
    /// I/O failure.
    pub fn set_favicon(&self, src: &Path) -> Result<(), SaveError> {
        let ext = match src.extension().and_then(|s| s.to_str()) {
            Some(e @ ("ico" | "png" | "svg")) => e,
            _ => {
                return Err(SaveError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "favicon must be .ico, .png, or .svg (got {})",
                        src.display()
                    ),
                )));
            }
        };

        // At-most-one invariant: remove any existing favicon.* first.
        for existing in ["svg", "png", "ico"] {
            let path = self.workspace.src_dir().join(format!("favicon.{existing}"));
            if path.exists() {
                std::fs::remove_file(&path).map_err(SaveError::Io)?;
            }
        }

        let dst = self.workspace.src_dir().join(format!("favicon.{ext}"));
        std::fs::copy(src, &dst).map_err(SaveError::Io)?;

        self.rebuild();
        Ok(())
    }

    /// Remove the site favicon (delete `src/favicon.*`), then trigger a
    /// rebuild. A no-op success when no favicon exists.
    ///
    /// # Errors
    /// Returns `SaveError::Io` on I/O failure.
    pub fn remove_favicon(&self) -> Result<(), SaveError> {
        for ext in ["svg", "png", "ico"] {
            let path = self.workspace.src_dir().join(format!("favicon.{ext}"));
            if path.exists() {
                std::fs::remove_file(&path).map_err(SaveError::Io)?;
            }
        }
        self.rebuild();
        Ok(())
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-gui-host favicon`
Expected: PASS (all seven new tests).

- [ ] **Step 5: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/tests/session_integration.rs
git commit -m "Add favicon methods to Session"
```

---

### Task 6: Editor UI — Favicon section in the Site settings modal

**Files:**
- Modify: `crates/lopress-editor/src/ui/nav_editor.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

**Goal:** Rename the modal to "Site settings", add a Favicon section above Navigation with staged-on-save semantics. The staging signal is created inside the modal's `dyn_container` closure so it is fresh (`Unchanged`) on every open.

**Note on Save semantics:** on Save, the favicon change applies first, then the
nav change. If the favicon apply fails, the modal stays open with the error and
nav is not written. Re-clicking Save re-applies the staged favicon change —
`set_favicon`/`remove_favicon` are idempotent, so this is safe. Two rebuilds
fire when both favicon and nav changed (one per session call); acceptable — the
second wins.

- [ ] **Step 1: Add `FaviconChange` to `nav_editor.rs`** — new imports at the top of the file, then the enum before the `NavModel` section:

**Imports — Before:**
```rust
use lopress_build::NavItem;
```

**Imports — After:**
```rust
use lopress_build::NavItem;
use lopress_gui_host::Session;
use std::path::PathBuf;
```

**New code (before the `// ── Working model` section):**
```rust
/// Staged favicon change for the Site settings modal.
///
/// The modal stages the user's choice and applies it on Save; Cancel
/// discards. A fresh signal (starting `Unchanged`) is created each time the
/// modal opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaviconChange {
    Unchanged,
    Set(PathBuf),
    Remove,
}

impl FaviconChange {
    /// Apply the staged change to the session. `Ok(())` for `Unchanged`.
    /// Errors are stringified for the modal's error line.
    pub fn apply_to_session(&self, session: &Session) -> Result<(), String> {
        match self {
            Self::Set(path) => session.set_favicon(path).map_err(|e| e.to_string()),
            Self::Remove => session.remove_favicon().map_err(|e| e.to_string()),
            Self::Unchanged => Ok(()),
        }
    }

    /// Label for the modal's status line: the staged filename, a removal
    /// marker, or `None` when unchanged (caller falls back to the current
    /// on-disk state).
    pub fn display_label(&self) -> Option<String> {
        match self {
            Self::Set(path) => Some(
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(chosen file)")
                    .to_string(),
            ),
            Self::Remove => Some("(will be removed on save)".to_string()),
            Self::Unchanged => None,
        }
    }
}
```

- [ ] **Step 2: Unit tests for the staging model** — append inside `nav_editor.rs`'s existing `mod tests`:

```rust
    #[test]
    fn favicon_change_display_label_for_set_is_filename() {
        let c = FaviconChange::Set(PathBuf::from("C:/pics/icon.png"));
        assert_eq!(c.display_label(), Some("icon.png".to_string()));
    }

    #[test]
    fn favicon_change_display_label_for_remove_is_marker() {
        assert_eq!(
            FaviconChange::Remove.display_label(),
            Some("(will be removed on save)".to_string())
        );
    }

    #[test]
    fn favicon_change_display_label_for_unchanged_is_none() {
        assert_eq!(FaviconChange::Unchanged.display_label(), None);
    }

    #[test]
    fn favicon_change_staging_transitions() {
        // The signal just holds the latest choice; any state may replace any other.
        let mut c = FaviconChange::Unchanged;
        c = FaviconChange::Set(PathBuf::from("a.svg"));
        assert!(matches!(c, FaviconChange::Set(_)));
        c = FaviconChange::Remove;
        assert_eq!(c, FaviconChange::Remove);
        c = FaviconChange::Unchanged;
        assert_eq!(c, FaviconChange::Unchanged);
    }
```

- [ ] **Step 3: Run to verify they pass**

Run: `cargo test -p lopress-editor favicon_change`
Expected: PASS.

- [ ] **Step 4: Import `button` and `FaviconChange` in `mod.rs`**

**Before (mod.rs:23 and mod.rs:40):**
```rust
use floem::views::{dyn_container, empty, h_stack, label, stack, v_stack, Decorators};
```
```rust
use crate::ui::nav_editor::{NavModel, PageChoice, TagChoice};
```

**After:**
```rust
use floem::views::{button, dyn_container, empty, h_stack, label, stack, v_stack, Decorators};
```
```rust
use crate::ui::nav_editor::{FaviconChange, NavModel, PageChoice, TagChoice};
```

- [ ] **Step 5: Thread the current favicon out of the modal's data block** — the tuple gains a fourth element (mod.rs:369-389):

**Before:**
```rust
            let (model, pages, tags) = {
```
…and its closing line:
```rust
                (NavModel::new(state.session.nav_items()), pages, tags)
            };
```

**After:**
```rust
            let (model, pages, tags, current_favicon) = {
```
…and:
```rust
                let current_favicon = state.session.favicon();
                (
                    NavModel::new(state.session.nav_items()),
                    pages,
                    tags,
                    current_favicon,
                )
            };
```

- [ ] **Step 6: Create the staging signal and apply-on-save** — (mod.rs:390-406):

**Before:**
```rust
            let model_sig: RwSignal<NavModel> = RwSignal::new(model);

            let editing_for_save = Rc::clone(&editing_for_modal);
            let on_save = move |items: Vec<NavItem>| {
                let guard = editing_for_save.borrow();
                let Some(state) = guard.as_ref() else {
                    return;
                };
                match state.session.update_nav(items) {
                    Ok(()) => {
                        nav_save_error.set(None);
                        nav_editor_open.set(false);
                    }
                    Err(e) => nav_save_error.set(Some(e.to_string())),
                }
            };
            let on_cancel = move || nav_editor_open.set(false);
```

**After:**
```rust
            let model_sig: RwSignal<NavModel> = RwSignal::new(model);
            // Fresh on every modal open: staging always starts Unchanged.
            let favicon_change: RwSignal<FaviconChange> =
                RwSignal::new(FaviconChange::Unchanged);

            let editing_for_save = Rc::clone(&editing_for_modal);
            let on_save = move |items: Vec<NavItem>| {
                let guard = editing_for_save.borrow();
                let Some(state) = guard.as_ref() else {
                    return;
                };
                // Favicon first, then nav; a favicon error keeps the modal
                // open and skips the nav write.
                if let Err(e) = favicon_change
                    .get_untracked()
                    .apply_to_session(&state.session)
                {
                    nav_save_error.set(Some(format!("favicon: {e}")));
                    return;
                }
                match state.session.update_nav(items) {
                    Ok(()) => {
                        nav_save_error.set(None);
                        nav_editor_open.set(false);
                    }
                    Err(e) => nav_save_error.set(Some(e.to_string())),
                }
            };
            let on_cancel = move || nav_editor_open.set(false);
```

- [ ] **Step 7: Title + favicon section in the modal `v_stack`** — (mod.rs:418-423):

**Before:**
```rust
            v_stack((
                label(|| "Site settings \u{2014} navigation".to_string())
                    .style(|s| s.font_size(15.).font_weight(Weight::SEMIBOLD)),
                error_line,
                nav_editor::nav_editor_view(model_sig, pages, tags, on_save, on_cancel),
            ))
```

**After:**
```rust
            v_stack((
                label(|| "Site settings".to_string())
                    .style(|s| s.font_size(15.).font_weight(Weight::SEMIBOLD)),
                error_line,
                favicon_section(favicon_change, current_favicon),
                nav_editor::nav_editor_view(model_sig, pages, tags, on_save, on_cancel),
            ))
```

- [ ] **Step 8: Add the `favicon_section` helper** — a standalone function in `mod.rs` (place it near the other private helpers, before `editing_view`):

```rust
/// Favicon block of the Site settings modal: a status line (staged change
/// wins over the on-disk state) plus "Choose file…" / "Remove" buttons.
/// Buttons only stage; the modal's Save applies.
fn favicon_section(
    favicon_change: RwSignal<FaviconChange>,
    current_favicon: Option<String>,
) -> floem::AnyView {
    let status = dyn_container(
        move || favicon_change.get(),
        move |change| {
            let text = change
                .display_label()
                .or_else(|| current_favicon.clone())
                .unwrap_or_else(|| "(none)".to_string());
            label(move || format!("Favicon: {text}"))
                .style(|s| s.font_size(12.).color(Color::rgb8(100, 100, 110)))
                .into_any()
        },
    );

    let choose_btn = button(label(|| "Choose file…".to_string()))
        .action(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Favicon (ico, png, svg)", &["ico", "png", "svg"])
                .pick_file();
            let Some(path) = picked else {
                return; // dialog cancelled
            };
            favicon_change.set(FaviconChange::Set(path));
        })
        .style(|s| s.padding_vert(4.).padding_horiz(10.).font_size(12.));

    let remove_btn = button(label(|| "Remove".to_string()))
        .action(move || favicon_change.set(FaviconChange::Remove))
        .style(|s| {
            s.padding_vert(4.)
                .padding_horiz(10.)
                .font_size(12.)
                .color(Color::rgb8(200, 60, 60))
        });

    let controls = h_stack((choose_btn, remove_btn)).style(|s| s.gap(6.));

    v_stack((status, controls))
        .style(|s| {
            s.gap(4.)
                .padding(8.)
                .border(1.)
                .border_color(Color::rgb8(220, 220, 220))
                .border_radius(4.)
        })
        .into_any()
}
```

- [ ] **Step 9: Run to verify everything compiles and tests pass**

Run: `cargo test -p lopress-editor`
Expected: PASS.

- [ ] **Step 10: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/lopress-editor/src/ui/mod.rs crates/lopress-editor/src/ui/nav_editor.rs
git commit -m "Add favicon section to Site settings modal with staged-on-save"
```

---

### Task 7: Live verification (no code)

**REQUIRED SUB-SKILLS:** `driving-lopress-editor` (launch/probe recipe) and `verifying-lopress-work` (evidence rules — no PASS without command + output).

- [ ] **Step 1:** Scaffold a scratch site (`cargo run --quiet -- new $env:TEMP\lopress-favicon-test`) and drop a small real PNG into it as pick-source (e.g. copy any PNG to `$env:TEMP\pick-me.png`). Never use a real site.
- [ ] **Step 2:** Launch the editor (`cargo run`, background, visible window), poll `/ping`, `/open` the scratch site's `hello.md` by absolute path.
- [ ] **Step 3:** **Convention-file path (no dialog needed):** copy `pick-me.png` to `$env:TEMP\lopress-favicon-test\src\favicon.png`, then trigger a rebuild by making any small edit via `/action` (the save pipeline rebuilds). Verify with evidence:
  - `$site\www\favicon.png` exists;
  - the rendered `$site\www\posts\hello\index.html` (path per scaffold) contains `<link rel="icon" href="/favicon.png">`.
- [ ] **Step 4:** **Dialog rendering:** `/screenshot`; `/click` the sidebar's "Site settings" button; `/screenshot` again and confirm the modal shows the title "Site settings", the `Favicon: favicon.png` status line, and the Choose/Remove buttons. (Block coordinates from the first screenshot.)
- [ ] **Step 5:** **Remove flow:** `/click` Remove, `/click` Save; verify `src\favicon.png` is deleted and after the rebuild `www\favicon.png` is gone and the link tag is absent from the rendered HTML.
- [ ] **Step 6:** **Needs-human handback:** the "Choose file…" button opens a *native* OS file dialog that the control server cannot drive. Report this single check for manual confirmation: click Choose file…, pick an image, Save, and confirm the status line and site update. Everything else must be verified by evidence per the steps above.
- [ ] **Step 7:** Kill the background editor, delete the scratch site. Report results with verbatim command output and screenshot paths (artifacts under `.pi/results/2026-07-XX-favicon-verify/`).

---

## Summary

| Task | Crate(s) | Description |
|------|----------|-------------|
| 1 | `lopress-build` | `Workspace::favicon()` helper with unit tests |
| 2 | `lopress-build` | `hash_favicon()` + `favicon_hash` → favicon changes force full rebuilds |
| 3 | `lopress-build` | Copy favicon to `www/` on full rebuild; duplicate warning |
| 4 | `lopress-theme`, `lopress-build` | `SiteCtx.favicon` + `layout.html` conditional + render tests + docs |
| 5 | `lopress-gui-host` | `Session::favicon()`, `set_favicon()`, `remove_favicon()` |
| 6 | `lopress-editor` | Favicon section in Site settings modal, staged-on-save |
| 7 | — | Live GUI verification with evidence; native-dialog check handed to human |

# Favicon Support — Editor-Configured Favicon via Convention File

**Date:** 2026-07-05
**Author:** Kyle (brainstorm), Claude (spec)
**Status:** Approved design, pre-implementation
**Branch:** `feature/favicon-support`
**Related:** `docs/superpowers/specs/2026-06-12-nav-editor-design.md` (same site-settings modal this extends)

---

## 1. Background

The default theme emits site-wide HTML from `layout.html` in
`crates/lopress-theme/assets/default-theme/templates/layout.html`, which
currently includes nav, meta tags, RSS link, and OG tags — but no favicon
link. Site-level settings (title, base URL, theme) live in
`lopress.toml` and are exposed to templates via `SiteCtx` in
`crates/lopress-theme/src/context.rs`. Navigation is already editable from
the editor's "Site settings" modal (see §8 of the nav-editor spec), and the
build copies `theme.css` to `www/assets/theme.css` on every full rebuild
(see `crates/lopress-build/src/build.rs`'s `write_theme_css` call).

This spec adds **favicon support** so a user can pick a single `.ico`,
`.png`, or `.svg` file from the editor, have it copied into the workspace,
and see a `<link rel="icon">` tag emitted in every page's `<head>`.

---

## 2. Scope

- A **Favicon** section at the top of the existing "Site settings" modal
  (the same modal that edits nav, opened from the sidebar's "Site settings"
  button in `crates/lopress-editor/src/ui/sidebar.rs`).
- The user picks a `.ico`, `.png`, or `.svg` file via a native dialog
  (`rfd::FileDialog`, same crate used for image import and workspace picker).
- The picked file is **copied** to `<workspace>/src/favicon.<ext>` (original
  extension preserved). Presence of that file means favicon enabled; removing
  it disables the favicon. **`lopress.toml` is never written.**
- The build copies the file to `www/favicon.<ext>` and populates
  `SiteCtx.favicon` with the web path (`/favicon.<ext>`).
- `layout.html` emits `{% if site.favicon %}<link rel="icon"
  href="{{ site.favicon }}">{% endif %}` in `<head>`.
- Cache invalidation: favicon add/remove/renamed triggers a full rebuild
  (every page's HTML changes).

### Non-goals

- No ICO generation (no multi-size `.ico` creation).
- No PNG icon-set (`<link rel="icon" sizes="..." type="image/png">`).
- No `apple-touch-icon`.
- No favicon preview thumbnail in the dialog.
- No `lopress.toml` configuration — convention file only.
- No fix for browser favicon caching during live preview.

---

## 3. Discovery Convention (Build Side)

The build uses a **fixed priority order** to discover the favicon in
`src/`:

1. `src/favicon.svg`
2. `src/favicon.png`
3. `src/favicon.ico`

The first file that exists wins. If more than one exists (only possible
through hand-editing), a **build warning** is emitted naming the one that
was used.

The file lives at `src/favicon.<ext>` rather than `src/images/` deliberately:
the image pipeline (in `crates/lopress-build/src/build.rs`'s image walk via
`lopress_assets::process_image`) would WebP-resize it. Page discovery only
walks `src/posts/` and `src/pages/` (see
`crates/lopress-build/src/pages.rs`'s `discover`), so `src/favicon.*` is
inert there.

---

## 4. Components

### 4.1 `lopress-build` — Workspace helper + copy step

A favicon lookup helper is added to `Workspace` (in
`crates/lopress-build/src/site.rs`), returning `Option<(PathBuf, String)>`
where the `PathBuf` is the source path on disk and the `String` is the web
path (e.g. `"/favicon.png"`), or `None` when no favicon is found.

```rust
impl Workspace {
    /// Find the favicon in `src/` by priority order (svg → png → ico).
    /// Returns (source path, web path) or None.
    pub fn favicon(&self) -> Option<(PathBuf, String)>;
}
```

The build step in `build()` (in `crates/lopress-build/src/build.rs`) copies
the favicon to `www/favicon.<ext>` alongside the existing `theme.css` copy
step (the `write_theme_css` call and plugin asset copy inside the `force_full`
block). When `force_full` is true and a favicon is found, it is copied; when
no favicon exists, any stale `www/favicon.*` file from a previous build is
removed.

### 4.2 `SiteCtx` — Template context field

`SiteCtx` (in `crates/lopress-theme/src/context.rs`) gains one field:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub nav: Vec<NavItem>,
    pub posts: Vec<PostSummary>,
    pub favicon: Option<String>,  // NEW: web path like "/favicon.png"
}
```

`lopress-build` populates it from the favicon helper. The default theme's
`layout.html` emits the conditional link tag:

```html
{% if site.favicon %}<link rel="icon" href="{{ site.favicon | safe }}">{% endif %}
```

(`| safe` because Tera entity-escapes `/` on `.html` templates; the value is
program-generated — one of three fixed strings — so this is not an injection
surface.)

This is placed in `<head>`, after the existing `<link rel="stylesheet">` line
and before `{% block extra_head %}`.

`docs/themes.md`'s context table gains a `site.favicon` row:

| Field | Type | Notes |
|---|---|---|
| `site.favicon` | string or null | Web path like `/favicon.png`; null when no favicon |

### 4.3 `Session` — `favicon()`, `set_favicon()`, `remove_favicon()`

`Session` (in `crates/lopress-gui-host/src/session.rs`) gains three methods,
mirroring the existing `import_image` and `update_nav` patterns:

```rust
impl Session {
    /// Current favicon filename (e.g. "favicon.png"), read fresh from disk.
    /// Returns None when no favicon file exists.
    pub fn favicon(&self) -> Option<String>;

    /// Validate extension, copy into `src/`, evict sibling `favicon.*`,
    /// then trigger a rebuild.
    ///
    /// # Errors
    /// Returns `SaveError` on I/O failure or invalid extension.
    pub fn set_favicon(&self, src: &Path) -> Result<(), SaveError>;

    /// Delete `src/favicon.*`, then trigger a rebuild.
    ///
    /// # Errors
    /// Returns `SaveError` on I/O failure.
    pub fn remove_favicon(&self) -> Result<(), SaveError>;
}
```

**`set_favicon` behavior:**
1. Validate the extension is one of `ico`, `png`, `svg` (reject otherwise,
   even though the dialog filters).
2. Find and delete any existing `src/favicon.*` file (invariant: at most one
   `favicon.*` exists at all times).
3. Copy the picked file to `src/favicon.<ext>`.
4. Call `self.rebuild()`.

**`remove_favicon` behavior:**
1. Delete any `src/favicon.*` file.
2. Call `self.rebuild()`.

**`favicon` behavior:**
1. Scan `src/` for `favicon.svg`, `favicon.png`, `favicon.ico` in priority
   order.
2. Return the base filename (e.g. `"favicon.png"`) or `None`.

### 4.4 Editor UI — Favicon section in the Site settings modal

The modal is wired in `crates/lopress-editor/src/ui/mod.rs` (`editing_view`)
and built by `crates/lopress-editor/src/ui/nav_editor.rs`
(`nav_editor_view`).

**Modal title change:** The label changes from `"Site settings — navigation"`
to `"Site settings"`.

**Favicon section (new, appears above Navigation):**
- Shows the effective favicon state: the staged change if any, otherwise the
  current filename (from `session.favicon()`), otherwise `"(none)"`.
- **"Choose file…"** button — opens `rfd::FileDialog` filtered to
  `ico`, `png`, `svg` (same pattern as `on_insert_image` in `mod.rs`) and
  stages `FaviconChange::Set(path)`. Nothing touches the session yet.
- **"Remove"** button — stages `FaviconChange::Remove`. Nothing touches the
  session yet.
- Errors surface on Save via the modal's existing error line
  (`nav_save_error` signal) and keep the modal open.

**Staged-on-save semantics:** The working model holds a favicon change enum:

```rust
enum FaviconChange {
    Unchanged,
    Set(PathBuf),   // staged path from the picker
    Remove,
}
```

On Save:
1. Apply the favicon change (if `Set`, call `session.set_favicon(path)`;
   if `Remove`, call `session.remove_favicon()`).
2. Apply the nav change via `session.update_nav(items)`.
3. Close the modal.

Cancel discards the working model and closes — identical semantics to the
existing nav rows.

`NavModel` stays nav-only. The favicon staging lives in a separate
`RwSignal<FaviconChange>` created in `mod.rs` alongside the `NavModel`
signal each time the modal opens (so it always starts `Unchanged`), and is
passed into the view. The `FaviconChange` enum and its transition logic are
defined in `nav_editor.rs` as pure data with unit tests, matching how
`NavModel` is tested without Floem.

---

## 5. Cache Invalidation

Adding, removing, or renaming the favicon changes every page's HTML output,
so it must trigger a full rebuild. This is handled through the theme hash
in `crates/lopress-build/src/cache.rs`.

The favicon's **filename and presence** are appended to the `theme_hash`
items list in `hash_theme()` (the same list that carries the six template
names + `css`). When the favicon file exists, its filename is added as a
key with its bytes as the value; when it doesn't exist, a sentinel entry
`"favicon: none"` is added. This ensures:

- Adding a favicon → hash changes → full rebuild.
- Removing a favicon → hash changes → full rebuild.
- Renaming the extension → hash changes → full rebuild.
- Changing the file contents → hash changes → full rebuild.
- No favicon → sentinel → stable hash when nothing changes.

---

## 6. Error Handling

- **Copy/IO failures** → `SaveError::Io` surfaced on the modal's error line,
  modal stays open.
- **Invalid extension** (if the user somehow bypasses the filter) →
  `SaveError` with a descriptive message.
- **Rebuild failure** → the session's `BuildStatus` reflects the error; the
  footer already displays build status.

---

## 7. Known Limitations

**Browser favicon caching.** Browsers cache favicons aggressively and do not
reload them on standard SSE page reloads. Swapping the favicon during live
preview will not reliably show the new icon without a hard reload. This is
documented but **not solved** in this spec.

---

## 8. Testing

### 8.1 Working-model unit tests (`nav_editor.rs`)

- Staging `Set(path)`, `Remove`, and `Unchanged` transitions.
- Save applies favicon change then nav change in order.
- Cancel discards the working model.

### 8.2 Session integration tests (`session.rs`)

- `set_favicon` copies the file to `src/` and returns `Ok`.
- Replacing a `.png` with an `.ico` evicts the old `favicon.png` (at-most-one
  invariant).
- `remove_favicon` deletes the file and returns `Ok`.
- Invalid extension (e.g. `.jpg`) is rejected with an error.
- `favicon()` returns the correct filename when present, `None` when absent.

### 8.3 Build tests (`build.rs` / `cache.rs`)

- Favicon is copied to `www/`; `SiteCtx.favicon` is populated.
- Rendered page HTML contains `<link rel="icon" href="/favicon.<ext>">`.
- No favicon → no tag, `www/` is clean of favicon files.
- Multiple favicons present → warning emitted naming the one used.
- Cache: favicon add/remove invalidates cached pages (pages regenerate on
  next build).

### 8.4 Template test (default theme)

- `layout.html` renders the conditional link when `site.favicon` is set.
- `layout.html` omits the tag when `site.favicon` is `None`.

---

## 9. Implementation Order

1. **`lopress-build` — `Workspace::favicon()` helper** with unit tests.
2. **`lopress-build` — copy step in `build()`** (copy to `www/`, remove
   stale files, emit warning on duplicates).
3. **`lopress-theme` — `SiteCtx.favicon` field** + `layout.html` conditional
   + `docs/themes.md` context table update.
4. **`lopress-build` — cache invalidation** in `hash_theme()` (append
   favicon filename/presence to the theme hash items).
5. **`lopress-gui-host` — `Session` methods** (`favicon`, `set_favicon`,
   `remove_favicon`) with integration tests.
6. **`lopress-editor` — Favicon section** in the Site settings modal
   (`mod.rs` wiring + `nav_editor.rs` model/view changes).
7. **Staged-on-save** integration: favicon change applied before nav change
   on Save.
8. **End-to-end tests**: working-model unit tests, session integration tests,
   build tests.

---

## 10. Decisions

### Serve as-is vs. generate an icon set

Chosen: **as-is** (single file, single `<link rel="icon">` tag). Rejected
generating `favicon.ico` + PNG sizes + `apple-touch-icon` — that would add
ICO encoding and resize logic to `lopress-assets` for marginal benefit.

### Convention file (`src/favicon.<ext>`) vs. `lopress.toml` field

Chosen: **convention file** (presence = enabled). Rejected a `[site] favicon`
config field — the editor has no programmatic `lopress.toml` writer today,
and adding one (`toml_edit` to preserve hand-written comments) is
disproportionate. Mirrors the `nav.toml` precedent of not touching user
config.

### Template-driven tag vs. build/serve-side HTML injection

Chosen: **`SiteCtx.favicon` + conditional in the theme layout**. Rejected
injecting the tag into rendered HTML — it smuggles content through the
injection layer, and custom themes are better served by a documented context
field.

### Staged-on-save vs. apply-on-pick in the dialog

Chosen: **staged, applied on Save** — consistent with the nav editor's
commit-on-save semantics and makes Cancel meaningful.

### Location `src/favicon.<ext>`

Chosen: **`src/`** (not `src/images/` and not the workspace root).
`src/images/` would hit the WebP resize pipeline; the workspace root keeps
user content under `src/`.

---

## 11. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no
placeholders.

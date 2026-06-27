# Navigation Editor — GUI for Site Nav with Page/Tag Pickers

**Date:** 2026-06-12
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Supersedes:** `2026-06-01-nav-editor-design.md`
**Related:** `docs/architecture.md` (§5.5 lopress-build config, §5.8 gui-host session, §7.4 editor UI)

---

## 1. Background

Site navigation is already configurable in `lopress.toml` under `[site.nav]` as an array
of `{ label, href }` items (`crates/lopress-build/src/site.rs`: `Nav` / `NavItem`), and
the default theme renders it in `layout.html`:

```jinja
<nav class="site-nav">
  {% for item in site.nav %}<a href="{{ item.href }}">{{ item.label }}</a>{% endfor %}
</nav>
```

What's missing is a way to edit it from the editor — today you must hand-edit
`lopress.toml`. This spec adds a **GUI navigation editor**: a panel to add / remove /
reorder nav links (label + href), with convenience **pickers** that fill the `href` from
an existing workspace page or an existing tag, persisted back to a dedicated `nav.toml`
file without touching `lopress.toml`.

---

## 2. Scope

- A "Site settings" entry point in the sidebar that opens a nav-editor modal.
- A nav-editor panel: editable list of rows (label + href), add / remove / reorder.
- A **page picker** (lists workspace pages → fills `href` with `/<slug>/`) and a **tag
  picker** (lists existing tags → fills `href` with `/tags/<tag>/`).
- Persistence to a new `nav.toml` file (machine-owned, written via `toml` serialization)
  at the workspace root, sibling to `lopress.toml`.
- A rebuild after save so the live preview reflects the new nav.
- `nav.toml` is the **only** nav source. `[site.nav]` in `lopress.toml` is no longer
  read; if present it is ignored, and a migration warning tells the user to move the
  items to `nav.toml` and delete the old block.

### Non-goals

- No nested/dropdown navigation (flat list only).
- No reordering by drag-and-drop (explicit up/down controls only).
- No external-link validation beyond non-empty label + href.
- No editing of other `lopress.toml` settings (title, base_url, theme) in this spec —
  the panel is nav-only, though it is named "Site settings" to leave room to grow.
- No automatic nav entries — every link is explicit.
- The tool never edits `lopress.toml`; a leftover `[site.nav]` block is never
  auto-migrated or auto-deleted — only warned about.

---

## 3. Nav File

A new file `nav.toml` at the workspace root, sibling to `lopress.toml`, owned by the
editor (machine-written, no comment preservation needed). It reuses the existing `Nav` /
`NavItem` serde types from `crates/lopress-build/src/site.rs`:

```toml
items = [
  { label = "Home", href = "/" },
  { label = "About", href = "/about/" },
]
```

The file's top-level key is `items` (a `Vec<NavItem>`), matching the `Nav` struct's
`items` field. This is the file the editor always reads and writes.

---

## 4. Loading

`Workspace::load` (`crates/lopress-build/src/site.rs`) is extended to read `nav.toml`
at the workspace root:

1. If `nav.toml` exists, deserialize it into `Nav` and use it as the site nav.
2. If it does not exist, nav is empty (`Nav { items: [] }`, the default).

`[site.nav]` in `lopress.toml` is **no longer read**. The `nav` field is removed from
the `Site` struct; `Nav`/`NavItem` remain as the `nav.toml` types. `SiteConfig` does not
use `deny_unknown_fields`, so an old `lopress.toml` containing `[site.nav]` still parses
fine — the table is simply ignored. Nav becomes its own field on `Workspace`
(`pub nav: Nav`), and `build()` reads `ws.nav.items` when assembling `SiteCtx`.

### Migration warning

So existing sites don't silently lose their nav, `Workspace::load` peeks at the parsed
`lopress.toml` for a leftover `site.nav` key (via the raw `toml::Value`, since the typed
struct no longer has the field). If found, a warning is produced: "`[site.nav]` in
`lopress.toml` is no longer supported and is ignored — move the items to `nav.toml` and
delete the old block." `Workspace` gains a `warnings: Vec<String>` field populated during
`load`; `build()` copies it into `BuildReport.warnings` (see §5) and also logs it to
stderr. The warning never blocks the build.

### Cache invalidation

Incremental builds skip pages unless the config hash changes, and `cache::hash_config`
currently hashes only the raw bytes of `lopress.toml`. It is extended to also hash the
bytes of `nav.toml` when present (with a separator so presence/absence of the file
changes the hash). Without this, saving nav from the GUI would not invalidate cached
pages and the old nav would remain baked into previously rendered pages.


---

## 5. Build Report Warnings

`BuildReport` (`crates/lopress-build/src/build.rs`) gains a field:

```rust
pub struct BuildReport {
    pub pages_written: usize,
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
    pub warnings: Vec<String>,  // NEW
}
```

The migration warning from §4 is appended to this vector. The editor surfaces warnings
in the same place build failures are already surfaced (the build status area).

---

## 6. Writing

The editor always writes to `nav.toml` (never to `lopress.toml`). The writer is a new
function in `crates/lopress-build/src/site.rs`:

```rust
/// Serialize `items` to TOML and write atomically to `nav.toml` at `root`.
pub fn write_nav(root: &Path, items: &[NavItem]) -> Result<(), BuildError>;
```

Implementation: serialize `items` with the `toml` crate (already a workspace dependency,
`toml = "0.8"`, supports serialization) and write `nav.toml` atomically (temp file +
rename, mirroring `Session::save`'s `atomic_write`). Zero new dependencies.

- Items with an empty `label` or empty `href` are dropped before writing (the UI also
  prevents adding them — see §8).
- An empty `items` list writes an empty `items = []` array.

The first save from the GUI creates `nav.toml`. A leftover `[site.nav]` in
`lopress.toml` triggers the §4 migration warning until the user deletes it by hand.
The tool never edits `lopress.toml`. Note that an existing site's `[site.nav]` items do
**not** appear in the panel (the old block is not read), so migrating means re-entering
the links in the GUI — or hand-moving the items into `nav.toml` — and deleting the old
block.

---

## 7. Session API (lopress-gui-host)

`Session` (`crates/lopress-gui-host/src/session.rs`) holds an `Arc<Workspace>` whose
`config` is the snapshot loaded at open. Crucially, `lopress_build::build()` re-loads the
workspace config **from disk** on every build, so writing `nav.toml` and then calling
`rebuild()` is sufficient for the live preview to pick up new nav — the stale in-memory
snapshot does not block the build.

Two methods are added:

```rust
/// Current nav items, read fresh from nav.toml on disk so repeated
/// edits in one session reflect the latest saved state. Empty when
/// the file doesn't exist.
pub fn nav_items(&self) -> Vec<lopress_build::NavItem>;

/// Write nav items to nav.toml, then trigger a rebuild + SSE reload.
///
/// # Errors
/// Returns an error if nav.toml can't be written.
pub fn update_nav(&self, items: Vec<lopress_build::NavItem>) -> Result<(), SaveError>;
```

`nav_items()` re-reads `nav.toml` rather than returning the open-time snapshot, so the
panel always shows current state. `update_nav()` calls the
new config-writer (§6), then `self.rebuild()`.

---

## 8. Editor UI

### Entry point

The sidebar (`crates/lopress-editor/src/ui/sidebar.rs`) gains a "Site settings" control in
its header (near the workspace name) — a small gear button / labeled link. Activating it
sets an `RwSignal<bool>` (`nav_editor_open`) owned by the editing view.

### Modal

`crates/lopress-editor/src/ui/mod.rs`'s editing view conditionally renders a centered
modal overlay when `nav_editor_open` is true (a dimmed backdrop + a centered panel). A new
module `crates/lopress-editor/src/ui/nav_editor.rs` builds the panel.

> **Floem overlay caveat.** Per the known floem 0.2 hit-test gotcha (absolutely-positioned
> children that overflow above/left of their parent with a negative inset are painted but
> not hit-tested), the modal must be laid out **within** its parent's bounds — a centered
> panel with non-negative insets — so its buttons remain clickable. Do not position it via
> negative margins.

### Panel contents

The panel holds a working copy of the nav items in editor state (e.g.
`RwSignal<Vec<NavRow>>` where `NavRow { label: RwSignal<String>, href: RwSignal<String> }`),
initialized from `session.nav_items()` when opened.

Per row:
- a **label** text input,
- an **href** text input,
- **↑ / ↓** buttons to reorder (disabled at the ends),
- a **✕** button to remove the row.

Below the list:
- an **"Add link"** button appending an empty row,
- a **"Link to page ▾"** picker and a **"Link to tag ▾"** picker.

Floem 0.2 has no stock dropdown (the plugin attr form notes this), so each picker opens a
small popup list of buttons — pages by title, tags by name — using the same pattern as
`attr_select` in `crates/lopress-editor/src/ui/blocks/plugin.rs`. Choosing a target fills
the **focused/last** row's `href` (`/<slug>/` for a page, `/tags/<tag>/` for a tag) and,
when that row's label is empty, pre-fills the label with the page title / tag name. The
page and tag choices come from `session.workspace().pages` (with the new `slug`) and
`session.workspace().tags`.

Footer:
- **Save** — drops empty rows, collects `Vec<NavItem>`, calls `session.update_nav(items)`,
  closes the modal. A write error is surfaced inline in the panel (reusing the editor's
  error-display style) and the modal stays open.
- **Cancel** — discards the working copy and closes the modal.

The panel performs light validation: a row with an empty label **or** empty href is
visually flagged and excluded from the save (it is not written).

---

## 9. Pickers Need Pages and Tags

The pickers draw from the workspace summary:

- **Pages.** `WorkspaceSummary.pages` already lists pages as `DocumentRef`, but
  `DocumentRef` carries only `path` + `title`, not the slug used to build URLs.
  `DocumentRef` gains a `slug: String` field, computed during `scan_dir` the same way the
  build computes it: front-matter `slug` if present, else the file stem. (This matches
  `discover()` in `crates/lopress-build/src/pages.rs` and keeps the picker's href
  correct.) A page's href is `/<slug>/`. (Posts use `/posts/<slug>/`, but the nav picker
  targets pages; linking to a specific post is out of scope — use a raw href.)
- **Tags.** Tags are not scanned today. `scan_dir`/`scan_workspace` is extended to collect
  the union of post front-matter `tags`, exposed as `WorkspaceSummary.tags: Vec<String>`
  (sorted, de-duplicated). A tag's href is `/tags/<tag>/`.

---

## 10. Scaffolding

`lopress new` (`crates/lopress-build/src/scaffold.rs`) is updated to write the default
Home / About nav to `nav.toml` instead of embedding it in `lopress.toml` under
`[site.nav]`.

```toml
# Written to nav.toml by `lopress new`:
items = [
  { label = "Home", href = "/" },
  { label = "About", href = "/about/" },
]
```


---

## 11. Testing

### lopress-build

- `nav.toml` round-trip: write via `write_nav`, `Workspace::load` reflects the new nav.
- No `nav.toml` → nav is empty.
- An old `lopress.toml` containing `[site.nav]` still parses; its items are ignored and
  the migration warning lands in `Workspace.warnings` → `BuildReport.warnings`.
- Cache invalidation: changing `nav.toml` changes `hash_config`'s result (and so forces a
  full rebuild); creating or deleting the file also changes it.
- Scaffold: `lopress new` output contains `nav.toml` and no `[site.nav]` in `lopress.toml`.

### lopress-gui-host

- `DocumentRef.slug` is computed from front-matter `slug` when present, else the file
  stem.
- `WorkspaceSummary.tags` is the sorted, de-duplicated union of post tags.
- `nav_items()` reflects a nav written by `update_nav` earlier in the same session
  (reads from disk).
- `update_nav` writes the file and triggers a rebuild (assert the file content; the
  rebuild is a background thread, consistent with existing `rebuild` tests/usage).

### Editor

- The nav-editor working model add/remove/reorder operations produce the expected item
  list (unit-test the pure list manipulation, independent of Floem views).
- A page pick fills `href` with `/<slug>/`; a tag pick fills `/tags/<tag>/`.
- Empty rows are excluded from the saved list.

### End-to-end (control interface)

Via the `127.0.0.1:7878` control server: open a workspace, open Site settings, add a link
(label + href, or via the page picker), save, and confirm `nav.toml` now contains the nav
item and the rebuilt pages render the link in the header nav. (The text-entry portions are
drivable via the control input API; opening the native modal and clicking are assertable
through document/UI state where exposed, with genuinely real-mouse checks handed back per
the control workflow.)

---

## 12. Implementation Order

1. `lopress-build`: implement `write_nav` (TOML serialization + atomic write) with unit
   tests.
2. `lopress-build`: remove `nav` from `Site`; add `nav: Nav` + `warnings: Vec<String>`
   to `Workspace` (loaded from `nav.toml`, with the `[site.nav]` migration warning); add
   `warnings` to `BuildReport` and populate it in `build()` (which now reads `ws.nav`);
   extend `cache::hash_config` to cover `nav.toml`.
3. `lopress-build`: update `scaffold.rs` to write `nav.toml` instead of `[site.nav]`.
4. `lopress-gui-host`: add `slug` to `DocumentRef` and `tags` to `WorkspaceSummary` (scan
   changes); add `nav_items()` and `update_nav()`.
5. `lopress-editor`: `nav_editor.rs` panel + working model; sidebar "Site settings" entry
   point + `nav_editor_open` signal; modal wiring in the editing view.
6. Page/tag pickers (popup-button pattern) wired to `session.workspace()`.
7. Save path → `session.update_nav` → rebuild; inline error handling; Cancel.
8. Tests (build round-trip / empty-nav / migration-warning / scaffold, gui-host scan/nav
   methods, editor working-model + pickers, e2e).

---

## 13. Decisions

### Separate machine-owned `nav.toml` instead of surgical `toml_edit` edits of `lopress.toml`

Chosen because the user wants no new dependencies. A file the editor owns outright has no
user comments or formatting to preserve, so plain `toml` serialization (already a workspace
dependency at `toml = "0.8"`) suffices. Cost is a new file at the workspace root; benefit
is zero new dependencies and no risk of corrupting user formatting.

Rejected: `toml_edit` surgical write (new dependency on `toml_edit`); re-serializing all
of `SiteConfig` (destroys user comments and formatting in `lopress.toml`).

### `[site.nav]` support dropped entirely, with a migration warning

Originally the design kept `[site.nav]` as a fallback (`nav.toml` wins + warning when both
exist); the user then decided (2026-06-12) not to keep the old path at all. `nav.toml` is
the only nav source. To avoid silently losing an existing site's nav, `Workspace::load`
detects a leftover `[site.nav]` table and emits a migration warning via
`BuildReport.warnings` (also logged to stderr); the build never fails over it and the tool
never edits `lopress.toml`. Rejected: silent ignoring (nav would just vanish with no
explanation) and erroring (blocks builds on a cosmetic leftover).

### `lopress new` scaffolds `nav.toml`

New sites get the nav file from day one, so `[site.nav]` only ever appears in
pre-migration sites, where it triggers the warning above.

### Rebuild-from-disk rather than mutating the shared `Arc<Workspace>`

`Session.workspace` is an immutable `Arc` shared with background build/watch threads, and
`build()` already reloads config from disk each run. Writing the file + `rebuild()` is the
simplest correct path; `nav_items()` re-reads from disk so the panel never shows stale
state. Rejected adding interior mutability to the shared workspace config (more surface,
no benefit since the build re-reads anyway).

### `DocumentRef` gains `slug`; tags added to the summary

The pickers need correct hrefs. Computing the slug the same way the build does (front
matter `slug` || file stem) keeps page links accurate, and surfacing the post-tag union
lets the tag picker offer real targets. Rejected hardcoding `href` to the file stem
(wrong when a page sets an explicit front-matter slug).

### Flat nav, explicit links, up/down reordering

Matches the existing `[site.nav]` data model (a flat array) and the theme's flat render.
Nested menus, drag-reorder, and auto-generated entries are deferred (YAGNI).

### "Site settings" naming for a nav-only panel

The panel only edits nav now, but is named and placed as "Site settings" so future
site-level settings (title, theme) can join it without relocating the entry point.

---

## 14. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no placeholders.

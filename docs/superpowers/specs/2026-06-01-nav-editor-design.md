# Navigation Editor — GUI for Site Nav with Page/Tag Pickers

**Date:** 2026-06-01
**Author:** Kyle
**Status:** spec — ready for implementation planning
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
an existing workspace page or an existing tag, persisted back to `[site.nav]` without
clobbering the rest of `lopress.toml`.

---

## 2. Scope

- A "Site settings" entry point in the sidebar that opens a nav-editor modal.
- A nav-editor panel: editable list of rows (label + href), add / remove / reorder.
- A **page picker** (lists workspace pages → fills `href` with `/<slug>/`) and a **tag
  picker** (lists existing tags → fills `href` with `/tags/<tag>/`).
- Persistence to `[site.nav]` in `lopress.toml` via a surgical `toml_edit` write that
  preserves all other keys, ordering, and comments.
- A rebuild after save so the live preview reflects the new nav.

### Non-goals

- No nested/dropdown navigation (flat list only).
- No reordering by drag-and-drop (explicit up/down controls only).
- No external-link validation beyond non-empty label + href.
- No editing of other `lopress.toml` settings (title, base_url, theme) in this spec —
  the panel is nav-only, though it is named "Site settings" to leave room to grow.
- No automatic nav entries — every link is explicit.

---

## 3. Session API (lopress-gui-host)

`Session` (`crates/lopress-gui-host/src/session.rs`) holds an `Arc<Workspace>` whose
`config` is the snapshot loaded at open. Crucially, `lopress_build::build()` re-loads the
workspace config **from disk** on every build, so writing `lopress.toml` and then calling
`rebuild()` is sufficient for the live preview to pick up new nav — the stale in-memory
snapshot does not block the build.

Two methods are added:

```rust
/// Current nav items, read fresh from lopress.toml on disk so repeated edits
/// in one session reflect the latest saved state.
pub fn nav_items(&self) -> Vec<lopress_build::NavItem>;

/// Surgically rewrite `[site.nav]` in lopress.toml (preserving other keys and
/// comments), then trigger a rebuild + SSE reload.
///
/// # Errors
/// Returns an error if lopress.toml can't be read, parsed, or written.
pub fn update_nav(&self, items: Vec<lopress_build::NavItem>) -> Result<(), SaveError>;
```

`nav_items()` re-reads and parses `lopress.toml` (cheap) rather than returning the
open-time snapshot, so the panel always shows current state. `update_nav()` calls the new
config-writer (§4), then `self.rebuild()`.

### Pickers need pages and tags

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

## 4. Persistence: surgical `toml_edit` write

`SiteConfig` derives `Serialize`, but re-serializing the whole struct would drop comments,
reorder keys, and normalize formatting of the user's `lopress.toml`. Instead, a new helper
in `crates/lopress-build/src/site.rs` performs a targeted edit:

```rust
/// Rewrite the `[site.nav]` items array in `lopress.toml` at `root`, leaving
/// every other key, table, and comment untouched. Creates `[site.nav]` if absent.
pub fn write_nav(root: &Path, items: &[NavItem]) -> Result<(), BuildError>;
```

Implementation: read `lopress.toml`, parse with `toml_edit` into a mutable `Document`,
replace `site.nav.items` (an array-of-tables of `{ label, href }`) with the new items,
and write the result back atomically (temp file + rename, mirroring `Session::save`'s
`atomic_write`). `toml_edit` is added as a dependency of `lopress-build`.

- If `[site]` exists but `nav` does not, the `nav.items` array is inserted under it.
- Items with an empty `label` or empty `href` are dropped before writing (the UI also
  prevents adding them — see §5).
- An empty `items` list writes an empty array (the nav simply renders nothing).

`Session::update_nav` calls `write_nav(&self.workspace.root, &items)` and then
`self.rebuild()`.

---

## 5. Editor UI

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

## 6. Testing

### lopress-build

- `write_nav` adds `[site.nav]` items to a config that had none, preserving an existing
  comment and unrelated keys (`title`, `base_url`, a `[build]` table).
- `write_nav` replaces existing nav items without touching other tables/comments.
- `write_nav` with an empty slice writes an empty items array and the config still parses
  back via `SiteConfig`/`toml::from_str`.
- Round-trip: after `write_nav`, `Workspace::load` reflects the new nav.

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
(label + href, or via the page picker), save, and confirm `lopress.toml` now contains the
nav item and the rebuilt pages render the link in the header nav. (The text-entry portions
are drivable via the control input API; opening the native modal and clicking are
assertable through document/UI state where exposed, with genuinely real-mouse checks handed
back per the control workflow.)

---

## 7. Implementation Order

1. `lopress-build`: add `toml_edit` dep; implement `write_nav` (surgical `[site.nav]`
   edit + atomic write) with unit tests.
2. `lopress-gui-host`: add `slug` to `DocumentRef` and `tags` to `WorkspaceSummary` (scan
   changes); add `nav_items()` and `update_nav()`.
3. `lopress-editor`: `nav_editor.rs` panel + working model; sidebar "Site settings"
   entry point + `nav_editor_open` signal; modal wiring in the editing view.
4. Page/tag pickers (popup-button pattern) wired to `session.workspace()`.
5. Save path → `session.update_nav` → rebuild; inline error handling; Cancel.
6. Tests (build write_nav, gui-host scan/nav methods, editor working-model + pickers,
   e2e).

---

## 8. Decisions

### Surgical `toml_edit` write, not `SiteConfig` re-serialization

Re-serializing the whole `SiteConfig` would discard the user's comments, key ordering, and
formatting in `lopress.toml`. A targeted `toml_edit` rewrite of only `[site.nav]` keeps the
file the user's, touching only the nav. Cost is one new dependency on `lopress-build`.

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

## 9. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no placeholders.

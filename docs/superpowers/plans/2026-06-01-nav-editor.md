# Navigation Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GUI panel to add/remove/reorder site nav links (label + href), with page and tag pickers, persisting to `[site.nav]` in `lopress.toml` and rebuilding the live preview.

**Architecture:** A surgical `toml_edit` write in `lopress-build` preserves the rest of `lopress.toml`. `lopress-gui-host` exposes `Session::nav_items()`/`update_nav()` and surfaces page slugs + tags for the pickers. The editor adds a "Site settings" entry in the sidebar that opens a centered modal (`nav_editor.rs`) whose Save calls `update_nav` → rebuild. `build()` re-reads config from disk, so write + rebuild is sufficient.

**Tech Stack:** Rust, `toml_edit`, Floem (modal + inputs), the workspace's strict clippy lints (`AGENTS.md`).

**Spec:** `docs/superpowers/specs/2026-06-01-nav-editor-design.md`

> **Gate:** run `bash scripts/check.sh` before declaring done.
>
> **Floem overlay caveat:** per the known floem 0.2 hit-test gotcha, the modal must be laid out **within** its parent bounds (centered, non-negative insets) or its buttons won't receive clicks. No negative margins.

---

## Task 1: `write_nav` — surgical `[site.nav]` rewrite

**Files:**
- Modify: `Cargo.toml` (root — add `toml_edit` to `[workspace.dependencies]`)
- Modify: `crates/lopress-build/Cargo.toml` (add `toml_edit`)
- Modify: `crates/lopress-build/src/site.rs` (`write_nav`)
- Test: `crates/lopress-build/src/site.rs`

- [ ] **Step 1: Add the dependency**

In the root `Cargo.toml` `[workspace.dependencies]`, add (use the current published version):

```toml
toml_edit = "0.22"
```

In `crates/lopress-build/Cargo.toml` `[dependencies]`, add:

```toml
toml_edit = { workspace = true }
```

- [ ] **Step 2: Write the failing tests**

Add to the `tests` module in `crates/lopress-build/src/site.rs`:

```rust
#[test]
fn write_nav_preserves_other_keys_and_comments() {
    let d = TempDir::new().unwrap();
    let path = d.path().join("lopress.toml");
    std::fs::write(
        &path,
        "# my site\n[site]\ntitle = \"S\"\nbase_url = \"https://e.com\"\n\n[build]\nimage_variants = [400, 800]\n",
    )
    .unwrap();
    let items = vec![
        NavItem { label: "Home".into(), href: "/".into() },
        NavItem { label: "Series".into(), href: "/tags/series/".into() },
    ];
    write_nav(d.path(), &items).unwrap();

    let back = std::fs::read_to_string(&path).unwrap();
    assert!(back.contains("# my site"), "comment preserved");
    assert!(back.contains("image_variants"), "other table preserved");
    // Re-parse via SiteConfig to confirm the nav landed.
    let ws = Workspace::load(d.path()).unwrap();
    assert_eq!(ws.config.site.nav.items.len(), 2);
    assert_eq!(ws.config.site.nav.items[0].label, "Home");
    assert_eq!(ws.config.site.nav.items[1].href, "/tags/series/");
}

#[test]
fn write_nav_replaces_existing_items() {
    let d = TempDir::new().unwrap();
    let path = d.path().join("lopress.toml");
    std::fs::write(
        &path,
        "[site]\ntitle = \"S\"\nbase_url = \"https://e.com\"\n\n[[site.nav.items]]\nlabel = \"Old\"\nhref = \"/old/\"\n",
    )
    .unwrap();
    write_nav(d.path(), &[NavItem { label: "New".into(), href: "/new/".into() }]).unwrap();
    let ws = Workspace::load(d.path()).unwrap();
    assert_eq!(ws.config.site.nav.items.len(), 1);
    assert_eq!(ws.config.site.nav.items[0].label, "New");
}

#[test]
fn write_nav_empty_clears_items() {
    let d = TempDir::new().unwrap();
    let path = d.path().join("lopress.toml");
    std::fs::write(
        &path,
        "[site]\ntitle = \"S\"\nbase_url = \"https://e.com\"\n\n[[site.nav.items]]\nlabel = \"Old\"\nhref = \"/old/\"\n",
    )
    .unwrap();
    write_nav(d.path(), &[]).unwrap();
    let ws = Workspace::load(d.path()).unwrap();
    assert!(ws.config.site.nav.items.is_empty());
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p lopress-build write_nav`
Expected: FAIL — `write_nav` undefined.

- [ ] **Step 4: Implement `write_nav`**

In `crates/lopress-build/src/site.rs`:

```rust
use std::path::Path;

/// Rewrite the `[site.nav]` items array in `lopress.toml` at `root`, leaving
/// every other key, table, and comment untouched. Creates the `site.nav.items`
/// array if absent. Writes atomically (temp file + rename).
///
/// # Errors
/// Returns `BuildError` if `lopress.toml` is missing, unparseable, or unwritable.
pub fn write_nav(root: &Path, items: &[NavItem]) -> Result<(), BuildError> {
    let path = root.join("lopress.toml");
    let src = std::fs::read_to_string(&path)?;
    let mut doc = src
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| BuildError::Config(format!("lopress.toml: {e}")))?;

    // Build the array-of-tables for site.nav.items.
    let mut arr = toml_edit::ArrayOfTables::new();
    for item in items {
        let mut t = toml_edit::Table::new();
        t["label"] = toml_edit::value(item.label.clone());
        t["href"] = toml_edit::value(item.href.clone());
        arr.push(t);
    }

    // Ensure [site] exists and is a table; set its `nav.items`.
    let site = doc["site"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(site_tbl) = site.as_table_mut() else {
        return Err(BuildError::Config("lopress.toml: [site] is not a table".into()));
    };
    let nav = site_tbl
        .entry("nav")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(nav_tbl) = nav.as_table_mut() else {
        return Err(BuildError::Config("lopress.toml: [site.nav] is not a table".into()));
    };
    nav_tbl["items"] = toml_edit::Item::ArrayOfTables(arr);

    // Atomic write.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
```

(Confirm the `toml_edit` API names against the pinned version: `DocumentMut`, `ArrayOfTables`, `value()`, `Item::ArrayOfTables`, `Table::entry`/`or_insert`. These are the 0.22 names; if the resolved version differs, adapt. `BuildError::Config` and `BuildError: From<std::io::Error>` already exist — confirm the `io::Error` conversion via the `?` on `read_to_string`/`write`/`rename`, which other functions in this crate rely on.)

- [ ] **Step 5: Run them to verify they pass**

Run: `cargo test -p lopress-build write_nav`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/lopress-build/Cargo.toml crates/lopress-build/src/site.rs
git commit -m "feat(build): surgical write_nav for lopress.toml [site.nav]"
```

---

## Task 2: Surface page slugs and tags from the workspace scan

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs` (`DocumentRef`, `WorkspaceSummary`, `scan_dir`, `scan_workspace`)
- Test: `crates/lopress-gui-host/tests/session_integration.rs` (or a unit test in session.rs)

- [ ] **Step 1: Write the failing test**

In `crates/lopress-gui-host/tests/session_integration.rs` (follow the file's existing workspace-setup harness), add a test that opens a workspace with a couple of posts (with front-matter slugs/tags) and asserts the summary exposes slugs and the de-duplicated tag union. If a unit test is easier given the harness, add it in `session.rs` against a temp workspace. Assertions:

```rust
// summary.pages[i].slug == front-matter slug or file stem
// summary.tags == sorted unique union of post front-matter tags
```

- [ ] **Step 2: Add the fields**

In `crates/lopress-gui-host/src/session.rs`:

```rust
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,
    pub title: String,
    pub slug: String,
    pub is_draft: bool,
    pub has_parse_error: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,
    pub posts: Vec<DocumentRef>,
    pub pages: Vec<DocumentRef>,
    pub tags: Vec<String>,
}
```

- [ ] **Step 3: Compute slug + tags in the scan**

Update `scan_dir` to compute `slug` (front-matter slug, else file stem) and to return the parsed front-matter tags so `scan_workspace` can union them. Simplest: have `scan_dir` return `Vec<DocumentRef>` (slug filled) and separately collect tags in `scan_workspace` by parsing posts. To avoid double-parsing, change `scan_dir` to also yield tags, e.g. return `(Vec<DocumentRef>, Vec<String>)` or compute slug+tags inline. Concretely, in the `map` closure where `DocumentRef` is built from a parsed `doc`:

```rust
Ok(Ok(doc)) => {
    let slug = doc.front_matter.slug.clone().unwrap_or_else(|| stem(&path));
    DocumentRef {
        title: doc.front_matter.title.clone().unwrap_or_else(|| stem(&path)),
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
```

Then in `scan_workspace`, collect tags from posts:

```rust
fn scan_workspace(ws: &Workspace) -> WorkspaceSummary {
    let posts = scan_dir(&ws.posts_dir());
    let pages = scan_dir(&ws.pages_dir());
    let mut tag_set = std::collections::BTreeSet::new();
    for p in &posts {
        if let Ok(src) = std::fs::read_to_string(&p.path) {
            if let Ok(doc) = parse(&src) {
                for t in doc.front_matter.tags {
                    tag_set.insert(t);
                }
            }
        }
    }
    WorkspaceSummary {
        root: ws.root.clone(),
        name: ws.config.site.title.clone(),
        posts,
        pages,
        tags: tag_set.into_iter().collect(),
    }
}
```

(Re-parsing posts for tags here is acceptable — the scan already parses once in `scan_dir`; if performance matters later, have `scan_dir` return tags alongside. For now, keep it simple and correct. `parse` is already imported in this file.)

- [ ] **Step 4: Fix `DocumentRef { .. }` literals**

Run: `grep -rn "DocumentRef {" crates/`
Add `slug` to every literal (the sidebar doesn't construct `DocumentRef`, but tests might).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p lopress-gui-host`
Expected: PASS (new test + existing). The editor crate consumes `DocumentRef`/`WorkspaceSummary`; it will still compile because new fields are additive (the sidebar reads only `path`/`title`/`is_draft`/`has_parse_error`).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/tests/session_integration.rs
git commit -m "feat(gui-host): expose page slugs and tag union in the workspace summary"
```

---

## Task 3: `Session::nav_items` and `Session::update_nav`

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Test: `crates/lopress-gui-host/tests/session_integration.rs`

- [ ] **Step 1: Write the failing test**

Open a temp workspace, call `update_nav` with two items, then assert `nav_items()` returns them and `lopress.toml` on disk contains them:

```rust
// let session = Session::open(workspace_root).unwrap();
// session.update_nav(vec![NavItem { label: "Home".into(), href: "/".into() }]).unwrap();
// let items = session.nav_items();
// assert_eq!(items.len(), 1);
// assert_eq!(items[0].label, "Home");
```

(Use `lopress_build::NavItem`. Note `Session::open` starts background build/serve threads — the existing integration tests already handle this; mirror their setup, and allow the rebuild to be in-flight.)

- [ ] **Step 2: Add the methods**

In `impl Session` (`crates/lopress-gui-host/src/session.rs`):

```rust
/// Current nav items, read fresh from `lopress.toml` so repeated edits in one
/// session reflect the latest saved state.
pub fn nav_items(&self) -> Vec<lopress_build::NavItem> {
    match lopress_build::Workspace::load(&self.workspace.root) {
        Ok(ws) => ws.config.site.nav.items,
        Err(_) => Vec::new(),
    }
}

/// Rewrite `[site.nav]` in lopress.toml, then rebuild + reload the preview.
///
/// # Errors
/// Returns `SaveError` if the config can't be written.
pub fn update_nav(&self, items: Vec<lopress_build::NavItem>) -> Result<(), SaveError> {
    lopress_build::write_nav(&self.workspace.root, &items)
        .map_err(|e| SaveError::Io(std::io::Error::other(e.to_string())))?;
    self.rebuild();
    Ok(())
}
```

(Confirm `NavItem`/`write_nav`/`Workspace` are re-exported from `lopress_build`'s crate root — add `pub use site::{NavItem, write_nav, Workspace, SiteConfig};` in `lopress-build/src/lib.rs` if they aren't already exported. Confirm the exact `SaveError` variant — read `crates/lopress-gui-host/src/error.rs`; if there's no `Io` tuple variant taking `std::io::Error`, use the variant that exists, e.g. wrap the message in whatever `SaveError` provides.)

- [ ] **Step 3: Run the test**

Run: `cargo test -p lopress-gui-host nav_items` (or your test name)
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/src/lib.rs crates/lopress-gui-host/tests/session_integration.rs
git commit -m "feat(gui-host): Session nav_items + update_nav"
```

---

## Task 4: Nav-editor working model (pure list ops)

The view holds a working list; the list manipulation is pure and unit-testable independent of Floem.

**Files:**
- Create: `crates/lopress-editor/src/ui/nav_editor.rs` (start with the model + tests)
- Modify: `crates/lopress-editor/src/ui/mod.rs` (`pub mod nav_editor;`)
- Test: `crates/lopress-editor/src/ui/nav_editor.rs`

- [ ] **Step 1: Write the failing tests**

`crates/lopress-editor/src/ui/nav_editor.rs` (model portion):

```rust
//! Site-settings nav editor: a modal listing nav links with add/remove/reorder
//! and page/tag pickers, persisting via `Session::update_nav`.

use lopress_build::NavItem;

/// One editable nav row.
#[derive(Debug, Clone, PartialEq)]
pub struct NavRow {
    pub label: String,
    pub href: String,
}

/// The working model: an ordered list of rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavModel {
    pub rows: Vec<NavRow>,
}

impl NavModel {
    pub fn from_items(items: Vec<NavItem>) -> Self {
        Self {
            rows: items
                .into_iter()
                .map(|i| NavRow { label: i.label, href: i.href })
                .collect(),
        }
    }

    /// Drop rows with an empty label or href, then convert to NavItems.
    pub fn to_items(&self) -> Vec<NavItem> {
        self.rows
            .iter()
            .filter(|r| !r.label.trim().is_empty() && !r.href.trim().is_empty())
            .map(|r| NavItem { label: r.label.clone(), href: r.href.clone() })
            .collect()
    }

    pub fn add_empty(&mut self) {
        self.rows.push(NavRow { label: String::new(), href: String::new() });
    }

    pub fn remove(&mut self, idx: usize) {
        if idx < self.rows.len() {
            self.rows.remove(idx);
        }
    }

    pub fn move_up(&mut self, idx: usize) {
        if idx > 0 && idx < self.rows.len() {
            self.rows.swap(idx - 1, idx);
        }
    }

    pub fn move_down(&mut self, idx: usize) {
        if idx + 1 < self.rows.len() {
            self.rows.swap(idx, idx + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> NavModel {
        NavModel::from_items(vec![
            NavItem { label: "A".into(), href: "/a/".into() },
            NavItem { label: "B".into(), href: "/b/".into() },
        ])
    }

    #[test]
    fn move_up_swaps_with_predecessor() {
        let mut m = model();
        m.move_up(1);
        assert_eq!(m.rows[0].label, "B");
        m.move_up(0); // no-op at top
        assert_eq!(m.rows[0].label, "B");
    }

    #[test]
    fn move_down_swaps_with_successor() {
        let mut m = model();
        m.move_down(0);
        assert_eq!(m.rows[0].label, "B");
        m.move_down(1); // no-op at bottom
        assert_eq!(m.rows[1].label, "A");
    }

    #[test]
    fn to_items_drops_empty_rows() {
        let mut m = model();
        m.add_empty();
        m.rows[2].label = "  ".into(); // whitespace-only label
        let items = m.to_items();
        assert_eq!(items.len(), 2, "empty/whitespace rows dropped");
    }

    #[test]
    fn remove_out_of_range_is_noop() {
        let mut m = model();
        m.remove(99);
        assert_eq!(m.rows.len(), 2);
    }
}
```

- [ ] **Step 2: Register the module + run tests**

In `crates/lopress-editor/src/ui/mod.rs`, add `pub mod nav_editor;` near the other `pub mod` lines.

Run: `cargo test -p lopress-editor nav_editor`
Expected: PASS (`from_items`/`to_items`/`move_*`/`remove`).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/nav_editor.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): nav-editor working model with reorder + empty-row pruning"
```

---

## Task 5: Nav-editor panel view (+ page/tag pickers)

**Files:**
- Modify: `crates/lopress-editor/src/ui/nav_editor.rs` (add the view)

This task is GUI view code; there is no headless unit test for the rendered view (the model is tested in Task 4). Verification is the compile + the e2e check in Task 7.

- [ ] **Step 1: Build the panel view**

Add to `crates/lopress-editor/src/ui/nav_editor.rs`. The panel takes the initial items, the available pages and tags (for pickers), and `on_save` / `on_cancel` callbacks:

```rust
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{
    button, dyn_container, h_stack, h_stack_from_iter, label, scroll, text_input, v_stack,
    v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// A picker target: a page (title → `/slug/`) or a tag (name → `/tags/name/`).
#[derive(Clone)]
pub struct PickTarget {
    pub label: String,
    pub href: String,
}

#[allow(clippy::too_many_arguments)]
pub fn nav_editor_panel(
    initial: Vec<NavItem>,
    pages: Vec<PickTarget>,
    tags: Vec<PickTarget>,
    on_save: Rc<dyn Fn(Vec<NavItem>)>,
    on_cancel: Rc<dyn Fn()>,
) -> AnyView {
    let model: RwSignal<NavModel> = RwSignal::new(NavModel::from_items(initial));

    // Rows re-render whenever the model changes.
    let rows = dyn_container(
        move || model.get(),
        move |m| {
            let mut row_views: Vec<AnyView> = Vec::with_capacity(m.rows.len());
            for (i, r) in m.rows.iter().enumerate() {
                row_views.push(nav_row_view(model, i, r.clone()));
            }
            v_stack_from_iter(row_views).style(|s| s.gap(4.).width_full()).into_any()
        },
    );

    let add_btn = button(label(|| "+ Add link".to_string()))
        .action(move || model.update(|m| m.add_empty()));

    let page_picker = picker_button("Link to page ▾", pages, model);
    let tag_picker = picker_button("Link to tag ▾", tags, model);

    let save = {
        let on_save = Rc::clone(&on_save);
        button(label(|| "Save".to_string())).action(move || {
            let items = model.with_untracked(|m| m.to_items());
            (on_save)(items);
        })
    };
    let cancel = button(label(|| "Cancel".to_string()))
        .action(move || (on_cancel)());

    let panel = v_stack((
        label(|| "Site settings — Navigation".to_string())
            .style(|s| s.font_size(15.).margin_bottom(8.)),
        scroll(rows).style(|s| s.max_height(360.).width_full()),
        h_stack((add_btn, page_picker, tag_picker)).style(|s| s.gap(8.).margin_top(8.)),
        h_stack((save, cancel)).style(|s| s.gap(8.).margin_top(12.)),
    ))
    .style(|s| {
        s.padding(16.)
            .width(520.)
            .background(Color::WHITE)
            .border(1.)
            .border_color(Color::rgb8(200, 200, 210))
            .border_radius(8.)
    });

    // Dimmed backdrop; the panel is centered within bounds (no negative insets,
    // per the floem overlay hit-test caveat).
    h_stack((panel,))
        .style(|s| {
            s.width_full()
                .height_full()
                .items_center()
                .justify_center()
                .background(Color::rgba8(0, 0, 0, 80))
        })
        .into_any()
}

fn nav_row_view(model: RwSignal<NavModel>, idx: usize, row: NavRow) -> AnyView {
    let label_buf: RwSignal<String> = RwSignal::new(row.label);
    let href_buf: RwSignal<String> = RwSignal::new(row.href);

    let label_input = text_input(label_buf)
        .on_event_cont(floem::event::EventListener::FocusLost, move |_| {
            let v = label_buf.get_untracked();
            model.update(|m| { if let Some(r) = m.rows.get_mut(idx) { r.label = v.clone(); } });
        })
        .style(|s| s.min_width(140.).font_size(12.));
    let href_input = text_input(href_buf)
        .on_event_cont(floem::event::EventListener::FocusLost, move |_| {
            let v = href_buf.get_untracked();
            model.update(|m| { if let Some(r) = m.rows.get_mut(idx) { r.href = v.clone(); } });
        })
        .style(|s| s.min_width(180.).font_size(12.));

    let up = button(label(|| "↑".to_string())).action(move || model.update(|m| m.move_up(idx)));
    let down = button(label(|| "↓".to_string())).action(move || model.update(|m| m.move_down(idx)));
    let del = button(label(|| "✕".to_string())).action(move || model.update(|m| m.remove(idx)));

    h_stack_from_iter(vec![
        label_input.into_any(),
        href_input.into_any(),
        up.into_any(),
        down.into_any(),
        del.into_any(),
    ])
    .style(|s| s.gap(4.).items_center().width_full())
    .into_any()
}

/// A picker that, when a target is chosen, appends a row pre-filled with its
/// label + href. Floem 0.2 has no stock dropdown, so this opens a small popup
/// list of buttons (mirroring `attr_select` in `ui/blocks/plugin.rs`).
fn picker_button(title: &'static str, targets: Vec<PickTarget>, model: RwSignal<NavModel>) -> AnyView {
    let open: RwSignal<bool> = RwSignal::new(false);
    let toggle = button(label(move || title.to_string())).action(move || open.update(|o| *o = !*o));

    let popup = dyn_container(
        move || open.get(),
        move |is_open| {
            if !is_open {
                return floem::views::empty().into_any();
            }
            let mut btns: Vec<AnyView> = Vec::with_capacity(targets.len());
            for t in &targets {
                let t = t.clone();
                let btn = button(label({ let l = t.label.clone(); move || l.clone() })).action(move || {
                    model.update(|m| {
                        m.rows.push(NavRow { label: t.label.clone(), href: t.href.clone() });
                    });
                    open.set(false);
                });
                btns.push(btn.into_any());
            }
            v_stack_from_iter(btns)
                .style(|s| {
                    s.background(Color::WHITE)
                        .border(1.)
                        .border_color(Color::rgb8(210, 210, 215))
                        .border_radius(4.)
                        .padding(4.)
                        .gap(2.)
                })
                .into_any()
        },
    );

    v_stack((toggle, popup)).into_any()
}
```

(Verify these Floem 0.2 method names against existing usage: `text_input(RwSignal<String>)`, `on_event_cont` vs `on_event` returning `EventPropagation` — `ui/blocks/plugin.rs::attr_text` uses `.on_event(FocusLost, |_| { …; EventPropagation::Continue })`; if `on_event_cont` doesn't exist, use `.on_event(...)` returning `EventPropagation::Continue` exactly as `attr_text` does. `Color::WHITE`/`Color::rgba8` — confirm the constants used elsewhere. `max_height` exists in this codebase's styles.)

- [ ] **Step 2: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: success (fix any Floem method-name mismatches against the patterns in `plugin.rs`/`sidebar.rs`).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/nav_editor.rs
git commit -m "feat(editor): nav-editor modal panel with page/tag pickers"
```

---

## Task 6: Wire the entry point + modal into the editing view

**Files:**
- Modify: `crates/lopress-editor/src/ui/sidebar.rs` (add a "Site settings" button + `on_site_settings` callback)
- Modify: `crates/lopress-editor/src/ui/mod.rs` (signal, sidebar arg, modal overlay)

- [ ] **Step 1: Add the sidebar callback + button**

In `crates/lopress-editor/src/ui/sidebar.rs`, add an `on_site_settings: Rc<dyn Fn()>` parameter to `sidebar_view` and a button in the footer:

```rust
pub fn sidebar_view(
    workspace: RwSignal<WorkspaceSummary>,
    current_path: RwSignal<Option<PathBuf>>,
    on_open: Rc<dyn Fn(DocumentRef)>,
    on_new_post: Rc<dyn Fn()>,
    on_new_page: Rc<dyn Fn()>,
    on_site_settings: Rc<dyn Fn()>,
) -> impl IntoView {
    // ... existing ...
    let on_settings_btn = on_site_settings;
    let settings_btn = button(label(|| "⚙ Site settings".to_string()))
        .action(move || (on_settings_btn)())
        .style(|s| s.width_full().padding_vert(4.));

    let footer = v_stack((new_post_btn, new_page_btn, settings_btn))
        .style(|s| s.gap(4.).padding(8.).border_top(1.).border_color(BORDER));
    // ... rest unchanged ...
}
```

- [ ] **Step 2: Add the signal + callback in the editing view**

In `crates/lopress-editor/src/ui/mod.rs`, near the other signals (e.g. after `slash_menu_open`):

```rust
let nav_editor_open: RwSignal<bool> = RwSignal::new(false);
let on_site_settings: Rc<dyn Fn()> = Rc::new(move || nav_editor_open.set(true));
```

Pass `on_site_settings` to `sidebar_view(...)` (add it as the new last argument).

- [ ] **Step 3: Build the modal overlay and add it to the root stack**

After `columns` is built and before/with the final `stack((columns, footer))`, add a modal overlay driven by `nav_editor_open`. It needs the session (via `editing`) for `nav_items`/`update_nav` and the workspace summary for pickers:

```rust
let editing_for_nav = Rc::clone(&editing);
let nav_modal = dyn_container(
    move || nav_editor_open.get(),
    move |is_open| {
        if !is_open {
            return empty().into_any();
        }
        let st = editing_for_nav.borrow();
        let initial = st.session.nav_items();
        let ws = workspace_signal.get_untracked();
        let pages: Vec<crate::ui::nav_editor::PickTarget> = ws
            .pages
            .iter()
            .map(|p| crate::ui::nav_editor::PickTarget {
                label: p.title.clone(),
                href: format!("/{}/", p.slug),
            })
            .collect();
        let tags: Vec<crate::ui::nav_editor::PickTarget> = ws
            .tags
            .iter()
            .map(|t| crate::ui::nav_editor::PickTarget {
                label: t.clone(),
                href: format!("/tags/{t}/"),
            })
            .collect();
        drop(st);

        let editing_for_save = Rc::clone(&editing_for_nav);
        let on_save: Rc<dyn Fn(Vec<lopress_build::NavItem>)> = Rc::new(move |items| {
            let st = editing_for_save.borrow();
            // Surfacing the error inline is a refinement; on failure leave the
            // modal open. For the first cut, log and close on success.
            match st.session.update_nav(items) {
                Ok(()) => nav_editor_open.set(false),
                Err(e) => eprintln!("update_nav failed: {e}"),
            }
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || nav_editor_open.set(false));

        crate::ui::nav_editor::nav_editor_panel(initial, pages, tags, on_save, on_cancel)
    },
)
.style(|s| s.absolute().inset(0.).width_full().height_full());

let editing_for_close = Rc::clone(&editing);
stack((columns, footer, nav_modal))
    .style(|s| s.flex_col().width_full().height_full())
    // ... existing on_event_stop(WindowClosed, ...) ...
```

(The overlay uses `.absolute().inset(0.)` to cover the window; the panel inside `nav_editor_panel` centers itself within those bounds — no negative insets, per the caveat. Confirm `s.absolute()` / `s.inset(0.)` are the Floem 0.2 style method names used elsewhere for overlays — grep `ui` for `absolute(` / `inset(`; the slash-menu overlay and toolbar use floating placement, so reuse whatever they use. `workspace_signal` and `editing` are already in scope in this function.)

- [ ] **Step 4: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/sidebar.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): open the nav editor from the sidebar as a modal"
```

---

## Task 7: Full gate + end-to-end verification

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace` pass. Fix clippy per `AGENTS.md`.

- [ ] **Step 2: End-to-end (control interface)**

Using the `driving-lopress-editor` capability (`127.0.0.1:7878`) against a throwaway workspace under `$TEMP`:
- launch the editor (repo-root `cargo run`, visible window; poll `/ping`),
- `/open` an absolute path into the workspace,
- open "Site settings", add a link via the page picker, Save,
- read `lopress.toml` and confirm the `[[site.nav.items]]` entry was added (and other keys/comments preserved),
- confirm the rebuilt pages render the link in the header nav.

Record verbatim commands + outputs; no PASS without them. (Native text entry + clicks in the modal are partly real-mouse; hand back genuinely real-mouse-only checks per the control workflow, but assert file + build state directly.)

- [ ] **Step 3: Commit any gate fixes**

```bash
git add -A
git commit -m "chore: gate pass for nav editor"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage:** `write_nav` toml_edit (Task 1), slug/tags surfacing (Task 2), `nav_items`/`update_nav` (Task 3), working model (Task 4), panel + pickers (Task 5), entry point + modal (Task 6), gate + e2e (Task 7).
- **Type consistency:** `NavItem` is `lopress_build::NavItem` throughout; `NavModel`/`NavRow`/`PickTarget` defined in nav_editor.rs and used unchanged by the editing view; `DocumentRef.slug` / `WorkspaceSummary.tags` added in Task 2 are consumed in Task 6.
- **Soft spots flagged for the implementer (external/Floem APIs to confirm against the pinned versions, not fabricate):** `toml_edit` 0.22 API names (Task 1); Floem `text_input`/`on_event` provenance, `Color::WHITE`/`rgba8`, `absolute()`/`inset()` overlay styling (Tasks 5–6) — all have in-repo precedents (`plugin.rs`, `sidebar.rs`, the slash-menu overlay) to copy.
- **No editor↔build signature coupling:** this plan is independent of the read-more and image plans; it touches `lopress-build` (`write_nav`), `lopress-gui-host` (session + summary), and `lopress-editor` (nav_editor + sidebar + mod) only.

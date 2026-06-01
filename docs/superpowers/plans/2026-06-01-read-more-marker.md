# Read-More Marker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a one-per-post "Read more" marker block that splits a post into a teaser and the rest, rendering the teaser (as HTML) on the home page with a "Read more →" link.

**Architecture:** The marker is an empty `lopress:more` comment-container block, shipped as a third base plugin (like `list`/`code`). The editor renders it as a slim divider via a registered editor widget, inserts it from the slash menu (a new plugin-block insertion path), and enforces one-per-post at both the menu and the `apply()` chokepoint. The build renders the blocks before the marker into `PostSummary.excerpt_html`, which the index template displays.

**Tech Stack:** Rust, Floem (editor GUI), Tera (templates), `serde_json`, the workspace's strict clippy lints (no `unwrap`/`expect`/`panic`, no lossy `as` casts — see `AGENTS.md`).

**Spec:** `docs/superpowers/specs/2026-06-01-read-more-marker-design.md`

> **Note on the spec's §8 cache approach:** the spec floated an `excerpt_hash` on `PageEntry`. This plan uses a simpler, equivalent mechanism that needs no cache-schema change: a re-rendered (non-skipped) post that contains a marker flips `post_set_changed`, which already drives index regeneration. See Task 9.

> **Convention:** run the canonical gate `bash scripts/check.sh` (fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace`) before declaring the feature done. Individual-test commands are given per task. `cargo fmt --all` runs in apply mode via the Stop hook, so formatting changes appear in your tree — stage them.

---

## Task 1: The `more` base plugin manifest

**Files:**
- Create: `base_plugins/more/manifest.toml`
- Test: `crates/lopress-plugin/src/manifest.rs` (add a unit test)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/lopress-plugin/src/manifest.rs`:

```rust
#[test]
fn parses_read_more_marker_manifest() {
    let src = r#"
name    = "lopress-more"
version = "0.1.0"

[[blocks]]
name    = "lopress:more"
editor  = "more"
builtin = true
"#;
    let m = parse_manifest_str(src).unwrap();
    assert_eq!(m.name, "lopress-more");
    assert_eq!(m.blocks.len(), 1);
    let b = &m.blocks[0];
    assert_eq!(b.name, "lopress:more");
    assert_eq!(b.editor.as_deref(), Some("more"));
    assert!(b.builtin);
    assert!(b.native.is_none());
    assert!(b.template.is_none());
    assert!(b.attrs.is_empty());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lopress-plugin parses_read_more_marker_manifest`
Expected: compiles and FAILS only if the manifest can't parse — but `parse_manifest_str` is pure, so this test actually passes once the assertions match. (It documents the expected shape.) If it fails, the assertions are the spec. Proceed to create the file so the embedded manifest exists for later tasks.

- [ ] **Step 3: Create the manifest file**

`base_plugins/more/manifest.toml`:

```toml
# Built-in "base" plugin: the read-more marker. Embedded at compile time via
# include_str! — see PluginRegistry::load_base_plugins.
name    = "lopress-more"
version = "0.1.0"

[[blocks]]
name    = "lopress:more"
editor  = "more"
builtin = true
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p lopress-plugin parses_read_more_marker_manifest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add base_plugins/more/manifest.toml crates/lopress-plugin/src/manifest.rs
git commit -m "feat(plugin): add the read-more base plugin manifest"
```

---

## Task 2: Seed the `more` base plugin into the editor registry

**Files:**
- Modify: `crates/lopress-plugin/src/registry.rs` (`load_base_plugins`)

First, read `load_base_plugins` to see how `list` and `code` are embedded — it uses `include_str!` against the `base_plugins/<name>/manifest.toml` files and inserts each into the registry. Mirror it for `more`.

- [ ] **Step 1: Find the existing embeds**

Run: `grep -n "include_str!" crates/lopress-plugin/src/registry.rs`
Expected: two lines embedding the `list` and `code` manifests inside `load_base_plugins`.

- [ ] **Step 2: Add the `more` embed**

In `load_base_plugins`, alongside the existing `list`/`code` embeds, add an embed + insert for `more`. Match the exact pattern already used (the surrounding lines show whether it parses via `parse_manifest_str` and how the `LoadedPlugin { root, manifest }` is built — replicate it, using `base_plugins/more/manifest.toml` and a synthetic root consistent with the others). Example shape (adapt to the real surrounding code):

```rust
let more_src = include_str!("../../../base_plugins/more/manifest.toml");
let more = parse_manifest_str(more_src)?;
self.insert(LoadedPlugin {
    root: std::path::PathBuf::from("<embedded:more>"),
    manifest: more,
})?;
```

- [ ] **Step 3: Add a test that the marker block resolves**

Add to the `tests` module in `crates/lopress-plugin/src/registry.rs`:

```rust
#[test]
fn base_plugins_include_read_more_marker() {
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    let (_plugin, decl) = reg.block("lopress:more").expect("more block registered");
    assert_eq!(decl.editor.as_deref(), Some("more"));
    assert!(decl.builtin);
    assert!(decl.native.is_none());
}
```

(If a similar `base_plugins_include_*` test already exists, follow its exact construction — e.g. it may call `load_base_plugins` differently.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p lopress-plugin base_plugins_include_read_more_marker`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): seed the read-more marker as a base plugin"
```

---

## Task 3: `to_core` emits an empty container for the marker

The marker must serialize as a clean empty `<!-- lopress:more -->`/`<!-- /lopress:more -->` pair. `plugin_block_to_core` always emits one inner child, so the marker needs a dedicated branch that emits **no** children.

**Files:**
- Modify: `crates/lopress-editor/src/model/to_core.rs` (`block_to_core`)
- Test: `crates/lopress-editor/src/model/to_core.rs` (unit test) or `crates/lopress-editor/tests/from_to_core_tests.rs`

- [ ] **Step 1: Write the failing test**

Add a unit test inside `crates/lopress-editor/src/model/to_core.rs` (add a `#[cfg(test)] mod tests` if none exists, or append to it):

```rust
#[cfg(test)]
mod more_marker_tests {
    use super::*;
    use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock, PluginMeta};
    use std::rc::Rc;

    fn marker_block() -> EditorBlock {
        EditorBlock {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(vec![]),
            plugin: Some(PluginMeta {
                block_type_name: Rc::from("lopress:more"),
                attrs: serde_json::Map::new(),
                attr_decls: Rc::from([]),
                builtin: true,
                editor: Some(Rc::from("more")),
                native: None,
            }),
        }
    }

    #[test]
    fn marker_serializes_to_empty_container() {
        let core = block_to_core(&marker_block());
        assert_eq!(core.r#type, "lopress:more");
        assert!(core.children.is_empty(), "marker must have no children");
        assert!(core.text.is_none());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lopress-editor marker_serializes_to_empty_container`
Expected: FAIL — `assert!(core.children.is_empty())` fails because `plugin_block_to_core` emits one paragraph child.

- [ ] **Step 3: Add the marker branch**

In `block_to_core`, at the top of the `if let Some(meta) = &b.plugin {` block (before the `match &meta.native`), add:

```rust
if let Some(meta) = &b.plugin {
    // The read-more marker is an empty container: emit no children so it
    // round-trips as a clean `<!-- lopress:more -->`/`<!-- /lopress:more -->`
    // pair (plugin_block_to_core would otherwise emit one inner child).
    if &*meta.block_type_name == "lopress:more" {
        return Block {
            r#type: "lopress:more".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: None,
        };
    }
    return match &meta.native {
        Some(core_type) => native_block_to_core(b, meta, core_type),
        None => plugin_block_to_core(b, meta),
    };
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p lopress-editor marker_serializes_to_empty_container`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/model/to_core.rs
git commit -m "feat(editor): serialize the read-more marker as an empty container"
```

---

## Task 4: Round-trip the marker (core + editor)

**Files:**
- Test: `crates/lopress-core/tests/roundtrip.rs`
- Test: `crates/lopress-editor/tests/from_to_core_tests.rs`

- [ ] **Step 1: Write the core round-trip test**

Add to `crates/lopress-core/tests/roundtrip.rs` (match the file's existing helper/assert style — most tests there parse then serialize and compare):

```rust
#[test]
fn read_more_marker_round_trips() {
    let src = "before\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nafter\n";
    let doc = lopress_core::parse(src).unwrap();
    let out = lopress_core::serialize(&doc);
    assert_eq!(out, src);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p lopress-core read_more_marker_round_trips`
Expected: PASS (the empty-container path already exists in the parser and serializer). If it fails on whitespace, adjust the expected `src` to match the serializer's exact blank-line output — run once, read the diff, and set `src` to the serializer's canonical form (this is the byte-identical target authors will get).

- [ ] **Step 3: Write the editor round-trip test**

Add to `crates/lopress-editor/tests/from_to_core_tests.rs`. Mirror how other tests there load base plugins and call `doc_from_core` / `doc_to_core` (read one existing test first for the exact registry setup):

```rust
#[test]
fn read_more_marker_survives_editor_round_trip() {
    let mut reg = lopress_plugin::PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    let src = "before\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nafter\n";
    let core = lopress_core::parse(src).unwrap();
    let edoc = lopress_editor::model::from_core::doc_from_core(&core, &reg);
    let back = lopress_editor::model::to_core::doc_to_core(&edoc);
    let out = lopress_core::serialize(&back);
    assert_eq!(out, src);
}
```

(Adjust module paths/visibility to match how other tests in this file reach `doc_from_core`/`doc_to_core`.)

- [ ] **Step 4: Run it**

Run: `cargo test -p lopress-editor read_more_marker_survives_editor_round_trip`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-core/tests/roundtrip.rs crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "test: round-trip the read-more marker through core and editor"
```

---

## Task 5: `EditorBlock::read_more` constructor

A fixed-shape constructor for the marker block, so insertion needs no plugin registry threaded into the editor pane.

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs`
- Test: same file

- [ ] **Step 1: Write the failing test**

Add to a `#[cfg(test)] mod tests` in `crates/lopress-editor/src/model/types.rs` (create the module if absent):

```rust
#[cfg(test)]
mod read_more_ctor_tests {
    use super::*;

    #[test]
    fn read_more_block_has_marker_meta() {
        let b = EditorBlock::read_more();
        let meta = b.plugin.as_ref().expect("plugin meta");
        assert_eq!(&*meta.block_type_name, "lopress:more");
        assert_eq!(meta.editor.as_deref(), Some("more"));
        assert!(meta.builtin);
        assert!(meta.native.is_none());
        assert!(matches!(b.body, BlockBody::Inline(ref runs) if runs.is_empty()));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor read_more_block_has_marker_meta`
Expected: FAIL — `EditorBlock::read_more` does not exist.

- [ ] **Step 3: Add the constructors**

In `crates/lopress-editor/src/model/types.rs`, add to `impl PluginMeta`:

```rust
/// The canonical `PluginMeta` for the read-more marker. A comment-container
/// block (no `native` claim), built-in (chrome suppressed), edited via the
/// `"more"` divider widget. No attrs.
pub fn read_more() -> Self {
    Self {
        block_type_name: Rc::from("lopress:more"),
        attrs: serde_json::Map::new(),
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("more")),
        native: None,
    }
}
```

And to `impl EditorBlock`:

```rust
/// The read-more marker block: an empty-bodied plugin block carrying
/// `PluginMeta::read_more`. The body is an empty inline run vec — the marker
/// renders via its editor widget and serializes to an empty container.
pub fn read_more() -> Self {
    Self {
        id: BlockId::new(),
        kind: BlockKind::Paragraph,
        body: BlockBody::Inline(vec![]),
        plugin: Some(PluginMeta::read_more()),
    }
}
```

(`serde_json::Map` is already imported via `serde_json::Value`; add `use serde_json::Map;` if not present, or write `serde_json::Map::new()` fully-qualified as above.)

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p lopress-editor read_more_block_has_marker_meta`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs
git commit -m "feat(editor): add EditorBlock::read_more marker constructor"
```

---

## Task 6: One-per-post guard in `apply()`

`apply_insert_after` must refuse to insert a second marker.

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs`
- Test: same file

- [ ] **Step 1: Write the failing test**

Add to `crates/lopress-editor/src/actions.rs` a new test module:

```rust
#[cfg(test)]
mod read_more_guard_tests {
    use super::*;
    use crate::model::types::{EditorBlock, InlineRun};

    fn doc_with_para() -> EditorDoc {
        EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("p")])],
            front_matter: lopress_core::FrontMatter::default(),
        }
    }

    #[test]
    fn first_marker_inserts_second_is_rejected() {
        let mut doc = doc_with_para();
        let anchor = doc.blocks[0].id;

        let first = apply(
            &mut doc,
            BlockAction::InsertAfter {
                anchor,
                new_block: Box::new(EditorBlock::read_more()),
            },
        );
        assert!(first.is_some(), "first marker should insert");
        assert_eq!(doc.blocks.len(), 2);

        let anchor2 = doc.blocks[0].id;
        let second = apply(
            &mut doc,
            BlockAction::InsertAfter {
                anchor: anchor2,
                new_block: Box::new(EditorBlock::read_more()),
            },
        );
        assert!(second.is_none(), "second marker must be rejected");
        assert_eq!(doc.blocks.len(), 2, "no second marker inserted");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor first_marker_inserts_second_is_rejected`
Expected: FAIL — the second insert currently succeeds (len becomes 3).

- [ ] **Step 3: Add the guard + helper**

In `crates/lopress-editor/src/actions.rs`, add a helper near `find_idx`:

```rust
/// True when `block` is the read-more marker (`lopress:more`).
fn is_read_more(block: &EditorBlock) -> bool {
    block
        .plugin
        .as_ref()
        .is_some_and(|m| &*m.block_type_name == "lopress:more")
}
```

Then at the top of `apply_insert_after` (before computing `pos`):

```rust
fn apply_insert_after(
    doc: &mut EditorDoc,
    anchor: BlockId,
    new_block: EditorBlock,
) -> Option<(BlockAction, BlockAction)> {
    // One read-more marker per post: refuse a second.
    if is_read_more(&new_block) && doc.blocks.iter().any(is_read_more) {
        return None;
    }
    let pos = find_idx(doc, anchor)
        .map(|i| i + 1)
        .unwrap_or(doc.blocks.len());
    // ... unchanged below ...
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p lopress-editor first_marker_inserts_second_is_rejected`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/actions.rs
git commit -m "feat(editor): enforce one read-more marker per post in apply()"
```

---

## Task 7: The `more` editor widget (divider)

Register a `"more"` editor widget that renders a slim divider and is focusable for deletion.

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/read_more.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (add `mod read_more;`)
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` (register `"more"`)
- Test: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

- [ ] **Step 1: Write the failing test**

In `editor_registry.rs` tests, extend the existing key-resolution test (or add one):

```rust
#[test]
fn editor_for_resolves_more() {
    assert!(editor_for("more").is_some());
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor editor_for_resolves_more`
Expected: FAIL — `editor_for("more")` returns `None`.

- [ ] **Step 3: Create the widget**

`crates/lopress-editor/src/ui/blocks/read_more.rs`:

```rust
//! The read-more marker's editor widget: a slim, full-width divider labeled
//! "Read more". It ignores the (empty) body and is focusable on PointerDown so
//! the block can be selected and deleted via the toolbar — mirroring the focus
//! handoff in `fallback.rs`.

use crate::ui::blocks::editor_registry::EditorContext;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{label, Decorators};
use floem::{AnyView, IntoView};

const RULE: Color = Color::rgb8(180, 160, 210);
const FG: Color = Color::rgb8(120, 100, 150);

pub fn read_more_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
    label(|| "— Read more —".to_string())
        .style(move |s| {
            s.width_full()
                .padding_vert(6.)
                .color(FG)
                .font_size(11.)
                .items_center()
                .justify_center()
                .border_top(1.)
                .border_bottom(1.)
                .border_color(RULE)
        })
        .on_event(EventListener::PointerDown, move |_| {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        })
        .into_any()
}
```

(If `border_top`/`justify_center` aren't the exact Floem 0.2 style method names used elsewhere, grep `crates/lopress-editor/src/ui` for the real ones — e.g. `border_top`, `justify_center` are used in existing block styles; match them.)

- [ ] **Step 4: Register the module and the editor key**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add alongside the other `pub mod` block declarations:

```rust
pub mod read_more;
```

In `crates/lopress-editor/src/ui/blocks/editor_registry.rs`:
- add `use crate::ui::blocks::read_more;` to the `use crate::ui::blocks::{...}` import,
- add the match arm in `editor_for`:

```rust
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        "code" => Some(code_editor_widget),
        "more" => Some(read_more::read_more_widget),
        _ => None,
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p lopress-editor editor_for_resolves_more`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/read_more.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "feat(editor): render the read-more marker as a divider widget"
```

---

## Task 8: Slash-menu insertion of the marker (+ one-per-post omission)

Generalize the slash menu from "BlockKind only" to a choice enum, add the "Read more" entry, route it to `InsertAfter`, and omit it when a marker already exists.

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs`
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs`
- Test: `crates/lopress-editor/tests/slash_menu_tests.rs`

- [ ] **Step 1: Write the failing test**

In `crates/lopress-editor/tests/slash_menu_tests.rs`, add tests for the new item model (read the file first for how it imports `slash_menu_items`):

```rust
#[test]
fn slash_items_include_read_more() {
    let items = lopress_editor::ui::slash_menu::slash_menu_items();
    assert!(
        items.iter().any(|(label, choice)| *label == "Read more"
            && matches!(choice, lopress_editor::ui::slash_menu::SlashChoice::ReadMore)),
        "expected a Read more / SlashChoice::ReadMore entry"
    );
}

#[test]
fn paragraph_entry_is_a_kind_choice() {
    use lopress_editor::model::types::BlockKind;
    let items = lopress_editor::ui::slash_menu::slash_menu_items();
    assert!(items.iter().any(|(label, choice)| *label == "Paragraph"
        && matches!(choice, lopress_editor::ui::slash_menu::SlashChoice::Kind(BlockKind::Paragraph))));
}
```

(Adjust the module path prefix to whatever the crate exposes — if `ui`/`model` aren't `pub`, add `pub use` or test via an in-crate `#[cfg(test)]` module in `slash_menu.rs` instead.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor slash_items_include_read_more`
Expected: FAIL — `SlashChoice` does not exist.

- [ ] **Step 3: Introduce `SlashChoice` and update `slash_menu_items`**

In `crates/lopress-editor/src/ui/slash_menu.rs`:

```rust
/// A slash-menu selection: either convert the current block to a built-in
/// kind, or insert a plugin block.
#[derive(Debug, Clone, PartialEq)]
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
}

/// The choices offered by the slash menu, in display order.
pub fn slash_menu_items() -> Vec<(&'static str, SlashChoice)> {
    vec![
        ("Paragraph", SlashChoice::Kind(BlockKind::Paragraph)),
        ("Heading 1", SlashChoice::Kind(BlockKind::Heading(1))),
        ("Heading 2", SlashChoice::Kind(BlockKind::Heading(2))),
        ("Heading 3", SlashChoice::Kind(BlockKind::Heading(3))),
        ("Code block", SlashChoice::Kind(BlockKind::Code { lang: Rc::from("") })),
        ("Unordered list", SlashChoice::Kind(BlockKind::List { ordered: false })),
        ("Ordered list", SlashChoice::Kind(BlockKind::List { ordered: true })),
        ("Read more", SlashChoice::ReadMore),
    ]
}
```

- [ ] **Step 4: Update the `slash_menu` view to take items + emit `SlashChoice`**

Change the `slash_menu` function signature so the caller supplies the items list (enabling per-document filtering) and `on_select` receives a `SlashChoice`:

```rust
pub fn slash_menu<F, C>(
    items: Vec<(&'static str, SlashChoice)>,
    on_select: F,
    on_close: C,
) -> impl IntoView
where
    F: Fn(SlashChoice) + Clone + 'static,
    C: Fn() + Clone + 'static,
{
    let len = items.len();
    // ... existing body, but:
    //  - iterate the passed-in `items` (clone for the keydown handler instead
    //    of calling slash_menu_items() again),
    //  - `kind_for_row` / `on_select_for_row(kind)` become
    //    `choice_for_row` / `on_select_for_row(choice)`,
    //  - the Enter handler reads `items_for_key.get(idx)` and passes the
    //    `SlashChoice` clone.
}
```

Replace the internal `let items = slash_menu_items();` and `let items_for_key = slash_menu_items();` with uses of the `items` parameter (clone it once up front for the keydown closure). Everywhere the old code cloned/passed a `BlockKind`, pass the `SlashChoice` instead.

- [ ] **Step 5: Update the editor pane caller**

In `crates/lopress-editor/src/ui/editor_pane.rs`, the menu overlay closure currently builds `on_select` as `BlockAction::ChangeType`. The pane already receives `current_doc: RwSignal<Option<EditorDoc>>`. Update the `Some(block_id) => { ... }` arm:

```rust
Some(block_id) => {
    let on_action_for_select = on_action_for_menu.clone();
    // Omit "Read more" when the document already has a marker.
    let has_more = current_doc.with_untracked(|d| {
        d.as_ref().map_or(false, |doc| {
            doc.blocks.iter().any(|b| {
                b.plugin
                    .as_ref()
                    .is_some_and(|m| &*m.block_type_name == "lopress:more")
            })
        })
    });
    let items: Vec<_> = crate::ui::slash_menu::slash_menu_items()
        .into_iter()
        .filter(|(_, choice)| {
            !(has_more && matches!(choice, crate::ui::slash_menu::SlashChoice::ReadMore))
        })
        .collect();
    let on_select = move |choice: crate::ui::slash_menu::SlashChoice| {
        match choice {
            crate::ui::slash_menu::SlashChoice::Kind(new_kind) => {
                on_action_for_select(BlockAction::ChangeType { block_id, new_kind });
            }
            crate::ui::slash_menu::SlashChoice::ReadMore => {
                on_action_for_select(BlockAction::InsertAfter {
                    anchor: block_id,
                    new_block: Box::new(crate::model::types::EditorBlock::read_more()),
                });
            }
        }
    };
    let on_close = move || {
        slash_menu_open.set(None);
        focus_target.set(Some(block_id));
    };
    slash_menu(items, on_select, on_close)
        .style(|s| s.margin_top(40.).margin_horiz(floem::unit::PxPctAuto::Auto))
        .into_any()
}
```

Add `use floem::reactive::SignalWith;` if `with_untracked` isn't already in scope. Confirm `current_doc` is captured by the overlay closure (clone it into the closure as the existing code does for other signals).

- [ ] **Step 6: Run the tests**

Run: `cargo test -p lopress-editor slash`
Expected: PASS for `slash_items_include_read_more`, `paragraph_entry_is_a_kind_choice`, and any pre-existing slash-menu tests (update them if they assumed the old `(&str, BlockKind)` shape).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/tests/slash_menu_tests.rs
git commit -m "feat(editor): insert the read-more marker from the slash menu (one per post)"
```

---

## Task 9: Build — render the marker to nothing + extract the excerpt

**Files:**
- Modify: `crates/lopress-build/src/render.rs`
- Test: `crates/lopress-build/src/render.rs` (unit tests)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/lopress-build/src/render.rs`:

```rust
#[test]
fn marker_renders_to_nothing_in_body() {
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![
            Block::paragraph("before"),
            Block { r#type: "lopress:more".into(), attrs: json!({}), children: vec![], text: None },
            Block::paragraph("after"),
        ],
    };
    let html = render_body(&doc, &empty_registry(), &Tera::default()).unwrap();
    assert_eq!(html, "<p>before</p>\n<p>after</p>\n");
}

#[test]
fn excerpt_is_blocks_before_marker() {
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![
            Block::paragraph("teaser"),
            Block { r#type: "lopress:more".into(), attrs: json!({}), children: vec![], text: None },
            Block::paragraph("hidden"),
        ],
    };
    let ex = render_excerpt(&doc, &empty_registry(), &Tera::default()).unwrap();
    assert_eq!(ex.as_deref(), Some("<p>teaser</p>\n"));
}

#[test]
fn excerpt_is_none_without_marker() {
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block::paragraph("only")],
    };
    let ex = render_excerpt(&doc, &empty_registry(), &Tera::default()).unwrap();
    assert!(ex.is_none());
}
```

(`empty_registry()`, `Block::paragraph`, `json!`, `Tera`, and `FrontMatter` are already used in this test module — confirm the imports at the top of the `tests` module include `serde_json::json` and `lopress_core::FrontMatter` as the existing tests do.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-build marker_renders_to_nothing_in_body excerpt_is_blocks_before_marker excerpt_is_none_without_marker`
Expected: FAIL — `render_excerpt` is undefined; the marker currently renders a `<!-- missing plugin -->` comment (empty registry) rather than nothing.

- [ ] **Step 3: Add the marker arm + `render_excerpt`**

In `crates/lopress-build/src/render.rs`, in `write_block`'s `match b.r#type.as_str()`, add an explicit arm **before** the `custom if custom.starts_with("lopress:")` arm:

```rust
"lopress:more" => {
    // The read-more marker is invisible on the full page; the excerpt
    // boundary is handled by `render_excerpt`.
}
```

Then add the public function (after `render_body`):

```rust
/// Render the blocks that precede the first `lopress:more` marker to HTML.
/// Returns `None` when the document has no marker.
pub fn render_excerpt(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<Option<String>, BuildError> {
    if !doc.blocks.iter().any(|b| b.r#type == "lopress:more") {
        return Ok(None);
    }
    let mut out = String::new();
    for b in &doc.blocks {
        if b.r#type == "lopress:more" {
            break;
        }
        write_block(&mut out, b, registry, tera)?;
    }
    Ok(Some(out))
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p lopress-build marker_renders_to_nothing_in_body excerpt_is_blocks_before_marker excerpt_is_none_without_marker`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-build/src/render.rs
git commit -m "feat(build): hide the read-more marker and add render_excerpt"
```

---

## Task 10: `PostSummary.excerpt_html` + populate it in `post_summaries`

**Files:**
- Modify: `crates/lopress-theme/src/context.rs` (add field)
- Modify: `crates/lopress-build/src/pages.rs` (`post_summaries` signature + body)
- Modify: `crates/lopress-build/src/build.rs` (call site)
- Test: `crates/lopress-build/src/pages.rs` or `crates/lopress-build/tests/build_integration.rs`

- [ ] **Step 1: Add the field**

In `crates/lopress-theme/src/context.rs`, add to `PostSummary`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PostSummary {
    pub title: String,
    pub slug: String,
    pub url: String,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    /// Rendered HTML of the blocks before a `lopress:more` marker, when the
    /// post has one. `None` when the post has no marker.
    pub excerpt_html: Option<String>,
}
```

This breaks every `PostSummary { .. }` literal — find them and add `excerpt_html: None`:

Run: `grep -rn "PostSummary {" crates/`
Expected hits: `crates/lopress-build/src/pages.rs` (in `post_summaries`) and any test fixtures. Update each to set `excerpt_html` (None in tests; the real value in `post_summaries`).

- [ ] **Step 2: Change `post_summaries` signature + populate excerpt**

In `crates/lopress-build/src/pages.rs`, change the signature and body:

```rust
/// Build the list of PostSummary objects used by index/tag templates and feed.
pub fn post_summaries(
    posts: &[DiscoveredPost],
    registry: &PluginRegistry,
    tera: &tera::Tera,
) -> Vec<PostSummary> {
    let mut out: Vec<PostSummary> = posts
        .iter()
        .filter(|p| !p.doc.front_matter.draft)
        .map(|p| {
            let slug = p.slug.clone();
            let url = format!("/posts/{slug}/");
            let excerpt_html = crate::render::render_excerpt(&p.doc, registry, tera)
                .ok()
                .flatten();
            PostSummary {
                title: p.doc.front_matter.title.clone().unwrap_or_else(|| slug.clone()),
                slug,
                url,
                date: p.doc.front_matter.date,
                tags: p.doc.front_matter.tags.clone(),
                description: p.doc.front_matter.description.clone(),
                excerpt_html,
            }
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    out
}
```

(A render failure degrades to `None` via `.ok().flatten()` rather than failing the whole summary build — consistent with the spec's "leave `excerpt_html` None on failure". Add `use lopress_plugin::PluginRegistry;` if not already imported in this file — it is, per the existing `render_all` signature.)

- [ ] **Step 3: Update call sites**

`crates/lopress-build/src/pages.rs` `render_all` (currently `post_summaries(posts, &workspace.config.site.base_url)`) →
```rust
let summaries = post_summaries(posts, registry, tera_shared);
```
`crates/lopress-build/src/build.rs` (currently `post_summaries(&posts, &ws.config.site.base_url)`) →
```rust
let summaries = pages::post_summaries(&posts, &registry, &tera);
```

- [ ] **Step 4: Write a test**

Add to `crates/lopress-build/src/pages.rs` tests (or build_integration.rs):

```rust
#[test]
fn post_summaries_populate_excerpt_when_marker_present() {
    use lopress_plugin::PluginRegistry;
    let src = "---\ntitle: T\n---\nteaser\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nrest\n";
    let doc = lopress_core::parse(src).unwrap();
    let posts = vec![DiscoveredPost {
        source_path: std::path::PathBuf::from("p.md"),
        slug: "p".into(),
        doc,
    }];
    let reg = PluginRegistry::default();
    let summaries = post_summaries(&posts, &reg, &tera::Tera::default());
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].excerpt_html.as_deref(), Some("<p>teaser</p>\n"));
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p lopress-build post_summaries_populate_excerpt_when_marker_present`
Expected: PASS. Also run `cargo test -p lopress-build -p lopress-theme` to catch any missed `PostSummary {}` literal.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-theme/src/context.rs crates/lopress-build/src/pages.rs crates/lopress-build/src/build.rs
git commit -m "feat(build): compute post excerpt_html from the read-more marker"
```

---

## Task 11: Regenerate the index when a marker post's body changes

A re-rendered (non-skipped) post that contains a marker must flip `post_set_changed` so the index picks up the new excerpt.

**Files:**
- Modify: `crates/lopress-build/src/pages.rs` (`render_all`, posts loop)
- Test: `crates/lopress-build/tests/build_integration.rs`

- [ ] **Step 1: Add the flip in the posts render branch**

In `render_all`'s posts loop, inside the `else` branch that calls `render_one_post` (the not-skipped path), after a successful render set `post_set_changed` when the post has a marker:

```rust
match render_one_post(&www, &site_ctx, p, registry, theme, tera_shared) {
    Ok(()) => {
        if let Some(ref old) = old {
            remove_stale_outputs(&www, &old.outputs, &new_outputs);
        }
        let new_entry =
            build_entry(source_hash, new_outputs, is_draft, &p.doc.front_matter);
        if aggregate_metadata_changed(old.as_ref(), &new_entry) {
            post_set_changed = true;
        }
        // A re-rendered post with a read-more marker may have a changed
        // excerpt (body-derived), which the index displays — regenerate it.
        if p.doc.blocks.iter().any(|b| b.r#type == "lopress:more") {
            post_set_changed = true;
        }
        cache.pages.insert(key, new_entry);
        pages_rendered += 1;
    }
    Err(e) => { /* unchanged */ }
}
```

- [ ] **Step 2: Write an integration test**

In `crates/lopress-build/tests/build_integration.rs` (follow the file's existing temp-workspace harness — it builds a workspace dir, runs `lopress_build::build`, and reads `www/`):

```rust
#[test]
fn home_page_shows_excerpt_with_read_more_link() {
    // Build a workspace whose single post has a read-more marker, then assert
    // index.html contains the pre-marker HTML and a Read more link, and the
    // post page contains the full content with no marker comment.
    // (Construct the workspace the same way the other tests in this file do.)
}
```

Fill the body using the existing harness pattern in this file: create `lopress.toml`, `src/posts/p.md` with:
```
---
title: P
date: 2026-06-01
---
teaser para

<!-- lopress:more -->
<!-- /lopress:more -->

hidden para
```
Run `lopress_build::build(workspace)`, then:
```rust
let index = std::fs::read_to_string(www.join("index.html")).unwrap();
assert!(index.contains("teaser para"));
assert!(!index.contains("hidden para"));
assert!(index.contains("Read more"));
let post = std::fs::read_to_string(www.join("posts/p/index.html")).unwrap();
assert!(post.contains("hidden para"));
assert!(!post.contains("lopress:more"));
```

(This test also depends on Task 12's template change for the "Read more" text — write the assertions now; they pass once Task 12 lands. If running this task in isolation, the `teaser`/`hidden` assertions pass immediately and the "Read more" assertion passes after Task 12.)

- [ ] **Step 3: Run it**

Run: `cargo test -p lopress-build home_page_shows_excerpt_with_read_more_link`
Expected: PASS after Task 12 (the excerpt/visibility assertions pass now).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/src/pages.rs crates/lopress-build/tests/build_integration.rs
git commit -m "feat(build): regenerate the index when a marker post's body changes"
```

---

## Task 12: Index template shows the excerpt + Read more link

**Files:**
- Modify: `crates/lopress-theme/assets/default-theme/templates/index.html`
- Modify: `crates/lopress-theme/assets/default-theme/theme.css`
- Test: `crates/lopress-theme/src/builtin.rs` (extend the engine render test)

- [ ] **Step 1: Update the template**

In `index.html`, replace the per-post excerpt block:

```jinja
    <li>
      <a href="{{ p.url }}">{{ p.title }}</a>
      {% if p.date %}<time datetime="{{ p.date }}">{{ p.date }}</time>{% endif %}
      {% if p.excerpt_html %}
      <div class="excerpt">{{ p.excerpt_html | safe }}</div>
      <a class="read-more" href="{{ p.url }}">Read more →</a>
      {% elif p.description %}
      <p class="excerpt">{{ p.description }}</p>
      {% endif %}
    </li>
```

- [ ] **Step 2: Add CSS**

Append to `theme.css`:

```css
.read-more { display: inline-block; margin-top: 0.25rem; font-size: 0.9rem; }
```

- [ ] **Step 3: Extend the builtin render test**

In `crates/lopress-theme/src/builtin.rs` tests, add a test rendering `index.html` with a `PostSummary` that has `excerpt_html: Some("<p>teaser</p>".into())` and assert the output contains `teaser` and `Read more`. Mirror the existing `default_engine_renders_post` test's construction (it builds `SiteCtx`/`PageCtx` directly); set `page.posts` to the summary and `page.kind = PageKind::Index`, render `"index.html"`.

```rust
#[test]
fn index_renders_excerpt_and_read_more() {
    let engine = default_engine().unwrap();
    let summary = PostSummary {
        title: "P".into(),
        slug: "p".into(),
        url: "/posts/p/".into(),
        date: None,
        tags: vec![],
        description: None,
        excerpt_html: Some("<p>teaser</p>".into()),
    };
    let site = SiteCtx { title: "S".into(), base_url: "https://e.com".into(), nav: vec![], posts: vec![summary.clone()] };
    let page = PageCtx {
        kind: PageKind::Index, title: "S".into(), slug: String::new(),
        url: "/".into(), canonical: "https://e.com/".into(),
        description: None, og_image: None, date: None, tags: vec![],
        body_html: String::new(), posts: vec![summary], tag: None,
    };
    let html = engine.render("index.html", &RenderContext { site: &site, page: &page }).unwrap();
    assert!(html.contains("teaser"));
    assert!(html.contains("Read more"));
}
```

- [ ] **Step 4: Run it**

Run: `cargo test -p lopress-theme index_renders_excerpt_and_read_more`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-theme/assets/default-theme/templates/index.html crates/lopress-theme/assets/default-theme/theme.css crates/lopress-theme/src/builtin.rs
git commit -m "feat(theme): show the read-more excerpt and link on the home page"
```

---

## Task 13: Full gate + end-to-end verification

**Files:** none (verification only)

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. Fix any clippy findings per `AGENTS.md` (no `unwrap`/`expect`/`panic`, no lossy `as`, justify any `#[allow]`). Stage formatting changes.

- [ ] **Step 2: End-to-end in the running editor (control interface)**

Using the `driving-lopress-editor` capability (debug HTTP control server on `127.0.0.1:7878`), against a throwaway workspace under `$TEMP` (never commit a `lopress.toml` into the repo):

- launch the editor (`cargo run` from repo root, visible non-minimized window; poll `/ping` until ok),
- `/open` an absolute path to a post in the throwaway workspace,
- insert a "Read more" marker via the slash menu; confirm the divider renders,
- attempt a second insertion; confirm the slash menu no longer offers "Read more" (and that a programmatic second `InsertAfter` is a no-op),
- save; read the file and confirm exactly one `<!-- lopress:more -->`/`<!-- /lopress:more -->` pair,
- confirm the built `index.html` shows the pre-marker content plus a "Read more →" link, and the post page shows the full content.

Record verbatim commands + outputs; do not mark PASS without them.

- [ ] **Step 3: Final commit (if the gate produced formatting changes)**

```bash
git add -A
git commit -m "chore: gate pass for read-more marker"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage:** marker representation (Task 1, 3, 4), base plugin (1, 2), editor divider (7), slash insertion + one-per-post (6, 8), build excerpt + marker-invisible (9), `PostSummary.excerpt_html` (10), index regen (11), template (12), tests/e2e (4, 9–13). All spec sections map to a task.
- **Shared seam:** Tasks 5 + 8 build the slash-menu plugin-block insertion path the Image plan also needs (`SlashChoice` enum, `EditorBlock::read_more` pattern). The Image plan adds a `SlashChoice::Image` variant and an analogous constructor.
- **Type consistency:** `SlashChoice` (slash_menu.rs) is used identically in editor_pane.rs and tests; `is_read_more` lives only in actions.rs; the marker string `"lopress:more"` is the single identity used across to_core, render, apply, and the slash filter.

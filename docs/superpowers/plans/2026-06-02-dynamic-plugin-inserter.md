# Dynamic Plugin-Block Inserter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface every registered comment-container plugin block in the editor's slash menu so users can insert callout/button/embed/etc. blocks from the GUI instead of hand-editing markdown.

**Architecture:** Compute a `PluginInserterItem` list from the workspace `PluginRegistry` at the editing-view boundary, thread it down `ui/mod.rs → editor_pane → slash menu`, add a `SlashChoice::Plugin` variant, and insert a fresh `Opaque` comment-container block (default attrs, empty body, correct `PluginMeta`) via the existing `InsertAfter` action.

**Tech Stack:** Rust workspace (`lopress-plugin`, `lopress-editor`, `lopress-gui-host`), floem editor UI, serde/serde_json, the existing plugin registry + comment-container block model.

---

## File Structure

```
crates/lopress-plugin/src/manifest.rs       — ADD: title, description, category to BlockDecl
crates/lopress-editor/src/model/types.rs    — ADD: PluginInserterItem struct, from_plugin_item() ctor
crates/lopress-editor/src/model/inserter.rs — NEW: inserter_items(registry) pure function
crates/lopress-editor/src/ui/slash_menu.rs  — ADD: SlashChoice::Plugin variant
crates/lopress-editor/src/ui/editor_pane.rs — ADD: inserter_items param, plugin rows, selection arm
crates/lopress-editor/src/ui/mod.rs         — ADD: compute inserter_items from Session registry
```

---

## Task 1: Manifest fields — `title`, `description`, `category` on `BlockDecl`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs` (add 3 new optional fields to `BlockDecl`)

> **These fields do NOT exist yet.** Template-form-blocks added `markdown_template` to `BlockDecl`
> and `label`/`help` to `AttrDecl` — NOT `title`/`description`/`category`. Add them now, exactly
> like `markdown_template` was added (`Option<String>`, `#[serde(default)]`). They parse from
> manifests that set them (the bundled `plugins/callout` and `plugins/button` already do); serde
> ignores unknown keys, so those plugins parse today, but the fields must exist to be *read*.

- [ ] **Step 1: Write the failing test**

In `crates/lopress-plugin/src/manifest.rs`, add to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn parses_title_description_category() {
    let src = r#"
name = "callout"
version = "0.1.0"

[[blocks]]
name              = "lopress:callout"
markdown_template = "blocks/callout.md"
title             = "Callout"
description       = "A highlighted note"
category          = "Text"
"#;
    let m = parse_manifest_str(src).unwrap();
    let b = &m.blocks[0];
    assert_eq!(b.title.as_deref(), Some("Callout"));
    assert_eq!(b.description.as_deref(), Some("A highlighted note"));
    assert_eq!(b.category.as_deref(), Some("Text"));
}

#[test]
fn title_description_category_default_to_none() {
    let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
    let m = parse_manifest_str(src).unwrap();
    let b = &m.blocks[0];
    assert!(b.title.is_none());
    assert!(b.description.is_none());
    assert!(b.category.is_none());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-plugin parses_title_description_category title_description_category_default_to_none`
Expected: FAIL — the fields don't exist on `BlockDecl` yet.

- [ ] **Step 3: Add the three fields to `BlockDecl`**

In `crates/lopress-plugin/src/manifest.rs`, after the `markdown_template` field on `BlockDecl`, add:

```rust
    /// Inserter menu label. When absent, the editor derives one from `name`.
    #[serde(default)]
    pub title: Option<String>,
    /// Inserter menu description / secondary line.
    #[serde(default)]
    pub description: Option<String>,
    /// Inserter grouping bucket (e.g. "Text", "Media"). Falls back to "Blocks".
    #[serde(default)]
    pub category: Option<String>,
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p lopress-plugin parses_title_description_category title_description_category_default_to_none`
Expected: PASS.

- [ ] **Step 5: Fix any literal `BlockDecl { … }` constructions**

Adding fields breaks literal struct constructions. Run `cargo test -p lopress-build` and
`cargo test -p lopress-editor` — if either fails to compile with missing-field errors on
`BlockDecl`, add `title: None, description: None, category: None` to each literal. Known literal
sites: `crates/lopress-build/src/render.rs` tests and any `lopress-editor` test that builds a
`BlockDecl` directly. (The `inserter.rs` test helper added in Task 2 already sets them.)

Run: `cargo test -p lopress-build --no-run && cargo test -p lopress-editor --no-run`
Expected: both compile.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-plugin/src/manifest.rs crates/lopress-build/src/render.rs
git commit -m "feat(plugin): add title, description, category fields to BlockDecl"
```

(Include any `lopress-editor` files you had to touch in Step 5 in the `git add`.)

---

## Task 2: `PluginInserterItem` type + `EditorBlock::from_plugin_item` constructor

**Files:**
- Create: `crates/lopress-editor/src/model/inserter.rs` (new module)
- Modify: `crates/lopress-editor/src/model/types.rs` (add `from_plugin_item` constructor)
- Modify: `crates/lopress-editor/src/model/mod.rs` (export `inserter` module)

### 2a: Define `PluginInserterItem` and `inserter_items()` function

- [ ] **Step 1: Write the module file**

Create `crates/lopress-editor/src/model/inserter.rs`:

```rust
//! Compute the list of insertable plugin blocks from a `PluginRegistry`.
//!
//! A block is insertable when it is a comment-container plugin block:
//! it has a `template` OR a `markdown_template`, is not `builtin`,
//! and does not claim a `native` core type.

use lopress_plugin::{AttrDecl, LoadedPlugin, PluginManifest, PluginRegistry};
use serde_json::{Map, Value};
use std::rc::Rc;

/// An item offered in the slash menu as an insertable plugin block.
#[derive(Debug, Clone)]
pub struct PluginInserterItem {
    /// The block type name (e.g. `"lopress:callout"`). Used to construct
    /// `BlockKind::Opaque { type_name }` and `PluginMeta.block_type_name`.
    pub type_name: Rc<str>,
    /// Human-readable label shown in the slash menu. Derived from the
    /// manifest `title` field or, when absent, from the block `name`
    /// (stripping the `lopress:` prefix and title-casing).
    pub title: String,
    /// Category bucket for grouping in the menu. Falls back to `"Blocks"`.
    pub category: String,
    /// Attribute declarations from the manifest, in declaration order.
    pub attr_decls: Rc<[AttrDecl]>,
    /// Default attribute values: for each `AttrDecl` that has a `default`,
    /// the corresponding key→value pair. Used to seed the fresh block's
    /// `PluginMeta.attrs`.
    pub default_attrs: Map<String, Value>,
}

/// Derive a display title from a block name.
///
/// Strips a leading `lopress:` prefix (lower-cased) and title-cases the
/// remaining word(s) separated by `-`.
fn derive_title(name: &str) -> String {
    let stripped = name.strip_prefix("lopress:").unwrap_or(name);
    // Title-case each hyphen-separated segment.
    stripped
        .split('-')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the default-attrs map from a list of `AttrDecl`.
///
/// For each decl that has a `default`, include the key→value pair.
fn build_default_attrs(attrs: &std::collections::BTreeMap<String, AttrDecl>) -> Map<String, Value> {
    attrs
        .iter()
        .filter_map(|(k, v)| v.default.as_ref().map(|d| (k.clone(), d.clone())))
        .collect()
}

/// Compute the list of insertable plugin blocks from the registry.
///
/// A `BlockDecl` is offered when:
///   `(template.is_some() || markdown_template.is_some()) && !builtin && native.is_none()`
///
/// Items are returned in registration order (plugin order, then block order
/// within each plugin).
pub fn inserter_items(registry: &PluginRegistry) -> Vec<PluginInserterItem> {
    registry
        .plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .manifest
                .blocks
                .iter()
                .filter(|decl| is_insertable(decl))
                .map(move |decl| make_item(plugin, decl))
        })
        .collect()
}

/// True when the block is a comment-container plugin block eligible for
/// insertion from the slash menu.
fn is_insertable(decl: &lopress_plugin::BlockDecl) -> bool {
    let has_template = decl.template.is_some() || decl.markdown_template.is_some();
    !decl.builtin && decl.native.is_none() && has_template
}

/// Build a single `PluginInserterItem` from a plugin + block decl pair.
fn make_item(plugin: &LoadedPlugin, decl: &lopress_plugin::BlockDecl) -> PluginInserterItem {
    let type_name: Rc<str> = decl.name.clone().into();
    let title = decl
        .title
        .clone()
        .unwrap_or_else(|| derive_title(&decl.name));
    let category = decl.category.clone().unwrap_or_else(|| "Blocks".to_string());
    let attr_decls: Rc<[AttrDecl]> = Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>());
    let default_attrs = build_default_attrs(&decl.attrs);

    PluginInserterItem {
        type_name,
        title,
        category,
        attr_decls,
        default_attrs,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopress_plugin::{AttrType, BlockDecl, LoadedPlugin, PluginManifest};
    use std::collections::BTreeMap;

    fn make_plugin(
        name: &str,
        blocks: Vec<BlockDecl>,
    ) -> LoadedPlugin {
        LoadedPlugin {
            root: std::path::PathBuf::from("/fake"),
            manifest: PluginManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                theme: false,
                blocks,
            },
        }
    }

    fn make_decl(
        block_name: &str,
        template: Option<&str>,
        markdown_template: Option<&str>,
        builtin: bool,
        native: Option<&str>,
        title: Option<&str>,
        category: Option<&str>,
    ) -> BlockDecl {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "foo".to_string(),
            AttrDecl {
                kind: AttrType::String,
                required: false,
                default: Some(Value::String("bar".to_string())),
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        BlockDecl {
            name: block_name.to_string(),
            template: template.map(String::from),
            markdown_template: markdown_template.map(String::from),
            attrs,
            renderer: None,
            editor: None,
            builtin,
            native: native.map(String::from),
            css: Vec::new(),
            js: Vec::new(),
        }
    }

    #[test]
    fn filters_out_builtin_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "base",
            vec![make_decl("list", None, None, true, Some("list"), None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "builtin/native blocks must be excluded");
    }

    #[test]
    fn filters_out_native_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "ext",
            vec![make_decl("lopress:embed", None, None, false, Some("embed"), None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "native blocks must be excluded");
    }

    #[test]
    fn includes_markdown_template_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "callout",
            vec![make_decl(
                "lopress:callout",
                None,
                Some("blocks/callout.md"),
                false,
                None,
                Some("Callout"),
                Some("Text"),
            )],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert_eq!(items.len(), 1);
        assert_eq!(&*items[0].type_name, "lopress:callout");
        assert_eq!(items[0].title, "Callout");
        assert_eq!(items[0].category, "Text");
    }

    #[test]
    fn includes_html_template_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "button",
            vec![make_decl(
                "lopress:button",
                Some("blocks/button.html"),
                None,
                false,
                None,
                None,
                None,
            )],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert_eq!(items.len(), 1);
        assert_eq!(&*items[0].type_name, "lopress:button");
        // Title derived from name: "lopress:button" → "Button"
        assert_eq!(items[0].title, "Button");
        assert_eq!(items[0].category, "Blocks");
    }

    #[test]
    fn derives_title_from_name_when_absent() {
        assert_eq!(derive_title("lopress:author-bio"), "Author bio");
        assert_eq!(derive_title("lopress:callout"), "Callout");
        assert_eq!(derive_title("lopress:pull-quote"), "Pull quote");
        assert_eq!(derive_title("standalone"), "Standalone");
    }

    #[test]
    fn default_attrs_contains_decl_defaults() {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "foo".to_string(),
            AttrDecl {
                kind: AttrType::String,
                required: false,
                default: Some(Value::String("bar".to_string())),
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        attrs.insert(
            "baz".to_string(),
            AttrDecl {
                kind: AttrType::Bool,
                required: false,
                default: None,
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        let defaults = build_default_attrs(&attrs);
        assert_eq!(defaults.get("foo").and_then(Value::as_str), Some("bar"));
        assert!(!defaults.contains_key("baz"), "no default → omitted");
    }

    #[test]
    fn excludes_blocks_with_no_template() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "editor-only",
            vec![make_decl("lopress:foo", None, None, false, None, None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "blocks without template/markdown_template are excluded");
    }
}
```

- [ ] **Step 2: Add the module export to `model/mod.rs`**

Add `pub mod inserter;` to `crates/lopress-editor/src/model/mod.rs`. Read the file first to find the right location:

```bash
grep -n 'pub mod' crates/lopress-editor/src/model/mod.rs
```

Add the line after the existing `pub mod` declarations (order doesn't matter for compilation).

- [ ] **Step 3: Compile-check the new module**

Run: `cargo build -p lopress-editor`
Expected: success.

- [ ] **Step 4: Run the inserter tests**

Run: `cargo test -p lopress-editor inserter_tests`
Expected: PASS — all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/model/inserter.rs crates/lopress-editor/src/model/mod.rs
git commit -m "feat(editor): add PluginInserterItem type and inserter_items() filter function"
```

### 2b: Add `EditorBlock::from_plugin_item` constructor

- [ ] **Step 1: Add the constructor to `types.rs`**

In `crates/lopress-editor/src/model/types.rs`, after the `EditorBlock::image` method (~line 175), add:

```rust
    /// A fresh plugin comment-container block for insertion from the slash menu.
    ///
    /// The block is `Opaque` with an empty body (`Value::Null`) and carries
    /// `PluginMeta` with default attribute values from the inserter item.
    /// This shape round-trips through `to_core` as a `<!-- lopress:NAME {attrs} -->`
    /// / `<!-- /lopress:NAME -->` comment container.
    pub fn from_plugin_item(item: &crate::model::inserter::PluginInserterItem) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Opaque {
                type_name: item.type_name.clone(),
            },
            body: BlockBody::Opaque(Value::Null),
            plugin: Some(PluginMeta {
                block_type_name: item.type_name.clone(),
                attrs: item.default_attrs.clone(),
                attr_decls: item.attr_decls.clone(),
                builtin: false,
                editor: None,
                native: None,
            }),
        }
    }
```

- [ ] **Step 2: Add an import guard test**

In `crates/lopress-editor/src/model/types.rs`, in the existing `#[cfg(test)]` section (at the bottom), add:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod plugin_inserter_ctor_tests {
    use super::*;
    use crate::model::inserter::PluginInserterItem;
    use lopress_plugin::AttrDecl;
    use serde_json::Map;
    use std::rc::Rc;

    fn test_item() -> PluginInserterItem {
        let mut attrs = Map::new();
        attrs.insert("foo".to_string(), Value::String("bar".to_string()));
        PluginInserterItem {
            type_name: Rc::from("lopress:test"),
            title: "Test".to_string(),
            category: "Blocks".to_string(),
            attr_decls: Rc::from([]),
            default_attrs: attrs,
        }
    }

    #[test]
    fn from_plugin_item_builds_opaque_block_with_meta() {
        let item = test_item();
        let b = EditorBlock::from_plugin_item(&item);
        assert!(matches!(b.kind, BlockKind::Opaque { .. }));
        if let BlockKind::Opaque { type_name } = &b.kind {
            assert_eq!(&**type_name, "lopress:test");
        } else {
            panic!("expected Opaque kind");
        }
        assert!(matches!(b.body, BlockBody::Opaque(serde_json::Value::Null)));
        let meta = b.plugin.as_ref().expect("plugin meta present");
        assert_eq!(&*meta.block_type_name, "lopress:test");
        assert_eq!(meta.attrs.get("foo").and_then(Value::as_str), Some("bar"));
        assert!(!meta.builtin);
        assert!(meta.editor.is_none());
        assert!(meta.native.is_none());
    }
}
```

- [ ] **Step 3: Run the constructor test**

Run: `cargo test -p lopress-editor plugin_inserter_ctor_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs
git commit -m "feat(editor): add EditorBlock::from_plugin_item constructor"
```

---

## Task 3: `SlashChoice::Plugin` variant

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (add variant)

- [ ] **Step 1: Add the `Plugin` variant to `SlashChoice`**

In `crates/lopress-editor/src/ui/slash_menu.rs`, change the enum from:

```rust
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
}
```

To:

```rust
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
    Plugin { type_name: Rc<str> },
}
```

- [ ] **Step 2: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: success — no other code references `SlashChoice` yet, so this is just the enum change.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs
git commit -m "feat(editor): add SlashChoice::Plugin variant"
```

---

## Task 4: Thread `inserter_items` through `editor_pane` and append plugin rows

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (change label type `&'static str` → `String`)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (new param, plugin rows, selection arm)

> **Implementer note:** Step 3 below contains an exploratory first attempt followed by a
> **"Revised Step 3"** — IGNORE the first attempt and apply the revised version (change
> `slash_menu_items()` and `slash_menu()` to use `String` labels). The revised code is verified
> correct against the real `slash_menu.rs`. Stage `slash_menu.rs` in this task's commit too.

### 4a: Add `inserter_items` parameter to `editor_pane`

- [ ] **Step 1: Add the parameter to `editor_pane` signature**

In `crates/lopress-editor/src/ui/editor_pane.rs`, find the function signature (~line 30). Change:

```rust
pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
    on_insert_image: Rc<dyn Fn(BlockId)>,
) -> impl IntoView {
```

To:

```rust
#[allow(clippy::too_many_arguments)]
pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
    on_insert_image: Rc<dyn Fn(BlockId)>,
    inserter_items: Rc<[crate::model::inserter::PluginInserterItem]>,
) -> impl IntoView {
```

Remove the existing `#[allow(clippy::too_many_arguments)]` if present (the new param is needed for the allow).

- [ ] **Step 2: Compile-check (will fail at call site)**

Run: `cargo build -p lopress-editor`
Expected: FAIL — the call site in `ui/mod.rs` doesn't pass the new param yet.

- [ ] **Step 3: Build plugin rows in the slash menu overlay**

In `editor_pane.rs`, find the slash menu overlay `dyn_container` (around lines 85-135). After collecting the built-in `items` and before calling `slash_menu`, add plugin rows:

Replace:
```rust
                let items: Vec<_> = crate::ui::slash_menu::slash_menu_items()
                    .into_iter()
                    .filter(|(_, choice)| !(has_more && matches!(choice, SlashChoice::ReadMore)))
                    .collect();
```

With:

```rust
                let items: Vec<_> = crate::ui::slash_menu::slash_menu_items()
                    .into_iter()
                    .filter(|(_, choice)| !(has_more && matches!(choice, SlashChoice::ReadMore)))
                    .collect();

                // Append plugin block rows. Build a flat list (grouping by
                // category is a nice-to-have; flat append is acceptable for MVP).
                let mut plugin_rows: Vec<(String, SlashChoice)> = Vec::new();
                for item in inserter_items.iter() {
                    plugin_rows.push((
                        item.title.clone(),
                        SlashChoice::Plugin {
                            type_name: item.type_name.clone(),
                        },
                    ));
                }
                // The slash_menu signature uses `(&'static str, SlashChoice)`
                // tuples. Plugin titles are owned Strings, so we need to
                // convert the items to a compatible form. We'll use a
                // `Vec<(Box<str>, SlashChoice)>` approach — but the menu
                // function takes `Vec<(&'static str, SlashChoice)>`. The
                // simplest fix: change the menu to accept `Vec<(String, SlashChoice)>`
                // and update the built-in items to use `String` labels.
```

Actually, looking at the code more carefully, the `slash_menu` function takes `Vec<(&'static str, SlashChoice)>`. Plugin titles are owned `String`s. The smallest clean change: modify `slash_menu_items()` and `slash_menu()` to use `String` labels instead of `&'static str`. This is the minimal change that compiles.

Let me revise the approach. The `slash_menu` function signature needs to accept owned labels. Here's the updated plan:

**Revised Step 3: Change label type from `&'static str` to `String`**

In `crates/lopress-editor/src/ui/slash_menu.rs`, change `slash_menu_items()` to return `Vec<(String, SlashChoice)>`:

```rust
pub fn slash_menu_items() -> Vec<(String, SlashChoice)> {
    vec![
        ("Paragraph".to_string(), SlashChoice::Kind(BlockKind::Paragraph)),
        ("Heading 1".to_string(), SlashChoice::Kind(BlockKind::Heading(1))),
        ("Heading 2".to_string(), SlashChoice::Kind(BlockKind::Heading(2))),
        ("Heading 3".to_string(), SlashChoice::Kind(BlockKind::Heading(3))),
        (
            "Code block".to_string(),
            SlashChoice::Kind(BlockKind::Code { lang: Rc::from("") }),
        ),
        (
            "Unordered list".to_string(),
            SlashChoice::Kind(BlockKind::List { ordered: false }),
        ),
        (
            "Ordered list".to_string(),
            SlashChoice::Kind(BlockKind::List { ordered: true }),
        ),
        ("Image".to_string(), SlashChoice::Image),
        ("Read more".to_string(), SlashChoice::ReadMore),
    ]
}
```

Change `slash_menu` signature from `items: Vec<(&'static str, SlashChoice)>` to `items: Vec<(String, SlashChoice)>`:

```rust
pub fn slash_menu<F, C>(
    items: Vec<(String, SlashChoice)>,
    on_select: F,
    on_close: C,
) -> impl IntoView
where
    F: Fn(SlashChoice) + Clone + 'static,
    C: Fn() + Clone + 'static,
{
    let len = items.len();
    let highlight: RwSignal<usize> = RwSignal::new(0);

    let items_for_key: Vec<_> = items.clone();
    let mut rows: Vec<AnyView> = Vec::with_capacity(len);
    for (i, (lbl, choice)) in items.into_iter().enumerate() {
        let on_select_for_row = on_select.clone();
        let on_close_for_row = on_close.clone();
        let choice_for_row = choice.clone();
        let row = label(move || lbl.clone())
            .on_click_stop(move |_| {
                on_select_for_row(choice_for_row.clone());
                on_close_for_row();
            })
            .on_event(EventListener::PointerEnter, move |_| {
                highlight.set(i);
                EventPropagation::Continue
            })
            .style(move |s| {
                let s = s.padding_horiz(8.).padding_vert(4.).width_full();
                if highlight.get() == i {
                    s.background(HIGHLIGHT_BG)
                } else {
                    s
                }
            });
        rows.push(row.into_any());
    }

    // ... rest of the function unchanged (keyboard handling, popup style, etc.)
```

In the keyboard handler, change `items_for_key` from `Vec<_>` to `Vec<(String, SlashChoice)>` — the type inference should handle it, but the `.get(idx)` access returns `Option<&(String, SlashChoice)>`:

```rust
                Key::Named(NamedKey::Enter) => {
                    let idx = highlight.get();
                    if let Some((_, choice)) = items_for_key.get(idx) {
                        on_select_for_key(choice.clone());
                    }
                    on_close_for_key();
                    EventPropagation::Stop
                }
```

This is the same pattern — the `&str` vs `String` destructure works the same way since we only use `choice`.

- [ ] **Step 4: Append plugin rows in `editor_pane.rs`**

In `editor_pane.rs`, after collecting built-in items, append plugin rows:

```rust
                // Append plugin block rows after the built-in entries.
                let mut plugin_rows: Vec<(String, SlashChoice)> = Vec::new();
                for item in inserter_items.iter() {
                    plugin_rows.push((
                        item.title.clone(),
                        SlashChoice::Plugin {
                            type_name: item.type_name.clone(),
                        },
                    ));
                }
                let items: Vec<_> = items
                    .into_iter()
                    .chain(plugin_rows.into_iter())
                    .collect();
```

- [ ] **Step 5: Add the selection arm for `SlashChoice::Plugin`**

In the `on_select` match inside the slash menu overlay, add:

```rust
                    SlashChoice::Plugin { type_name } => {
                        if let Some(item) = inserter_items
                            .iter()
                            .find(|i| &*i.type_name == type_name)
                        {
                            on_action_for_select(BlockAction::InsertAfter {
                                anchor: block_id,
                                new_block: Box::new(EditorBlock::from_plugin_item(item)),
                            });
                        }
                    }
```

- [ ] **Step 6: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: FAIL — the call site in `ui/mod.rs` doesn't pass `inserter_items` yet.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs
git commit -m "feat(editor): thread inserter_items through editor_pane and append plugin rows"
```

---

## Task 5: Compute `inserter_items` in `ui/mod.rs` and pass it through

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs` (compute and pass `inserter_items`)

> **Implementer note:** `EditingState` has a real `plugin_registry: PluginRegistry` field
> (`crates/lopress-editor/src/state.rs:31`), so `s.plugin_registry.clone()` is correct. The
> `editing_view` function (`ui/mod.rs:151`) holds `editing: Rc<RefCell<Option<EditingState>>>` and
> already reads it via `editing.borrow().as_ref().map(|s| s.session.workspace())` (~line 161) —
> mirror that exact access pattern. Step 2 shows a first attempt then a simpler **final** version
> (the `Rc::from(... .into_boxed_slice())` one) — apply the FINAL version. `PluginRegistry: Default`
> holds, so `unwrap_or_default()` is fine.

- [ ] **Step 1: Read the `editing_view` function to find the right insertion point**

The `editing_view` function in `ui/mod.rs` has access to the `editing` `Rc<RefCell<Option<EditingState>>>`. The `EditingState` has a `plugin_registry: PluginRegistry` field. We need to compute the inserter items once at view-build time (same pattern as `workspace_signal`).

- [ ] **Step 2: Compute `inserter_items` from the editing state**

In `editing_view`, after the `workspace_signal` initialization (~line 170), add:

```rust
    // Compute the plugin inserter list once at view-build time. The registry
    // is stable for a loaded workspace; recomputing per keystroke is wasteful.
    let initial_inserter_items: Rc<[crate::model::inserter::PluginInserterItem]> = Rc::from(
        crate::model::inserter::inserter_items(
            &editing
                .borrow()
                .as_ref()
                .map(|s| s.plugin_registry.clone())
                .unwrap_or_default(),
        )
        .into_boxed_slice()
        .as_ref()
        .clone(),
    );
    let inserter_items: RwSignal<Rc<[crate::model::inserter::PluginInserterItem]>> =
        RwSignal::new(initial_inserter_items);
```

Actually, looking at the pattern more carefully — `on_undo`, `on_redo`, `on_insert_image` are all `Rc<dyn Fn()>` or `Rc<dyn Fn(BlockId)>` that are cloned into the `dyn_container` closure. The `inserter_items` should follow the same pattern: an `Rc<[PluginInserterItem]>` cloned into the closure.

Since `inserter_items` is only read (never mutated) after construction, we can just use `Rc<[PluginInserterItem]>` directly:

```rust
    // Compute the plugin inserter list once at view-build time. The registry
    // is stable for a loaded workspace; recomputing per keystroke is wasteful.
    let initial_inserter_items: Rc<[crate::model::inserter::PluginInserterItem]> = Rc::from(
        crate::model::inserter::inserter_items(
            &editing
                .borrow()
                .as_ref()
                .map(|s| s.plugin_registry.clone())
                .unwrap_or_default(),
        )
        .into_boxed_slice(),
    );
```

- [ ] **Step 3: Clone and pass to the `editor_pane` call**

In the `dyn_container` that builds the editor pane, find the `editor_pane::editor_pane(&doc, …)` call. Add `inserter_items_for_pane.clone()` as the last argument. Clone it before the closure:

```rust
    let inserter_items_for_pane = Rc::clone(&initial_inserter_items);
```

Then in the `editor_pane` call site, add it as the final parameter after `on_insert_image_for_pane.clone()`.

- [ ] **Step 4: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(editor): compute inserter_items from Session registry and pass to editor_pane"
```

---

## Task 6: Full gate + e2e verification

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: fmt + `cargo clippy --workspace --all-targets -D warnings` + `cargo test --workspace` pass.

> **Clippy caching note:** clippy can falsely pass on cached crates after a prior `cargo test/run/build`. If you get a green clippy but suspect stale results, force a re-lint:
> ```bash
> touch crates/lopress-plugin/src/manifest.rs crates/lopress-editor/src/model/inserter.rs crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/src/ui/mod.rs
> cargo clippy --workspace --all-targets -- -D warnings
> ```

- [ ] **Step 2: End-to-end via control server (driving-lopress-editor skill)**

Via the `127.0.0.1:7878` control server:

1. **Scaffold a throwaway workspace** under `$env:TEMP` with
   `target\debug\lopress.exe new <TEMP_DIR> --title "Test" --base-url "http://localhost:3000"`
   (a hand-rolled `lopress.toml` makes `/open` return 404 — `Session::open` needs a real theme).
   Then copy the repo's `plugins/callout` into `<TEMP_DIR>\plugins\callout` and write a post under
   `<TEMP_DIR>\src\posts\test.md` with a `Hello` paragraph.

2. **Launch the editor** — plain `cargo run` from the repo root (the bin is the root `lopress`
   crate; `-p lopress-editor`/`-p lopress-gui-host` have no runnable bin). Visible, non-minimized
   window. Poll `/ping` until `ok` (cold build is minutes); never `--release`.

3. **Open the post** — `/open` with the ABSOLUTE path (first open bootstraps the workspace).

4. **Trigger the slash menu** — focus an empty paragraph and inject `/` via `/input`
   (`{"type":"text","text":"/"}`), or open it however the editor triggers `OpenSlashMenu`.

5. **Verify the plugin entry appears** — `/screenshot` and confirm a "Callout" row appears below
   the built-in entries.

6. **Select it** — Down-arrow to the Callout row + Enter via `/input` keys, or click it.

7. **Verify insertion** — re-read `/state`; confirm a new block with `kind` `Opaque(lopress:callout)`
   (the control server serializes plugin blocks via the Opaque arm). `/screenshot` to confirm the
   callout attr form (variant select, title, body textarea) rendered.

8. **Save and verify round-trip** — save; confirm `<TEMP_DIR>\src\posts\test.md` gained a
   `<!-- lopress:callout {…} -->\n<!-- /lopress:callout -->` comment container.

Record verbatim commands + outputs; `dispatched`/`200` ≠ effect happened (re-read `/state`). No
PASS without evidence. Clean up the temp workspace afterward.

- [ ] **Step 3: Commit any gate fixes (named files only)**

Stage only the files you changed — never `git add -A` (the tree has untracked `.pi-delegations/`
briefs and temp artifacts that must not be swept in):

```bash
git add crates/lopress-plugin/src/manifest.rs crates/lopress-editor/src/model/inserter.rs crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/model/mod.rs crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "chore: gate pass for dynamic plugin inserter"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage:** manifest fields (Task 1, already done by template-form work), `PluginInserterItem` type + `inserter_items()` filter + `EditorBlock::from_plugin_item` constructor (Task 2), `SlashChoice::Plugin` variant with label-type change (Task 3), `editor_pane` threading + plugin rows + selection arm (Task 4), `ui/mod.rs` computation from Session registry (Task 5), gate + e2e (Task 6).
- **Label type change:** `slash_menu_items()` returns `Vec<(String, SlashChoice)>` instead of `Vec<(&'static str, SlashChoice)>`. This is the minimal change that allows owned plugin titles to mix with built-in entries. The `slash_menu` function adapts accordingly. No `'static` lifetime issues since labels are owned.
- **Filter logic:** `(template.is_some() || markdown_template.is_some()) && !builtin && native.is_none()` — excludes base plugins (list, code, image, more) from the duplicate listing and auto-includes any future plugin block.
- **`from_plugin_item` shape:** `Opaque` kind, `Opaque(Value::Null)` body, `PluginMeta` with `builtin: false`, `editor: None`, `native: None`, `attrs` seeded from `AttrDecl.default`. This matches the comment-container round-trip contract proven by `plugin_block_to_core`.
- **`inserter_items` computed once:** at the editing-view boundary (same pattern as `workspace_signal`), passed as `Rc<[PluginInserterItem]>` down through `ui/mod.rs → editor_pane`. The registry is stable per loaded workspace.
- **No clippy `too_many_arguments`:** the `#[allow(clippy::too_many_arguments)]` on `editor_pane` is justified — the 10th parameter is the inserter list, needed for the slash menu to offer plugin blocks. Without it, the function wouldn't compile under `-D warnings`.
- **Commit style:** conventional commits scoped by crate, matching git history. One commit per task.

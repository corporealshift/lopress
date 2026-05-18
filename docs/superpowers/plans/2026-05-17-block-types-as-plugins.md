# Block Types as Plugins — Plugin Capability Model & List Migration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the plugin layer real control over the list block — its manifest drives editing dispatch and markdown serialization — and build the reusable machinery so paragraph/heading/code can migrate the same way later.

**Architecture:** Add three capability fields to the plugin manifest (`editor`, `native`, `css`/`js`). Build an editor registry (a `match`-based `editor_for` function mapping an editor key to a widget) and a native-block registry (core-type → block, on `PluginRegistry`). Route `from_core`/`to_core` for `native`-claiming block types through generic paths; non-migrated built-ins keep their hardcoded arms. Only the list block is migrated end-to-end this round; `BlockKind` is retained.

**Tech Stack:** Rust workspace, `cargo test`, Floem GUI, `serde`/`toml`, `serde_json`. Spec: `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md`.

---

## File Structure

**Create:**
- `crates/lopress-editor/src/ui/blocks/editor_registry.rs` — `EditorContext`, `EditorWidget` type alias, `editor_for(key)` lookup, `list_editor_widget` adapter.

**Modify:**
- `crates/lopress-plugin/src/manifest.rs` — `BlockDecl` gains `native`, `css`, `js`.
- `crates/lopress-plugin/src/error.rs` — `PluginError::DuplicateNative`.
- `crates/lopress-plugin/src/registry.rs` — `native_index`, `native_block()`, duplicate-`native` check.
- `base_plugins/list/manifest.toml` — add `native = "list"`.
- `crates/lopress-editor/src/model/types.rs` — `PluginMeta` gains `editor`, `native`.
- `crates/lopress-editor/src/model/from_core.rs` — native-registry routing; delete the hardcoded `"list"` arm.
- `crates/lopress-editor/src/model/to_core.rs` — native-serialization branch; remove the `BlockKind::List` skip and match arm.
- `crates/lopress-editor/src/ui/blocks/plugin.rs` — `render_body` dispatches via `editor_for`.
- `crates/lopress-editor/src/ui/blocks/mod.rs` — register `editor_registry` module; remove the built-in `BlockKind::List` arm from `block_view`.
- Test files in `crates/lopress-editor/tests/` — load base plugins where lists are exercised.

**Conventions:** Tests are standard `#[test]` fns in `#[cfg(test)] mod tests` (unit) or files under `crates/*/tests/` (integration). Run with `cargo test -p <crate>`. Commit messages use Conventional Commits (`feat(plugin):`, `refactor(editor):`, `test(editor):`, `docs:`). Run all commands from the project root `C:\Users\corpo\Documents\projects\lopress`.

---

## Task 1: Add `native`/`css`/`js` fields to `BlockDecl`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs` (the `BlockDecl` struct, ~lines 16-33; tests module)

- [ ] **Step 1: Write the failing tests**

Add these two tests inside the existing `#[cfg(test)] mod tests` block in `crates/lopress-plugin/src/manifest.rs` (after `builtin_defaults_to_false`):

```rust
    #[test]
    fn parses_native_and_asset_fields() {
        let src = r#"
name = "lopress-list"
version = "0.1.0"

[[blocks]]
name    = "list"
editor  = "list"
native  = "list"
builtin = true
css     = ["assets/list.css"]
js      = ["assets/list.js"]
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert_eq!(b.native.as_deref(), Some("list"));
        assert_eq!(b.css, vec!["assets/list.css".to_string()]);
        assert_eq!(b.js, vec!["assets/list.js".to_string()]);
    }

    #[test]
    fn native_and_assets_default_to_empty() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert!(b.native.is_none());
        assert!(b.css.is_empty());
        assert!(b.js.is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p lopress-plugin parses_native_and_asset_fields native_and_assets_default_to_empty`
Expected: FAIL — compile error, `no field native on type BlockDecl`.

- [ ] **Step 3: Add the fields to `BlockDecl`**

In `crates/lopress-plugin/src/manifest.rs`, add three fields to the `BlockDecl` struct, after the existing `builtin` field:

```rust
    /// When true this block ships as part of the core codebase. The editor
    /// suppresses plugin chrome (header strip, attr form) for builtin blocks.
    #[serde(default)]
    pub builtin: bool,
    /// Capability #2 — Transform. When set, this block IS a native markdown
    /// construct identified by this `lopress_core` Block type. The value is an
    /// exclusive claim (see `PluginRegistry`). Absent → comment-container form.
    #[serde(default)]
    pub native: Option<String>,
    /// Capability #3 — Assets. CSS files this block contributes to the page
    /// `<head>`. Parsed and exposed; build-side injection is a follow-up.
    #[serde(default)]
    pub css: Vec<String>,
    /// Capability #3 — Assets. JS files this block contributes to the page
    /// `<head>`. Parsed and exposed; build-side injection is a follow-up.
    #[serde(default)]
    pub js: Vec<String>,
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p lopress-plugin`
Expected: PASS — all `lopress-plugin` tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-plugin/src/manifest.rs
git commit -m "feat(plugin): add native/css/js capability fields to BlockDecl"
```

---

## Task 2: Native-block registry and duplicate-`native` enforcement

**Files:**
- Modify: `crates/lopress-plugin/src/error.rs` (the `PluginError` enum)
- Modify: `crates/lopress-plugin/src/registry.rs` (the `PluginRegistry` struct, `insert`, new `native_block`; tests module)

- [ ] **Step 1: Write the failing tests**

Add these two tests inside the `#[cfg(test)] mod tests` block in `crates/lopress-plugin/src/registry.rs` (after `load_base_plugins_registers_the_list_block`). Also add `use crate::manifest::parse_manifest_str;` at the top of that `mod tests` block if not already imported:

```rust
    #[test]
    fn native_block_looks_up_by_core_type() {
        let mut reg = PluginRegistry::default();
        let m = parse_manifest_str(
            r#"
name = "x"
version = "0.1.0"

[[blocks]]
name   = "x:list"
native = "list"
"#,
        )
        .unwrap();
        reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: m,
        })
        .unwrap();
        let (_, decl) = reg.native_block("list").expect("list core type claimed");
        assert_eq!(decl.name, "x:list");
        assert!(reg.native_block("heading").is_none());
    }

    #[test]
    fn duplicate_native_claim_is_an_error() {
        let mut reg = PluginRegistry::default();
        let one = parse_manifest_str(
            r#"
name = "a"
version = "0.1.0"

[[blocks]]
name   = "a:list"
native = "list"
"#,
        )
        .unwrap();
        reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: one,
        })
        .unwrap();
        let two = parse_manifest_str(
            r#"
name = "b"
version = "0.1.0"

[[blocks]]
name   = "b:list"
native = "list"
"#,
        )
        .unwrap();
        let err = reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: two,
        });
        assert!(matches!(err, Err(PluginError::DuplicateNative(s)) if s == "list"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p lopress-plugin native_block_looks_up_by_core_type duplicate_native_claim_is_an_error`
Expected: FAIL — compile error, `no method native_block` / `no variant DuplicateNative`.

- [ ] **Step 3: Add the `DuplicateNative` error variant**

In `crates/lopress-plugin/src/error.rs`, add a variant to `PluginError` after `DuplicateBlock`:

```rust
    #[error("duplicate block name `{0}` across plugins")]
    DuplicateBlock(String),
    #[error("duplicate native claim `{0}` — two plugins claim the same core type")]
    DuplicateNative(String),
```

- [ ] **Step 4: Add `native_index`, the duplicate check, and `native_block`**

In `crates/lopress-plugin/src/registry.rs`, add a field to the `PluginRegistry` struct:

```rust
#[derive(Debug, Default, Clone)]
pub struct PluginRegistry {
    pub plugins: Vec<LoadedPlugin>,
    pub block_index: BTreeMap<String, (usize, usize)>,
    pub native_index: BTreeMap<String, (usize, usize)>,
    pub theme_index: BTreeMap<String, usize>,
}
```

Replace the block loop in `insert` with this version (adds the native check + index):

```rust
        for (bi, block) in plugin.manifest.blocks.iter().enumerate() {
            if self.block_index.contains_key(&block.name) {
                return Err(PluginError::DuplicateBlock(block.name.clone()));
            }
            if let Some(native) = &block.native {
                if self.native_index.contains_key(native) {
                    return Err(PluginError::DuplicateNative(native.clone()));
                }
            }
            self.block_index.insert(block.name.clone(), (pi, bi));
            if let Some(native) = &block.native {
                self.native_index.insert(native.clone(), (pi, bi));
            }
        }
```

Add a `native_block` accessor next to the existing `block` method:

```rust
    /// Look up the block that exclusively claims a native `core_type`.
    pub fn native_block(&self, core_type: &str) -> Option<(&LoadedPlugin, &BlockDecl)> {
        let (pi, bi) = *self.native_index.get(core_type)?;
        let plugin = self.plugins.get(pi)?;
        let decl = plugin.manifest.blocks.get(bi)?;
        Some((plugin, decl))
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p lopress-plugin`
Expected: PASS — all `lopress-plugin` tests green.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-plugin/src/error.rs crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): add native-block registry and duplicate-native enforcement"
```

---

## Task 3: List base plugin claims `native = "list"`

**Files:**
- Modify: `base_plugins/list/manifest.toml`
- Modify: `crates/lopress-plugin/src/registry.rs` (the `load_base_plugins_registers_the_list_block` test)

- [ ] **Step 1: Update the registry test to assert the native claim**

In `crates/lopress-plugin/src/registry.rs`, replace the `load_base_plugins_registers_the_list_block` test body with this stronger version:

```rust
    #[test]
    fn load_base_plugins_registers_the_list_block() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_, decl) = reg.block("list").expect("list block registered");
        assert!(decl.builtin);
        assert_eq!(decl.editor.as_deref(), Some("list"));
        assert_eq!(decl.native.as_deref(), Some("list"));
        assert!(decl.attrs.contains_key("ordered"));
        let (_, native_decl) = reg.native_block("list").expect("list claims native list");
        assert_eq!(native_decl.name, "list");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lopress-plugin load_base_plugins_registers_the_list_block`
Expected: FAIL — `assertion failed: decl.native.as_deref() == Some("list")` (currently `None`).

- [ ] **Step 3: Add `native = "list"` to the manifest**

In `base_plugins/list/manifest.toml`, change the block entry so it reads:

```toml
[[blocks]]
name    = "list"
editor  = "list"
native  = "list"
builtin = true

[blocks.attrs]
ordered = { type = "bool", ui = "hidden" }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p lopress-plugin`
Expected: PASS — all `lopress-plugin` tests green.

- [ ] **Step 5: Commit**

```bash
git add base_plugins/list/manifest.toml crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): list base plugin claims native markdown list"
```

---

## Task 4: `PluginMeta` carries the `editor` and `native` keys

The editor model needs the editor key and the native core type snapshotted onto each plugin block so `render_body` can dispatch (without a registry) and `to_core` can serialize natively (it receives no registry).

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs` (the `PluginMeta` struct, ~lines 83-91)
- Modify: `crates/lopress-editor/src/model/from_core.rs` (the two `PluginMeta` constructors: `plugin_block_from_core`, `list_plugin_meta`)

- [ ] **Step 1: Add the fields to `PluginMeta`**

In `crates/lopress-editor/src/model/types.rs`, extend `PluginMeta`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: String,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Vec<AttrDecl>,
    /// True when this block is owned by a built-in base plugin. The plugin
    /// block view suppresses chrome (header strip, attr form) when set.
    pub builtin: bool,
    /// The block's editor key (manifest `editor` field). Drives `render_body`
    /// dispatch via the editor registry. `None` → generic attr-form editor.
    pub editor: Option<String>,
    /// The native core type this block claims (manifest `native` field).
    /// `Some` → `to_core` serializes it as bare native markdown of this type.
    /// `None` → `to_core` uses the comment container.
    pub native: Option<String>,
}
```

- [ ] **Step 2: Update both `PluginMeta` constructors in `from_core.rs`**

In `crates/lopress-editor/src/model/from_core.rs`, in `plugin_block_from_core`, change the `PluginMeta { ... }` literal to:

```rust
    let plugin = PluginMeta {
        block_type_name: b.r#type.clone(),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    };
```

In the same file, in `list_plugin_meta`, change the `Some(PluginMeta { ... })` literal to:

```rust
    Some(PluginMeta {
        block_type_name: "list".to_string(),
        attrs,
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    })
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: builds clean — no other `PluginMeta` constructors exist (verify with `git grep "PluginMeta {" crates/lopress-editor/src` — only these two literals and the struct definition should match; if a test constructs one, add `editor: None, native: None`).

- [ ] **Step 4: Run the editor test suite to confirm no regression**

Run: `cargo test -p lopress-editor`
Expected: PASS — same tests green as before this task. If a test file fails to compile because it builds a `PluginMeta` literal, add `editor: None,` and `native: None,` to that literal and re-run.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/model/from_core.rs
git commit -m "feat(editor): snapshot editor and native keys onto PluginMeta"
```

---

## Task 5: Editor registry module

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (the `pub mod` declarations, ~lines 8-15)

- [ ] **Step 1: Create the module with `EditorContext`, `EditorWidget`, `editor_for`, and the list adapter**

Create `crates/lopress-editor/src/ui/blocks/editor_registry.rs` with this exact content:

```rust
//! Editor registry — data-driven dispatch from a manifest `editor` key to a
//! built-in editor widget.
//!
//! `editor_for(key)` maps an editor key string to an `EditorWidget`. The key
//! comes from a block's `PluginMeta.editor`, which is copied from the plugin
//! manifest — so dispatch is driven by the manifest, not the Rust `BlockKind`
//! enum. Only the `"list"` key is registered in this iteration; paragraph,
//! heading, and code keep their hardcoded arms in `render_body` until they
//! migrate the same way.

use crate::model::types::{BlockBody, BlockId, EditorBlock, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::ui::blocks::list;
use floem::reactive::RwSignal;
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Everything a built-in editor widget needs to render one block. Built once
/// per block by `render_body` and passed by reference to the widget.
pub struct EditorContext<'a> {
    pub block: &'a EditorBlock,
    pub on_action: ActionSink,
    pub focus_target: RwSignal<Option<BlockId>>,
    pub focus_pub: FocusPublisher,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}

/// A built-in editor widget constructor. A plain `fn` pointer so the registry
/// is a simple `match` with no boxing or global state.
pub type EditorWidget = fn(&EditorContext) -> AnyView;

/// Resolve an editor key to its widget. `None` for keys not (yet) registered.
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        _ => None,
    }
}

/// The `editor = "list"` widget. Adapts `EditorContext` to the list view:
/// pulls items from the block body and reads `ordered` from the manifest-
/// driven `PluginMeta.attrs`, not from the `BlockKind::List` enum.
fn list_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::List(items) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
    let ordered = ctx
        .block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("ordered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    list::editable_list_view(
        items,
        ctx.block.id,
        ordered,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_for_resolves_list_and_rejects_unknown() {
        assert!(editor_for("list").is_some());
        assert!(editor_for("paragraph").is_none());
        assert!(editor_for("bogus").is_none());
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add the module declaration alongside the other `pub mod` lines (keep alphabetical order — insert after `pub mod code;`):

```rust
pub mod code;
pub mod editor_registry;
pub mod heading;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test -p lopress-editor editor_for_resolves_list_and_rejects_unknown`
Expected: PASS.

- [ ] **Step 4: Confirm the crate still builds and tests stay green**

Run: `cargo test -p lopress-editor`
Expected: PASS — all editor tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/editor_registry.rs crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "feat(editor): add editor registry with the list editor widget"
```

---

## Task 6: Registry-driven `from_core` for native block types

Route list through a generic native path keyed on the core block type; delete the hardcoded `"list" =>` arm. This is a refactor guarded by the existing round-trip suite — list documents must still round-trip identically.

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs`
- Modify: `crates/lopress-editor/tests/from_to_core_tests.rs` (only if it builds a bare `PluginRegistry` — see Step 4)

- [ ] **Step 1: Replace the `"list"` arm with the native-registry path**

In `crates/lopress-editor/src/model/from_core.rs`, in `block_from_core`, delete the `"list" => list_from_core(b, registry),` arm and replace the `other =>` arm so the match reads:

```rust
fn block_from_core(b: &Block, registry: &PluginRegistry) -> EditorBlock {
    match b.r#type.as_str() {
        "paragraph" => {
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::paragraph(parse_inline(text))
        }
        "heading" => {
            let level = b
                .attrs
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::heading(level, parse_inline(text))
        }
        "code_block" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = b.text.clone().unwrap_or_default();
            EditorBlock::code(lang, text)
        }
        other => {
            if let Some((_plugin, decl)) = registry.native_block(other) {
                native_block_from_core(b, decl)
            } else if let Some((_plugin, decl)) = registry.block(other) {
                plugin_block_from_core(b, decl)
            } else {
                EditorBlock::opaque(
                    other.to_string(),
                    serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
                )
            }
        }
    }
}
```

- [ ] **Step 2: Replace `list_from_core` and `list_plugin_meta` with the native path**

In the same file, delete the `list_from_core` function and the `list_plugin_meta` function entirely, and add these two functions in their place:

```rust
/// Build an `EditorBlock` for a block type that claims a native markdown
/// construct. Dispatches on the editor key's implied body shape. `list` is
/// the only native type migrated so far; any other native editor key is
/// unreachable today and degrades to `Opaque` for a verbatim round-trip.
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

/// Native-list body parser. A list is convertible only if every `list_item`
/// child holds exactly one `paragraph` child with no further nesting;
/// otherwise the whole list becomes `Opaque` so its structure round-trips
/// verbatim. Convertible lists are stamped with `PluginMeta` so they route
/// through the plugin view and serialize back via `to_core`'s native branch.
fn native_list_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let ordered = b
        .attrs
        .get("ordered")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let items: Option<Vec<ListItem>> = if b.children.is_empty() {
        None
    } else {
        b.children
            .iter()
            .map(|item| {
                if item.r#type != "list_item" || item.children.len() != 1 {
                    return None;
                }
                let para = item.children.first()?;
                if para.r#type != "paragraph" || !para.children.is_empty() {
                    return None;
                }
                let text = para.text.as_deref().unwrap_or("");
                Some(ListItem {
                    id: BlockId::new(),
                    runs: parse_inline(text),
                })
            })
            .collect()
    };

    match items {
        Some(items) => {
            let mut block = EditorBlock::list(ordered, items);
            let mut attrs = Map::new();
            attrs.insert("ordered".to_string(), Value::Bool(ordered));
            block.plugin = Some(PluginMeta {
                block_type_name: decl.name.clone(),
                attrs,
                attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
                builtin: decl.builtin,
                editor: decl.editor.clone(),
                native: decl.native.clone(),
            });
            block
        }
        None => EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

Note: `list_items_from_block` is still used by `plugin_block_from_core` (the comment-path list editor) — leave it in place. The `registry` parameter of `block_from_core` is still used (`native_block`, `block`).

- [ ] **Step 3: Run the round-trip suite to verify it fails on bare registries**

Run: `cargo test -p lopress-editor`
Expected: list-related round-trip tests may FAIL — a test that builds a bare `PluginRegistry` (no `load_base_plugins()`) will now turn a core `list` block into `Opaque` (because `native_block("list")` returns `None`), changing the round-trip. This failure is expected and fixed in the next step.

- [ ] **Step 4: Ensure list-touching tests load base plugins**

Open `crates/lopress-editor/tests/from_to_core_tests.rs`. For every test that converts a document containing a list, find where its `PluginRegistry` is built and ensure `load_base_plugins()` is called on it before use. The pattern is:

```rust
let mut registry = lopress_plugin::PluginRegistry::default();
registry.load_base_plugins().unwrap();
```

If a test builds `PluginRegistry::default()` and passes it straight to `doc_from_core` without `load_base_plugins()`, insert the `load_base_plugins().unwrap()` call. (Tests that do not involve list blocks need no change.)

- [ ] **Step 5: Run the suite to verify it passes**

Run: `cargo test -p lopress-editor`
Expected: PASS — all editor tests green, list round-trip identical.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "refactor(editor): route from_core list conversion through the native registry"
```

---

## Task 7: Native serialization in `to_core`

**Files:**
- Modify: `crates/lopress-editor/src/model/to_core.rs`

- [ ] **Step 1: Write the failing test**

Add this test to `crates/lopress-editor/tests/from_to_core_tests.rs` (a focused native-serialization check). It builds an editor doc with a plugin-flagged list and asserts `to_core` emits a bare native `list` block, not a comment-wrapped one:

```rust
#[test]
fn native_list_block_serializes_to_a_core_list() {
    use lopress_editor::model::to_core::doc_to_core;
    use lopress_editor::model::types::{
        BlockBody, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
    };

    let mut block = EditorBlock::list(
        false,
        vec![ListItem {
            id: lopress_editor::model::types::BlockId::new(),
            runs: vec![InlineRun::plain("only item")],
        }],
    );
    let mut attrs = serde_json::Map::new();
    attrs.insert("ordered".to_string(), serde_json::Value::Bool(false));
    block.plugin = Some(PluginMeta {
        block_type_name: "list".to_string(),
        attrs,
        attr_decls: vec![],
        builtin: true,
        editor: Some("list".to_string()),
        native: Some("list".to_string()),
    });
    let _ = matches!(block.body, BlockBody::List(_));

    let doc = EditorDoc {
        blocks: vec![block],
        front_matter: lopress_core::FrontMatter::default(),
    };
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks.len(), 1);
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].children.len(), 1);
    assert_eq!(core.blocks[0].children[0].r#type, "list_item");
}
```

If `doc_to_core` / `to_core` items are not public, this test confirms the existing visibility; adjust the `use` paths to match how `from_to_core_tests.rs` already imports `doc_to_core` and the model types (copy the import style from the top of that file).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lopress-editor native_list_block_serializes_to_a_core_list`
Expected: FAIL — current `to_core` routes a plugin-flagged `BlockKind::List` to the built-in match arm; with the upcoming change absent it would otherwise hit `plugin_block_to_core` (comment wrapper). It should fail with a `type`/`children` mismatch or compile cleanly only after Step 3.

- [ ] **Step 3: Add the native-serialization branch and remove the `BlockKind::List` skip**

In `crates/lopress-editor/src/model/to_core.rs`, replace `block_to_core` with this version (the `BlockKind::List` skip is gone; plugin blocks branch on `meta.native`; the built-in `BlockKind::List` match arm is removed):

```rust
fn block_to_core(b: &EditorBlock) -> Block {
    // Plugin-flagged blocks: a `native` claim serializes as bare native
    // markdown of that core type; otherwise the comment container is used.
    if let Some(meta) = &b.plugin {
        return match &meta.native {
            Some(core_type) => native_block_to_core(b, meta, core_type),
            None => plugin_block_to_core(b, meta),
        };
    }
    match (&b.kind, &b.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => Block {
            r#type: "heading".into(),
            attrs: json!({ "level": level }),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.clone(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            })
        }
        // kind / body mismatch shouldn't arise from the constructors, but if
        // it does, fall back to an empty paragraph rather than panic.
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    }
}

/// Serialize a `native`-claiming plugin block to its core markdown form.
/// Dispatches on the body shape; `list` is the only native type today.
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => {
            let ordered = meta
                .attrs
                .get("ordered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "ordered": ordered }),
                children: items
                    .iter()
                    .map(|i| Block {
                        r#type: "list_item".into(),
                        attrs: empty_attrs(),
                        children: vec![Block {
                            r#type: "paragraph".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&i.runs)),
                        }],
                        text: None,
                    })
                    .collect(),
                text: None,
            }
        }
        // Other body shapes belong to native types not yet migrated; emit a
        // typed block carrying the attrs rather than panicking.
        _ => Block {
            r#type: core_type.to_string(),
            attrs: Value::Object(meta.attrs.clone()),
            children: vec![],
            text: None,
        },
    }
}
```

`plugin_block_to_core` is unchanged — it remains the comment-container path. `BlockKind` is still imported and used (`Paragraph`/`Heading`/`Code`/`Opaque`).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p lopress-editor native_list_block_serializes_to_a_core_list`
Expected: PASS.

- [ ] **Step 5: Run the full editor suite for the round-trip safety net**

Run: `cargo test -p lopress-editor`
Expected: PASS — all editor tests green, list documents round-trip byte-identically.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/to_core.rs crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "refactor(editor): serialize native plugin blocks to bare markdown in to_core"
```

---

## Task 8: `render_body` dispatches via the editor registry

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs` (the `render_body` function, ~lines 297-345)

- [ ] **Step 1: Replace `render_body` with registry dispatch plus a hardcoded fallback**

In `crates/lopress-editor/src/ui/blocks/plugin.rs`, replace the entire `render_body` function with this version. It tries `editor_for(meta.editor)` first; editor keys not yet in the registry (paragraph, heading, code) fall back to the existing `BlockKind` match — consistent with the spec's "generic registry path with hardcoded fallback" approach:

```rust
fn render_body(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    use crate::ui::blocks::editor_registry::{editor_for, EditorContext};

    // Registry path: a manifest `editor` key with a registered widget wins.
    if let Some(key) = block.plugin.as_ref().and_then(|m| m.editor.as_deref()) {
        if let Some(widget) = editor_for(key) {
            let ctx = EditorContext {
                block,
                on_action: on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                on_undo: Rc::clone(&on_undo),
                on_redo: Rc::clone(&on_redo),
            };
            return widget(&ctx);
        }
    }

    // Fallback: editor keys not yet migrated to the registry (paragraph,
    // heading, code) still dispatch on the Rust `BlockKind` enum.
    let block_id = block.id;
    match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block_id,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
            on_undo,
            on_redo,
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block_id,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
            on_undo,
            on_redo,
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code::render_code(lang, text).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => list::editable_list_view(
            items,
            block_id,
            *ordered,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
        ),
        _ => floem::views::empty().into_any(),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: builds clean. The `list` import in `plugin.rs` is still used (fallback arm).

- [ ] **Step 3: Run the editor suite**

Run: `cargo test -p lopress-editor`
Expected: PASS — all editor tests green. List blocks now reach `editable_list_view` through `editor_for("list")`.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "refactor(editor): dispatch render_body through the editor registry"
```

---

## Task 9: Remove the built-in `BlockKind::List` arm from `block_view`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (the `block_view` body match, ~lines 85-126)

- [ ] **Step 1: Delete the list arm**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, in `block_view`, delete this arm from the `let body = match (&block.kind, &block.body) { ... }` expression:

```rust
        (BlockKind::List { ordered }, BlockBody::List(items)) => list::editable_list_view(
            items,
            block.id,
            *ordered,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
        ),
```

After deletion the match has arms for `Paragraph`, `Heading`, `Code`, `Opaque`, and the `_` mismatch fallback. Lists never reach this match: they always carry `PluginMeta` (base plugins are loaded at startup), so `block_view` returns early via the `block.plugin.is_some()` plugin path above.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: builds clean. If the compiler warns that the `list` module import is now unused in `mod.rs`, note that `pub mod list;` is a module declaration, not a `use` — it stays. Only remove a `use ...list...` line if one becomes genuinely unused.

- [ ] **Step 3: Run the editor suite**

Run: `cargo test -p lopress-editor`
Expected: PASS — all editor tests green.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "refactor(editor): drop the built-in list arm from block_view"
```

---

## Task 10: Test-context sweep and end-to-end verification

**Files:**
- Modify (as needed): `crates/lopress-editor/tests/list_plugin_meta_tests.rs`, `crates/lopress-editor/tests/plugin_block_tests.rs`, `crates/lopress-editor/tests/list_action_tests.rs`, and any other test under `crates/lopress-editor/tests/` that constructs list blocks.

- [ ] **Step 1: Audit every list-touching test for base-plugin loading**

Run: `git grep -l "PluginRegistry" crates/lopress-editor/tests`
For each file listed, open it and confirm: any test that calls `doc_from_core` (or otherwise converts a core document) with a document that contains a `list` block builds its `PluginRegistry` with `load_base_plugins()`. Where missing, add:

```rust
let mut registry = lopress_plugin::PluginRegistry::default();
registry.load_base_plugins().unwrap();
```

Tests that only exercise paragraph/heading/code need no change.

- [ ] **Step 2: Run the entire workspace test suite**

Run: `cargo test`
Expected: PASS — every crate's tests green. This is the round-trip safety net confirming the migration changed no behavior.

- [ ] **Step 3: Commit any test fixes**

```bash
git add crates/lopress-editor/tests
git commit -m "test(editor): load base plugins in list-touching test contexts"
```

(Skip this commit if Step 1 found nothing to change.)

- [ ] **Step 4: Build and launch the editor with the debug control server**

Build and run the editor GUI with its debug HTTP control server enabled (the control server on `127.0.0.1:7878`, per the `driving-lopress-editor` capability). Use the project's normal run command for the GUI host (`cargo run` on the workspace root binary). Confirm the control server answers before proceeding.

- [ ] **Step 5: Drive the end-to-end list verification through the control interface**

Using the `driving-lopress-editor` control interface, perform this sequence and confirm each step via document-state reads / screenshots:

1. Open a document that contains a bulleted list of at least three items.
2. Place the cursor in the second item and edit its text.
3. Press Enter mid-item to split it (`SplitListItem`) — confirm a new item appears.
4. Press Backspace at offset 0 of an item to merge it with the previous one (`MergeListItemWithPrev`) — confirm the items join.
5. Save the document.
6. Read the saved `.md` file and confirm the list is written as bare native markdown (`- item` lines, no `<!-- lopress:list -->` wrapper) and that re-opening the document reproduces the edited list.

- [ ] **Step 6: Record the e2e result**

If every step in Step 5 passes, the migration is verified end-to-end. If any step fails, treat it as a regression: use `superpowers:systematic-debugging` before claiming completion. Document the e2e outcome in the final task summary.

---

## Notes for the implementer

- **Round-trip is the safety net.** After Tasks 6, 7, and 9, `cargo test -p lopress-editor` must stay green — a list document must convert core → editor → core byte-identically. If it does not, stop and debug before continuing.
- **`BlockKind` stays.** Do not remove `BlockKind::List` or any other `BlockKind` variant. It is still constructed by `from_core` and used by action dispatch.
- **Only `list` migrates.** Do not register paragraph/heading/code in `editor_for`, and do not remove their arms in `block_view` or the `render_body` fallback.
- **No on-disk format change.** Lists stay bare `- item`. If the e2e check shows a comment wrapper around a list, `meta.native` is not being read in `to_core` — revisit Task 7.

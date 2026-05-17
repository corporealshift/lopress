# Editable List — List as a Base Plugin — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make list items fully editable in the Floem editor and migrate the list block onto the plugin infrastructure as the first built-in ("base") plugin.

**Architecture:** A new `base_plugins/` directory at the project root holds plugin manifests embedded at compile time via `include_str!`. `lopress-plugin` gains a string-parsing path and a `builtin` block flag. List blocks loaded from markdown keep `BlockKind::List` for serialization but also carry `PluginMeta`, routing them through `plugin_block_view`. List editing introduces three new `BlockAction` variants and a new editable list view with per-item native editors.

**Tech Stack:** Rust, Floem reactive UI, `lapce-xi-rope::Rope`, `toml`, `serde_json`.

**Spec:** `docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md`

### Reconciliation notes (spec vs. codebase)

The spec is a design document; three of its illustrative details do not match the current code. This plan resolves them as follows:

1. **Manifest format.** The spec's section-2 TOML (top-level `editor`/`builtin`, `[[attrs]]`) is illustrative. The real `PluginManifest` requires `name`, `version`, and a `[[blocks]]` array of `BlockDecl`. The base plugin manifest uses the real format.
2. **`template` is required + checked on disk.** Base plugins ship no HTML template (they have an editor, not a renderer). `BlockDecl.template` becomes `Option<String>` and the on-disk check is skipped when absent.
3. **List serialization.** Markdown lists serialize to core block type `"list"`. List blocks therefore keep `BlockKind::List` / `BlockBody::List` and serialize through the existing `to_core` list arm — `to_core` skips the plugin path for `BlockKind::List`. The `PluginMeta` they carry is purely for editor routing. This is the spec's "Level C seam."
4. **`block_view`'s built-in `List` arm is kept, not removed** (spec implementation-order step 8). Lists created at runtime via `ChangeType` have `plugin: None` and would otherwise lose their renderer. The arm is retained but re-pointed at the new editable list view, so both plugin-flagged and freshly-converted lists are editable.
5. **`EditListItem` action.** The spec's section 5 lists only `SplitListItem` and `MergeListItemWithPrev`, but persisting plain text edits to a single list item needs a third action. `EditListItem { block_id, item_id, new_runs }` is added alongside them.

---

### Task 1: `lopress-plugin` — string parsing, `builtin` flag, optional template

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs`
- Modify: `crates/lopress-plugin/src/loader.rs:33-40`
- Modify: `crates/lopress-plugin/src/lib.rs:25`

- [ ] **Step 1: Write failing tests for `parse_manifest_str` and the `builtin` flag**

Add to the `tests` module at the bottom of `crates/lopress-plugin/src/manifest.rs` (inside `mod tests`, after the existing tests):

```rust
    #[test]
    fn parses_manifest_from_str_with_builtin_block() {
        let src = r#"
name = "lopress-list"
version = "0.1.0"

[[blocks]]
name    = "list"
editor  = "list"
builtin = true

[blocks.attrs]
ordered = { type = "bool", ui = "hidden" }
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.name, "lopress-list");
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.name, "list");
        assert!(b.builtin);
        assert!(b.template.is_none());
        assert_eq!(b.editor.as_deref(), Some("list"));
        assert!(b.attrs.contains_key("ordered"));
    }

    #[test]
    fn builtin_defaults_to_false() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        assert!(!m.blocks[0].builtin);
        assert_eq!(m.blocks[0].template.as_deref(), Some("blocks/video.html"));
    }
```

- [ ] **Step 2: Run the tests — verify they fail**

```
cargo test -p lopress-plugin parses_manifest_from_str_with_builtin_block builtin_defaults_to_false 2>&1 | head -20
```

Expected: compile error — `parse_manifest_str` not found, `builtin` field not found.

- [ ] **Step 3: Make `template` optional and add `builtin` to `BlockDecl`**

In `crates/lopress-plugin/src/manifest.rs`, replace the `BlockDecl` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockDecl {
    pub name: String,
    /// HTML template path, relative to the plugin root. Absent for built-in
    /// ("base") plugins, which provide an editor rather than a renderer.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub attrs: BTreeMap<String, AttrDecl>,
    #[serde(default)]
    pub renderer: Option<String>,
    #[serde(default)]
    pub editor: Option<String>,
    /// When true this block ships as part of the core codebase. The editor
    /// suppresses plugin chrome (header strip, attr form) for builtin blocks.
    #[serde(default)]
    pub builtin: bool,
}
```

- [ ] **Step 4: Add `parse_manifest_str`**

In `crates/lopress-plugin/src/manifest.rs`, add after `parse_manifest`:

```rust
/// Parse a manifest from an in-memory TOML string. Used for base plugins
/// embedded via `include_str!`, which have no path on disk.
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })
}
```

- [ ] **Step 5: Update the loader's template existence check**

In `crates/lopress-plugin/src/loader.rs`, replace the `for block in &manifest.blocks` loop (lines ~33-40):

```rust
        for block in &manifest.blocks {
            if let Some(template) = &block.template {
                if !root.join(template).exists() {
                    return Err(PluginError::MissingTemplate {
                        name: block.name.clone(),
                        template: template.clone(),
                    });
                }
            }
        }
```

- [ ] **Step 6: Re-export `parse_manifest_str`**

In `crates/lopress-plugin/src/lib.rs`, update the manifest re-export line:

```rust
pub use manifest::{parse_manifest, parse_manifest_str, AttrDecl, AttrType, BlockDecl, PluginManifest};
```

(If `parse_manifest` was not previously re-exported, adding it here is harmless.)

- [ ] **Step 7: Run tests — verify they pass**

```
cargo test -p lopress-plugin
```

Expected: all tests pass (including the two new ones). The existing `parses_plugin_with_blocks_and_attrs` test reads `b.attrs["src"]` etc. — unaffected. `errors_on_invalid_toml` and the loader tests still pass because `template` is `Option`.

- [ ] **Step 8: Commit**

```
git add crates/lopress-plugin/src/manifest.rs crates/lopress-plugin/src/loader.rs crates/lopress-plugin/src/lib.rs
git commit -m "feat(plugin): add parse_manifest_str, builtin flag, optional template"
```

---

### Task 2: Base plugin manifest + `load_base_plugins`

**Files:**
- Create: `base_plugins/list/manifest.toml`
- Modify: `crates/lopress-plugin/src/registry.rs`

- [ ] **Step 1: Create the base list plugin manifest**

Create `base_plugins/list/manifest.toml`:

```toml
# Built-in "base" plugin: the list block, dogfooding the plugin infrastructure.
# Embedded at compile time via include_str! — see PluginRegistry::load_base_plugins.
name    = "lopress-list"
version = "0.1.0"

[[blocks]]
name    = "list"
editor  = "list"
builtin = true

[blocks.attrs]
ordered = { type = "bool", ui = "hidden" }
```

- [ ] **Step 2: Write a failing test for `load_base_plugins`**

Add a `tests` module to `crates/lopress-plugin/src/registry.rs` (the file currently has none):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_base_plugins_registers_the_list_block() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_, decl) = reg.block("list").expect("list block registered");
        assert!(decl.builtin);
        assert_eq!(decl.editor.as_deref(), Some("list"));
        assert!(decl.attrs.contains_key("ordered"));
    }
}
```

- [ ] **Step 3: Run the test — verify it fails**

```
cargo test -p lopress-plugin load_base_plugins_registers_the_list_block 2>&1 | head -20
```

Expected: compile error — `load_base_plugins` not found.

- [ ] **Step 4: Implement `load_base_plugins`**

In `crates/lopress-plugin/src/registry.rs`, add the imports and method. At the top, ensure these imports exist:

```rust
use crate::error::PluginError;
use crate::manifest::{parse_manifest_str, BlockDecl, PluginManifest};
use std::collections::BTreeMap;
use std::path::PathBuf;
```

Add this method inside `impl PluginRegistry` (after `theme`):

```rust
    /// Register the built-in ("base") plugins shipped in the core codebase.
    /// Their manifests are embedded at compile time, so they are present
    /// regardless of the workspace's `plugins/` directory and cannot be
    /// removed by the user. Call this before loading user plugins so base
    /// blocks win any name collision.
    pub fn load_base_plugins(&mut self) -> Result<(), PluginError> {
        const LIST_MANIFEST: &str =
            include_str!("../../../base_plugins/list/manifest.toml");
        for src in [LIST_MANIFEST] {
            let manifest = parse_manifest_str(src)?;
            self.insert(LoadedPlugin {
                root: PathBuf::new(),
                manifest,
            })?;
        }
        Ok(())
    }
```

- [ ] **Step 5: Run the test — verify it passes**

```
cargo test -p lopress-plugin load_base_plugins_registers_the_list_block
```

Expected: PASS.

- [ ] **Step 6: Commit**

```
git add base_plugins/list/manifest.toml crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): embed base_plugins and add load_base_plugins"
```

---

### Task 3: Wire base plugins into the editor's startup sequence

**Files:**
- Modify: `crates/lopress-editor/src/state.rs:38-49`

`EditingState::new` currently takes the registry straight from `session.plugin_registry()` (user plugins only). It must seed base plugins first, then layer user plugins on top.

- [ ] **Step 1: Seed base plugins in `EditingState::new`**

In `crates/lopress-editor/src/state.rs`, replace the `new` method:

```rust
    /// Create a new `EditingState` wrapping the given `session`.
    ///
    /// The plugin registry is seeded with the built-in base plugins first,
    /// then user plugins from the workspace are layered on top. A user plugin
    /// that declares a block name already owned by a base plugin is rejected
    /// by `insert` (and silently skipped here) — base plugins are non-removable.
    pub fn new(session: Session) -> Self {
        let mut plugin_registry = PluginRegistry::default();
        if let Err(e) = plugin_registry.load_base_plugins() {
            eprintln!("failed to load base plugins: {e}");
        }
        for plugin in session.plugin_registry().plugins {
            // `insert` recomputes block/theme indices from the registry's
            // current length, so moving a `LoadedPlugin` across registries
            // is sound. Duplicate block names (e.g. a user plugin shadowing
            // a base block) are skipped.
            let _ = plugin_registry.insert(plugin);
        }
        Self {
            session,
            plugin_registry,
            current_doc: None,
            current_ref: None,
            last_error: None,
        }
    }
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors. `PluginRegistry` is already imported in `state.rs`.

- [ ] **Step 3: Commit**

```
git add crates/lopress-editor/src/state.rs
git commit -m "feat(editor): seed base plugins before user plugins at session open"
```

---

### Task 4: Model wiring — `PluginMeta.builtin`, list plugin meta, serialization passthrough

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs:83-88`
- Modify: `crates/lopress-editor/src/model/from_core.rs`
- Modify: `crates/lopress-editor/src/model/to_core.rs:18-21`
- Test: `crates/lopress-editor/tests/list_plugin_meta_tests.rs` (create)

- [ ] **Step 1: Add `builtin` to `PluginMeta`**

In `crates/lopress-editor/src/model/types.rs`, replace the `PluginMeta` struct:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: String,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Vec<AttrDecl>,
    /// True when this block is owned by a built-in base plugin. The plugin
    /// block view suppresses chrome (header strip, attr form) when set.
    pub builtin: bool,
}
```

- [ ] **Step 2: Write a failing test for list plugin-meta round-trip**

Create `crates/lopress-editor/tests/list_plugin_meta_tests.rs`:

```rust
#![allow(clippy::unwrap_used)]

use lopress_core::{Block, Document, FrontMatter};
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{BlockBody, BlockKind};
use lopress_plugin::PluginRegistry;

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::default();
    r.load_base_plugins().unwrap();
    r
}

fn list_doc() -> Document {
    Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "list".into(),
            attrs: serde_json::json!({ "ordered": true }),
            children: vec![Block {
                r#type: "list_item".into(),
                attrs: serde_json::json!({}),
                children: vec![Block {
                    r#type: "paragraph".into(),
                    attrs: serde_json::json!({}),
                    children: vec![],
                    text: Some("first".into()),
                }],
                text: None,
            }],
            text: None,
        }],
    }
}

#[test]
fn list_block_gets_plugin_meta_when_base_plugin_registered() {
    let editor_doc = doc_from_core(&list_doc(), &registry());
    let block = &editor_doc.blocks[0];
    assert!(matches!(block.kind, BlockKind::List { ordered: true }));
    assert!(matches!(block.body, BlockBody::List(_)));
    let meta = block.plugin.as_ref().expect("list block has plugin meta");
    assert_eq!(meta.block_type_name, "list");
    assert!(meta.builtin);
    assert_eq!(meta.attrs.get("ordered"), Some(&serde_json::Value::Bool(true)));
}

#[test]
fn list_block_serializes_back_to_core_list_type() {
    let editor_doc = doc_from_core(&list_doc(), &registry());
    let core = doc_to_core(&editor_doc);
    // Despite carrying plugin meta, the list must round-trip as a core
    // "list" block — not as a `<!-- lopress:... -->` plugin block.
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].children[0].r#type, "list_item");
}

#[test]
fn list_without_registered_base_plugin_has_no_plugin_meta() {
    let editor_doc = doc_from_core(&list_doc(), &PluginRegistry::default());
    assert!(editor_doc.blocks[0].plugin.is_none());
}
```

- [ ] **Step 3: Run the test — verify it fails**

```
cargo test -p lopress-editor --test list_plugin_meta_tests 2>&1 | head -25
```

Expected: failures — list blocks currently get `plugin: None`.

- [ ] **Step 4: Stamp plugin meta onto list blocks in `from_core`**

In `crates/lopress-editor/src/model/from_core.rs`:

First, change the `"list"` match arm in `block_from_core` to pass the registry:

```rust
        "list" => list_from_core(b, registry),
```

Then replace the `list_from_core` function:

```rust
fn list_from_core(b: &Block, registry: &PluginRegistry) -> EditorBlock {
    let ordered = b
        .attrs
        .get("ordered")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // A list is convertible only if every list_item child contains exactly one
    // paragraph child with no further nesting. Otherwise the whole list becomes
    // Opaque so its structure round-trips verbatim.
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
            // When the base list plugin is registered, route the block
            // through the plugin block view by stamping `PluginMeta`.
            // `BlockKind::List` is retained for serialization (see to_core).
            block.plugin = list_plugin_meta(registry, ordered);
            block
        }
        None => EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

/// Build `PluginMeta` for a list block from the registered base list plugin.
/// Returns `None` when no `"list"` block is registered (e.g. in tests that
/// build a bare registry) so lists degrade to the built-in dispatch.
fn list_plugin_meta(registry: &PluginRegistry, ordered: bool) -> Option<PluginMeta> {
    let (_, decl) = registry.block("list")?;
    let mut attrs = Map::new();
    attrs.insert("ordered".to_string(), Value::Bool(ordered));
    Some(PluginMeta {
        block_type_name: "list".to_string(),
        attrs,
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
    })
}
```

Then update `plugin_block_from_core` so its `PluginMeta` literal includes the new field. In `plugin_block_from_core`, replace the `let plugin = PluginMeta { ... };` block:

```rust
    let plugin = PluginMeta {
        block_type_name: b.r#type.clone(),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
    };
```

Ensure `PluginMeta` is in the `use crate::model::types::{...}` import at the top of `from_core.rs` (it is — `PluginMeta` is already imported).

- [ ] **Step 5: Skip the plugin path for `BlockKind::List` in `to_core`**

In `crates/lopress-editor/src/model/to_core.rs`, replace the opening of `block_to_core`:

```rust
fn block_to_core(b: &EditorBlock) -> Block {
    // List blocks carry `PluginMeta` purely for editor routing; they still
    // serialize as core "list" blocks via the match arm below. Every other
    // plugin-flagged block reconstructs through `plugin_block_to_core`.
    if let Some(meta) = &b.plugin {
        if !matches!(b.kind, BlockKind::List { .. }) {
            return plugin_block_to_core(b, meta);
        }
    }
    match (&b.kind, &b.body) {
```

(The rest of `block_to_core` is unchanged — the existing `(BlockKind::List { ordered }, BlockBody::List(items))` arm handles list serialization.)

- [ ] **Step 6: Run the new test — verify it passes**

```
cargo test -p lopress-editor --test list_plugin_meta_tests
```

Expected: all 3 tests pass.

- [ ] **Step 7: Run the full editor test suite and fix fallout**

```
cargo test -p lopress-editor 2>&1 | tail -40
```

Existing tests in `from_to_core_tests.rs` and `plugin_block_tests.rs` may build their own registries. Two failure modes to fix:
- A test that builds `doc_from_core` for a list and asserts `plugin.is_none()` — update it to use a registry with `load_base_plugins()` and assert `plugin.is_some()`, or keep a bare registry and assert `is_none()` (both are now valid; pick whichever matches the test's intent).
- A test calling `doc_from_core` whose registry argument no longer compiles — `block_from_core`'s signature is unchanged, so this should not occur; if `list_from_core`'s new `registry` argument surfaces a borrow error, it will be a compile error caught here.

Fix each failure minimally and re-run until green.

- [ ] **Step 8: Commit**

```
git add crates/lopress-editor/src/model/types.rs \
        crates/lopress-editor/src/model/from_core.rs \
        crates/lopress-editor/src/model/to_core.rs \
        crates/lopress-editor/tests/list_plugin_meta_tests.rs
git commit -m "feat(editor): route list blocks through plugin meta, keep list serialization"
```

---

### Task 5: New `BlockAction` variants + apply implementations

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs`
- Test: `crates/lopress-editor/tests/list_action_tests.rs` (create)

- [ ] **Step 1: Write failing tests for the list actions**

Create `crates/lopress-editor/tests/list_action_tests.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::types::{
    BlockBody, BlockId, EditorBlock, EditorDoc, InlineRun, ListItem,
};

fn item(text: &str) -> ListItem {
    ListItem { id: BlockId::new(), runs: vec![InlineRun::plain(text)] }
}

fn list_doc(items: Vec<ListItem>) -> EditorDoc {
    EditorDoc {
        blocks: vec![EditorBlock::list(false, items)],
        front_matter: lopress_core::FrontMatter::default(),
    }
}

fn items_of(doc: &EditorDoc) -> Vec<String> {
    match &doc.blocks[0].body {
        BlockBody::List(items) => items
            .iter()
            .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect())
            .collect(),
        _ => panic!("not a list"),
    }
}

#[test]
fn edit_list_item_replaces_runs() {
    let it0 = item("old");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs: vec![InlineRun::plain("new")],
        },
    );
    assert_eq!(items_of(&doc), vec!["new"]);
}

#[test]
fn split_list_item_inserts_new_item_after() {
    let it0 = item("hello world");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    // "hello " is 6 bytes.
    apply(
        &mut doc,
        BlockAction::SplitListItem { block_id, item_id, byte_offset: 6 },
    );
    assert_eq!(items_of(&doc), vec!["hello ", "world"]);
}

#[test]
fn merge_list_item_with_prev_joins_into_predecessor() {
    let it0 = item("foo");
    let it1 = item("bar");
    let item_id = it1.id;
    let mut doc = list_doc(vec![it0, it1]);
    let block_id = doc.blocks[0].id;
    apply(&mut doc, BlockAction::MergeListItemWithPrev { block_id, item_id });
    assert_eq!(items_of(&doc), vec!["foobar"]);
}

#[test]
fn merge_first_list_item_is_a_no_op() {
    let it0 = item("only");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    apply(&mut doc, BlockAction::MergeListItemWithPrev { block_id, item_id });
    assert_eq!(items_of(&doc), vec!["only"]);
}

#[test]
fn split_on_a_list_block_splits_the_containing_item() {
    // The ctrl API's `Split` command treats the list as items joined by '\n'.
    // Items "ab" (2) + '\n' (1) + "cd" — offset 4 lands inside "cd" at local 1.
    let mut doc = list_doc(vec![item("ab"), item("cd")]);
    let block_id = doc.blocks[0].id;
    apply(&mut doc, BlockAction::Split { block_id, byte_offset: 4 });
    assert_eq!(items_of(&doc), vec!["ab", "c", "d"]);
}
```

- [ ] **Step 2: Run the tests — verify they fail**

```
cargo test -p lopress-editor --test list_action_tests 2>&1 | head -20
```

Expected: compile error — `EditListItem`, `SplitListItem`, `MergeListItemWithPrev` not found.

- [ ] **Step 3: Add the three variants to `BlockAction`**

In `crates/lopress-editor/src/actions.rs`, add these variants inside the `enum BlockAction` (after `EditCode`):

```rust
    /// Replace the runs of a single list item. No-op when the block isn't a
    /// list or the item id is unknown.
    EditListItem {
        block_id: BlockId,
        item_id: BlockId,
        new_runs: Vec<InlineRun>,
    },
    /// Split a list item at `byte_offset` into the item's flat text. The
    /// trailing portion becomes a new `ListItem` directly after it.
    SplitListItem {
        block_id: BlockId,
        item_id: BlockId,
        byte_offset: usize,
    },
    /// Merge a list item into its predecessor item. No-op for the first item.
    MergeListItemWithPrev {
        block_id: BlockId,
        item_id: BlockId,
    },
```

- [ ] **Step 4: Add the variants to the `apply` dispatch**

In `crates/lopress-editor/src/actions.rs`, add these arms to the `match action` in `apply` (after the `EditCode` arm):

```rust
        BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs,
        } => apply_edit_list_item(doc, block_id, item_id, new_runs),
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset,
        } => apply_split_list_item(doc, block_id, item_id, byte_offset),
        BlockAction::MergeListItemWithPrev { block_id, item_id } => {
            apply_merge_list_item(doc, block_id, item_id)
        }
```

- [ ] **Step 5: Implement the apply functions and the shared split helper**

In `crates/lopress-editor/src/actions.rs`, add these functions (after `apply_edit_code`):

```rust
fn apply_edit_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    new_runs: Vec<InlineRun>,
) {
    let Some(idx) = find_idx(doc, block_id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    if let BlockBody::List(items) = &mut block.body {
        if let Some(item) = items.iter_mut().find(|it| it.id == item_id) {
            item.runs = new_runs;
        }
    }
}

fn apply_split_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    byte_offset: usize,
) {
    let Some(idx) = find_idx(doc, block_id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    if let BlockBody::List(items) = &mut block.body {
        if let Some(pos) = items.iter().position(|it| it.id == item_id) {
            split_item_at(items, pos, byte_offset);
        }
    }
}

fn apply_merge_list_item(doc: &mut EditorDoc, block_id: BlockId, item_id: BlockId) {
    let Some(idx) = find_idx(doc, block_id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else { return };
    if let BlockBody::List(items) = &mut block.body {
        let Some(pos) = items.iter().position(|it| it.id == item_id) else {
            return;
        };
        if pos == 0 {
            return;
        }
        let cur = items.remove(pos);
        if let Some(prev) = items.get_mut(pos - 1) {
            prev.runs.extend(cur.runs);
        }
    }
}

/// Split `items[pos]` at `byte_offset` into its flat text. The head stays in
/// place; the tail becomes a fresh `ListItem` inserted at `pos + 1`. Styling
/// is dropped on both sides (the split produces plain runs), matching the
/// behaviour of `apply_split` for paragraphs.
fn split_item_at(items: &mut Vec<ListItem>, pos: usize, byte_offset: usize) {
    let Some(item) = items.get(pos) else { return };
    let flat: String = item.runs.iter().map(|r| r.text.as_str()).collect();
    let safe_offset = flat
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(flat.len()))
        .find(|&b| b >= byte_offset)
        .unwrap_or(flat.len());
    let head = flat.get(..safe_offset).unwrap_or("").to_owned();
    let tail = flat.get(safe_offset..).unwrap_or("").to_owned();
    if let Some(item) = items.get_mut(pos) {
        item.runs = vec![InlineRun::plain(head)];
    }
    items.insert(
        pos + 1,
        ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain(tail)],
        },
    );
}
```

- [ ] **Step 6: Fix `apply_split` for list bodies**

In `crates/lopress-editor/src/actions.rs`, in `apply_split`, replace the trailing `_ => {}` arm of the `match body` with a `BlockBody::List` arm:

```rust
        BlockBody::List(items) => {
            // The ctrl API's `Split` command treats a list as the flat text
            // of its items joined by '\n'. Walk cumulative byte offsets to
            // find the item containing `byte_offset` and split it there.
            let mut cumulative = 0usize;
            let mut target: Option<(usize, usize)> = None;
            for (i, it) in items.iter().enumerate() {
                let item_len: usize = it.runs.iter().map(|r| r.text.len()).sum();
                if byte_offset <= cumulative + item_len {
                    target = Some((i, byte_offset - cumulative));
                    break;
                }
                cumulative += item_len + 1; // +1 for the joining '\n'
            }
            let (pos, local) = target.unwrap_or((items.len().saturating_sub(1), 0));
            if let Some(b) = doc.blocks.get_mut(idx) {
                if let BlockBody::List(list) = &mut b.body {
                    split_item_at(list, pos, local);
                }
            }
        }
```

Note: `apply_split` binds `let body = block.body.clone();` then matches on `body` by value, so the `BlockBody::List(items)` arm owns a clone of `items` — used here only for the offset walk; the real mutation re-borrows `doc.blocks[idx]`.

- [ ] **Step 7: Run the tests — verify they pass**

```
cargo test -p lopress-editor --test list_action_tests
```

Expected: all 5 tests pass.

- [ ] **Step 8: Commit**

```
git add crates/lopress-editor/src/actions.rs crates/lopress-editor/tests/list_action_tests.rs
git commit -m "feat(editor): add EditListItem/SplitListItem/MergeListItemWithPrev actions"
```

---

### Task 6: Undo inverses + `ui/mod.rs` wiring for list actions

**Files:**
- Modify: `crates/lopress-editor/src/undo.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`
- Test: `crates/lopress-editor/tests/undo_tests.rs` (append)

- [ ] **Step 1: Write failing undo tests for the list actions**

Append to `crates/lopress-editor/tests/undo_tests.rs` (the helpers `doc_with` and `para` already exist in that file):

```rust
use lopress_editor::model::types::{BlockBody, EditorBlock, ListItem};

fn list_item(text: &str) -> ListItem {
    ListItem { id: BlockId::new(), runs: vec![InlineRun::plain(text)] }
}

#[test]
fn inverse_of_edit_list_item_restores_old_runs() {
    let it0 = list_item("old");
    let item_id = it0.id;
    let list = EditorBlock::list(false, vec![it0]);
    let block_id = list.id;
    let doc = doc_with(vec![list]);
    let inv = compute_inverse(
        &doc,
        &BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs: vec![InlineRun::plain("new")],
        },
    )
    .unwrap();
    match inv {
        BlockAction::EditListItem { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("old")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_merge_list_item_is_split_at_join_point() {
    let it0 = list_item("foo");
    let it1 = list_item("bar");
    let prev_id = it0.id;
    let cur_id = it1.id;
    let list = EditorBlock::list(false, vec![it0, it1]);
    let block_id = list.id;
    let doc = doc_with(vec![list]);
    let inv = compute_inverse(
        &doc,
        &BlockAction::MergeListItemWithPrev { block_id, item_id: cur_id },
    )
    .unwrap();
    match inv {
        BlockAction::SplitListItem { item_id, byte_offset, .. } => {
            assert_eq!(item_id, prev_id);
            assert_eq!(byte_offset, 3); // "foo"
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn split_list_item_pushes_placeholder_then_fixes_it() {
    use lopress_editor::undo::UndoStack;
    let it0 = list_item("hello world");
    let item_id = it0.id;
    let list = EditorBlock::list(false, vec![it0]);
    let block_id = list.id;
    let mut doc = doc_with(vec![list]);
    let mut stack = UndoStack::new();

    let action = BlockAction::SplitListItem { block_id, item_id, byte_offset: 6 };
    stack.push_before_apply(&doc, &action);
    lopress_editor::actions::apply(&mut doc, action);
    assert_eq!(stack.undo_depth(), 1);

    // After apply, the new item is the second one.
    let new_item_id = match &doc.blocks[0].body {
        BlockBody::List(items) => items[1].id,
        _ => panic!("not a list"),
    };
    stack.fix_split_list_item_inverse(new_item_id);

    let undo = stack.pop_undo().unwrap();
    match undo {
        BlockAction::MergeListItemWithPrev { item_id, .. } => {
            assert_eq!(item_id, new_item_id);
        }
        _ => panic!("wrong variant"),
    }
}
```

- [ ] **Step 2: Run the tests — verify they fail**

```
cargo test -p lopress-editor --test undo_tests 2>&1 | head -20
```

Expected: compile error — `fix_split_list_item_inverse` not found / missing match arms.

- [ ] **Step 3: Handle `SplitListItem` in `push_before_apply`**

In `crates/lopress-editor/src/undo.rs`, replace the `let Some(inverse) = compute_inverse(...) else { ... }` block in `push_before_apply`:

```rust
        let Some(inverse) = compute_inverse(doc, action) else {
            // Actions whose inverse needs post-apply state push a placeholder
            // here; the caller fixes it up once the new id exists.
            match action {
                BlockAction::Split { .. } => {
                    let placeholder = BlockAction::MergeWithPrev {
                        block_id: BlockId::new(), // replaced by fix_split_inverse
                    };
                    self.redo.clear();
                    self.push_entry(UndoEntry {
                        action: action.clone(),
                        inverse: placeholder,
                    });
                }
                BlockAction::SplitListItem { block_id, .. } => {
                    let placeholder = BlockAction::MergeListItemWithPrev {
                        block_id: *block_id,
                        item_id: BlockId::new(), // replaced by fix_split_list_item_inverse
                    };
                    self.redo.clear();
                    self.push_entry(UndoEntry {
                        action: action.clone(),
                        inverse: placeholder,
                    });
                }
                // OpenSlashMenu and unrecordable actions: never recorded.
                _ => {}
            }
            return;
        };
```

- [ ] **Step 4: Add `fix_split_list_item_inverse`**

In `crates/lopress-editor/src/undo.rs`, add this method inside `impl UndoStack` (after `fix_split_inverse`):

```rust
    /// Replace the placeholder inverse for the most recent `SplitListItem`
    /// entry with `MergeListItemWithPrev` targeting the newly-created item.
    pub fn fix_split_list_item_inverse(&mut self, new_item_id: BlockId) {
        if let Some(entry) = self.undo.back_mut() {
            if matches!(entry.action, BlockAction::SplitListItem { .. }) {
                if let BlockAction::MergeListItemWithPrev { item_id, .. } =
                    &mut entry.inverse
                {
                    *item_id = new_item_id;
                }
            }
        }
    }
```

- [ ] **Step 5: Add `compute_inverse` arms for the list actions**

In `crates/lopress-editor/src/undo.rs`, in `compute_inverse`, add these arms before `BlockAction::OpenSlashMenu { .. } => None,`:

```rust
        BlockAction::EditListItem {
            block_id,
            item_id,
            ..
        } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let BlockBody::List(items) = &block.body else {
                return None;
            };
            let old_runs = items.iter().find(|it| it.id == *item_id)?.runs.clone();
            Some(BlockAction::EditListItem {
                block_id: *block_id,
                item_id: *item_id,
                new_runs: old_runs,
            })
        }
        BlockAction::SplitListItem { .. } => None, // post-state required
        BlockAction::MergeListItemWithPrev { block_id, item_id } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let BlockBody::List(items) = &block.body else {
                return None;
            };
            let pos = items.iter().position(|it| it.id == *item_id)?;
            let prev = items.get(pos.checked_sub(1)?)?;
            let split_offset: usize =
                prev.runs.iter().map(|r| r.text.len()).sum();
            Some(BlockAction::SplitListItem {
                block_id: *block_id,
                item_id: prev.id,
                byte_offset: split_offset,
            })
        }
```

- [ ] **Step 6: Run the undo tests — verify they pass**

```
cargo test -p lopress-editor --test undo_tests
```

Expected: all tests pass (the 7 originals plus the 3 new ones).

- [ ] **Step 7: Wire the `SplitListItem` inverse fix and focus into `ui/mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, add a helper near `focus_block_for` (a free function in the file). Add this `use` if `BlockBody` is not already imported, and the helper:

```rust
/// The id of the item immediately after `item_id` in `block_id`'s list.
fn list_item_after(doc: &EditorDoc, block_id: BlockId, item_id: BlockId) -> Option<BlockId> {
    let block = doc.blocks.iter().find(|b| b.id == block_id)?;
    let crate::model::types::BlockBody::List(items) = &block.body else {
        return None;
    };
    let pos = items.iter().position(|it| it.id == item_id)?;
    items.get(pos + 1).map(|it| it.id)
}
```

In the `on_action` closure, after the existing `if let BlockAction::Split { block_id, .. } = &action { ... }` block, add:

```rust
        // Fix SplitListItem inverse now that the new item id exists.
        if let BlockAction::SplitListItem { block_id, item_id, .. } = &action {
            let new_item_id = current_doc
                .with_untracked(|maybe| list_item_after(maybe.as_ref()?, *block_id, *item_id));
            if let Some(new_item_id) = new_item_id {
                undo_stack.update(|s| s.fix_split_list_item_inverse(new_item_id));
            }
        }
```

Then extend the `post_focus` match. Replace the `post_focus` binding's match body so it also focuses the new list item after a `SplitListItem`:

```rust
        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            (BlockAction::SplitListItem { block_id, item_id, .. }, Some(d)) => {
                list_item_after(d, *block_id, *item_id)
            }
            _ => None,
        });
```

- [ ] **Step 8: Add `focus_block_for` arms for the list actions**

In `crates/lopress-editor/src/ui/mod.rs`, `focus_block_for` maps an action to the block to focus after undo/redo. Add arms for the three new variants so undo/redo of a list edit focuses the list block:

```rust
        BlockAction::EditListItem { block_id, .. }
        | BlockAction::SplitListItem { block_id, .. }
        | BlockAction::MergeListItemWithPrev { block_id, .. } => Some(*block_id),
```

(Place this arm alongside the existing variant arms in `focus_block_for`'s `match`. If `focus_block_for` has a catch-all `_ => None`, this arm goes before it.)

- [ ] **Step 9: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors. If `BlockBody` / `EditorDoc` are unresolved in the helper, fully-qualify them as `crate::model::types::BlockBody` / `crate::model::types::EditorDoc` (the helper above already qualifies `BlockBody`; do the same for `EditorDoc` and `BlockId` if needed).

- [ ] **Step 10: Commit**

```
git add crates/lopress-editor/src/undo.rs crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/tests/undo_tests.rs
git commit -m "feat(undo): inverse coverage and focus wiring for list actions"
```

---

### Task 7: `plugin_block_view` — suppress chrome for builtin blocks

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs:46-92`

When `meta.builtin` is true, the header strip and attr form are suppressed — only the body editor renders. The body dispatch in `render_body` already handles `(BlockKind::List, BlockBody::List)`; Task 8 re-points it at the editable list view.

- [ ] **Step 1: Suppress header + form when `meta.builtin`**

In `crates/lopress-editor/src/ui/blocks/plugin.rs`, replace the body of `plugin_block_view` from `let header = label(...)` through the final `.into_any()` with:

```rust
    let body = render_body(
        block,
        on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
    );

    // Builtin (base-plugin) blocks suppress plugin chrome: no header strip,
    // no attr form — they render as plain editable blocks.
    if meta.builtin {
        return v_stack((body,))
            .style(|s| s.width_full())
            .into_any();
    }

    let header = label({
        let name = meta.block_type_name.clone();
        move || name.clone()
    })
    .style(|s| {
        s.padding_horiz(8.)
            .padding_vert(2.)
            .background(HEADER_BG)
            .color(HEADER_FG)
            .font_size(11.)
            .font_weight(Weight::SEMIBOLD)
            .border_radius(3.)
    });

    let attrs_sig: RwSignal<serde_json::Map<String, Value>> = RwSignal::new(meta.attrs.clone());
    let on_action_for_attrs = on_action.clone();
    let form = build_attr_form(&meta.attr_decls, attrs_sig, block_id, on_action_for_attrs);

    v_stack((header, form, body))
        .style(|s| {
            s.gap(4.)
                .padding(6.)
                .border(1.)
                .border_color(BORDER)
                .border_radius(4.)
                .background(FORM_BG)
                .width_full()
        })
        .into_any()
```

Note: `body` is now built before the `meta.builtin` branch, so `on_action` is cloned for `render_body` first and for `build_attr_form` second — keep the `on_action.clone()` calls as shown. The unused-variable lint will not fire because `on_action` is consumed by `build_attr_form` on the non-builtin path; on the builtin path it is moved into `render_body`'s clone and the original is dropped.

- [ ] **Step 2: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors. If the compiler warns that `HEADER_BG`/`HEADER_FG`/`FORM_BG`/`BORDER` are unused, they are still used on the non-builtin path — no warning expected.

- [ ] **Step 3: Manual test**

Run the app, open a document containing a markdown list. The list should render **without** the purple plugin header strip and without an attr form — just the list items (still read-only until Task 8).

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "feat(editor): suppress plugin chrome for builtin base-plugin blocks"
```

---

### Task 8: Editable list view — per-item native editors

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs` (rewrite)
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs:328-330` (re-point `render_body`'s List arm)

This is the largest task. It introduces `editable_list_view` — the canonical `editor = "list"` implementation — building one native `BlockEditorState` per list item, each with a list-specific key handler.

- [ ] **Step 1: Rewrite `list.rs` with the editable list view**

Replace the **entire contents** of `crates/lopress-editor/src/ui/blocks/list.rs` with:

```rust
//! Editable list rendering — the canonical `editor = "list"` implementation.
//!
//! Each `ListItem` gets its own native `BlockEditorState`. The view is a
//! `v_stack` of `[bullet/number] [item editor]` rows. Per-item keys handle
//! splitting, merging, and cross-item / cross-block navigation.

use crate::actions::BlockAction;
use crate::model::sync::rope_and_spans_to_runs;
use crate::model::types::{BlockId, EditorDoc, ListItem};
use crate::ui::blocks::inline_editor::{build_block_editor, ActionSink, FocusPublisher};
use crate::ui::blocks::paragraph::BODY_FONT_SIZE;
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::default_key_handler;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::view::editor_container_view;
use floem::views::editor::Editor;
use floem::views::{h_stack, stack, text, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::rc::Rc;

/// Build the editable list view for a list block.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
pub fn editable_list_view(
    items: &[ListItem],
    block_id: BlockId,
    ordered: bool,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> AnyView {
    let item_ids: Rc<Vec<BlockId>> = Rc::new(items.iter().map(|it| it.id).collect());
    let count = items.len();
    let rows: Vec<AnyView> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let prefix = if ordered {
                format!("{}.", idx + 1)
            } else {
                "•".to_string()
            };
            let editor = list_item_editor(
                &item.runs,
                block_id,
                item.id,
                idx,
                count,
                Rc::clone(&item_ids),
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
            );
            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                editor.style(|s| s.flex_grow(1.0)),
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
        })
        .collect();
    v_stack_from_iter(rows)
        .style(|s| s.padding_vert(4.).padding_left(8.).width_full())
        .into_any()
}

/// One list item's native editor.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn list_item_editor(
    runs: &[ListItem],
    block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> AnyView {
    // `runs` is typed `&[ListItem]` only to satisfy the closure capture; the
    // caller passes `&item.runs`, a `&[InlineRun]`. See the build call below.
    let _ = (item_count, &item_ids);
    unreachable!("placeholder — replaced in Step 2")
}
```

Note: the `list_item_editor` body above is an intentional stub so Step 1 is reviewable on its own; Step 2 fills it in. Do not run a build between Step 1 and Step 2.

- [ ] **Step 2: Implement `list_item_editor` and the key handler**

Replace the stub `list_item_editor` function (and append the key handler) so the bottom of `list.rs` reads:

```rust
/// One list item's native editor: a `BlockEditorState` plus a list-specific
/// key handler for splitting, merging, and navigation.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn list_item_editor(
    runs: &[crate::model::types::InlineRun],
    block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> AnyView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, BODY_FONT_SIZE as usize);
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let style_rev = state.style_rev;
    let text_sig = state.text_sig;
    let link_url_sig = state.link_url_sig;

    let default_kp_handler = default_key_handler(editor_sig);
    let on_action_for_key = on_action;

    let view = editor_container_view(
        editor_sig,
        move |_| editor_sig.with_untracked(|ed| ed.active.get()),
        move |kp, ms| {
            let result = handle_list_item_key(
                kp,
                ms,
                editor_sig,
                spans_sig,
                block_id,
                item_id,
                item_index,
                item_count,
                &item_ids,
                &on_action_for_key,
                focus_target,
                current_doc,
            );
            if result == CommandExecuted::Yes {
                result
            } else {
                default_kp_handler(kp, ms)
            }
        },
    );

    // Publish focus: the list *block* (not the item) owns the toolbar slot,
    // so report `block_id` while exposing this item's editor handles.
    create_effect(move |_| {
        let is_active = editor_sig.with(|ed| ed.active.get());
        if is_active {
            focus_pub.block.set(Some(block_id));
            focus_pub
                .editor_and_spans
                .set(Some((editor_sig, spans_sig, style_rev, link_url_sig)));
        }
    });

    // Programmatic focus when `focus_target` names this item.
    create_effect(move |_| {
        if focus_target.get() == Some(item_id) {
            editor_sig.with_untracked(|ed| {
                if let Some(view_id) = ed.editor_view_id.get_untracked() {
                    view_id.request_focus();
                    view_id.scroll_to(None);
                }
            });
            focus_target.set(None);
        }
    });

    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    stack((view,))
        .style(move |s| {
            let lines = text_sig.get().split('\n').count().max(1) as f32;
            s.class(GutterClass, |s| s.hide())
                .width_full()
                .height(lines * line_height)
        })
        .into_any()
}

/// Write the item's current editor text back to the document.
fn commit_list_item(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<crate::model::style_span::StyleSpan>>,
    block_id: BlockId,
    item_id: BlockId,
    on_action: &ActionSink,
) {
    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditListItem {
        block_id,
        item_id,
        new_runs,
    });
}

/// List-item key handling: Enter splits, Backspace-at-0 merges, ↑/↓ navigate.
#[allow(clippy::too_many_arguments)]
fn handle_list_item_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<crate::model::style_span::StyleSpan>>,
    block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: &[BlockId],
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> CommandExecuted {
    use floem::keyboard::{Key, NamedKey};

    let shift = ms.shift();
    let ctrl_or_cmd = ms.control() || ms.meta();
    if ctrl_or_cmd {
        // Ctrl/Cmd shortcuts are not handled at the item level; let the
        // default editor handler deal with them.
        return CommandExecuted::No;
    }

    match &kp.key {
        // Shift+Enter — soft line break within the item.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => {
            editor_sig.with_untracked(|ed| {
                ed.doc().receive_char(ed, "\n");
            });
            CommandExecuted::Yes
        }

        // Enter — commit, then split this item at the cursor.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) => {
            let byte_offset =
                editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            on_action(BlockAction::SplitListItem {
                block_id,
                item_id,
                byte_offset,
            });
            CommandExecuted::Yes
        }

        // Backspace at offset 0 — merge with the previous item, or with the
        // block before the list when this is the first item.
        KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
            let offset =
                editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            if offset != 0 {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index > 0 {
                on_action(BlockAction::MergeListItemWithPrev { block_id, item_id });
            } else {
                on_action(BlockAction::MergeWithPrev { block_id });
            }
            CommandExecuted::Yes
        }

        // ↑ on the first visual line — move to the previous item, or to the
        // block before the list.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
            let on_first = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
            });
            if !on_first {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index > 0 {
                if let Some(prev) = item_ids.get(item_index - 1) {
                    focus_target.set(Some(*prev));
                }
            } else {
                let prev_block = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
                });
                if let Some(id) = prev_block {
                    focus_target.set(Some(id));
                }
            }
            CommandExecuted::Yes
        }

        // ↓ on the last visual line — move to the next item, or to the block
        // after the list.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
            let on_last = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                vline.0 == ed.last_vline().0
            });
            if !on_last {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index + 1 < item_count {
                if let Some(next) = item_ids.get(item_index + 1) {
                    focus_target.set(Some(*next));
                }
            } else {
                let next_block = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    d.blocks.get(i + 1).map(|b| b.id)
                });
                if let Some(id) = next_block {
                    focus_target.set(Some(id));
                }
            }
            CommandExecuted::Yes
        }

        _ => CommandExecuted::No,
    }
}
```

Also delete the `runs: &[ListItem]` placeholder stub and its `unreachable!` body — the function above replaces it entirely. Remove the now-unused `ListItem` from the `use crate::model::types::{...}` import if it is no longer referenced (it is still referenced by `editable_list_view`'s `items: &[ListItem]` parameter — keep it).

- [ ] **Step 3: Re-point `plugin_block_view`'s `render_body` List arm**

In `crates/lopress-editor/src/ui/blocks/plugin.rs`, in `render_body`, replace the List arm:

```rust
        (BlockKind::List { ordered }, BlockBody::List(items)) => list::editable_list_view(
            items,
            block_id,
            *ordered,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
        ),
```

(`block_id`, `on_action`, `focus_target`, `focus_pub`, `current_doc` are all already in scope in `render_body`. The arm previously read `list::render_list(*ordered, items).into_any()`; `editable_list_view` already returns `AnyView`, so drop the `.into_any()`.)

- [ ] **Step 4: Compile**

```
cargo check -p lopress-editor 2>&1 | tail -30
```

Expected: no errors. Likely fixes needed:
- `render_list` is no longer defined — `code.rs`/`opaque.rs`/`block_view` may still reference it. Grep: `cargo check` will name any remaining caller. The only other caller is `block_view`'s built-in List arm, fixed in Task 9; until then it will error. **If `block_view` fails to compile here**, temporarily change its List arm to `(BlockKind::List { .. }, BlockBody::List(_)) => empty().into_any(),` and note it — Task 9 Step 1 replaces it properly.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "feat(editor): add editable list view with per-item native editors"
```

---

### Task 9: Route `block_view` to the editable list view + full verification

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs:112-114`

- [ ] **Step 1: Re-point `block_view`'s built-in List arm**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, replace the `(BlockKind::List { ordered }, BlockBody::List(items))` arm of the `match (&block.kind, &block.body)` in `block_view`:

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

This arm now runs only for lists with `plugin: None` — i.e. lists created at runtime via the `UL`/`OL` toolbar buttons (`ChangeType`). Plugin-flagged lists loaded from markdown go through the `block.plugin.is_some()` branch above. Both paths reach the same `editable_list_view`.

- [ ] **Step 2: Compile the whole workspace**

```
cargo check --workspace --all-targets 2>&1 | tail -30
```

Expected: no errors.

- [ ] **Step 3: Run the full test suite**

```
cargo test --workspace 2>&1 | tail -40
```

Expected: all tests pass. Fix any remaining fallout (most likely in `plugin_block_tests.rs` or `from_to_core_tests.rs` if a test constructs `PluginMeta` literally — add `builtin: false` to any such literal).

- [ ] **Step 4: Lint and format**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -30
```

Expected: clean. Fix any clippy findings.

- [ ] **Step 5: Manual test — list editing**

Run the app and open a document containing a markdown list:

1. **Edit text** — click into a list item, type; the edit persists (reopen the file after the 500 ms save debounce to confirm).
2. **Split** — press Enter mid-item; the item splits into two, focus lands in the new item.
3. **Merge into previous item** — at offset 0 of item N>0, press Backspace; the item merges into its predecessor.
4. **Merge into block before the list** — at offset 0 of item 0, press Backspace; item 0 merges into the block above the list.
5. **Navigate** — ↑/↓ move between items; ↑ from item 0 jumps to the block above; ↓ from the last item jumps to the block below.
6. **Undo/redo** — Ctrl+Z reverts a split (items re-merge) and a merge (item re-splits); Ctrl+Y / Ctrl+Shift+Z re-applies.
7. **Ordered vs. unordered** — both `-` and `1.` lists render with correct prefixes and are editable.
8. **No plugin chrome** — the list shows no purple header strip and no attr form.
9. **ChangeType** — focus a paragraph, click `UL` in the toolbar; the block becomes an editable list.
10. **Ctrl API** — POST a `Split` action targeting a list block to `http://127.0.0.1:7878/action`; the targeted item splits.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "feat(editor): route built-in list dispatch to the editable list view"
```

- [ ] **Step 7: Update the spec status**

In `docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md`, change the header line `**Status:** approved, deferred` to `**Status:** implemented`. Commit:

```
git add docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md
git commit -m "docs: mark editable-list base-plugin spec as implemented"
```

---

## Self-Review

**Spec coverage:**

| Spec section | Covered by |
|---|---|
| §1 Base plugin infrastructure (`from_str`, `load_base_plugins`) | Tasks 1, 2 |
| §2 List base plugin manifest | Task 2 |
| §3 Model wiring (`PluginMeta`, `from_core`/`to_core`) | Task 4 |
| §4 Plugin block view changes (`builtin` chrome suppression) | Task 7 |
| §5 New `BlockAction` variants | Task 5 |
| §6 Fix `apply_split` for lists | Task 5 Step 6 |
| §7 Editable list view | Tasks 8, 9 |
| §8 (impl order) startup wiring | Task 3 |
| Undo coverage for new actions | Task 6 |

**Deviations from the spec, with rationale** — documented in the "Reconciliation notes" header: real manifest format, optional `template`, list serialization passthrough, retaining (not removing) `block_view`'s List arm, and the added `EditListItem` action.

**Type consistency:** `PluginMeta` gains `builtin: bool` (Task 4) — every constructor (`from_core.rs` ×2, plus any test literal) sets it. `BlockDecl` gains `builtin: bool` and `template: Option<String>` (Task 1). The three new `BlockAction` variants use consistent field names (`block_id`, `item_id`, `byte_offset`, `new_runs`) across `actions.rs`, `undo.rs`, `ui/mod.rs`, and `list.rs`. `editable_list_view` returns `AnyView`; both call sites (`plugin.rs`, `mod.rs`) drop the old `.into_any()`.

**Placeholder scan:** The only intentional stub is `list_item_editor` between Task 8 Step 1 and Step 2 — explicitly flagged, with an instruction not to build between those steps. No `TBD`/`TODO` remain.

**Implementer fit:** Task 8 is the largest (full rewrite of `list.rs`). It is split into Step 1 (scaffold + `editable_list_view`) and Step 2 (`list_item_editor` + key handler) so each step is a single coherent transcription. All other tasks touch 1–3 files with complete code provided.

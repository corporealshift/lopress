# Everything Is a Plugin — Stage A (migrate paragraph & heading) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the last two non-plugin block types (paragraph, heading) onto the plugin path so every block carries `PluginMeta` and routes through `editor_for`, deleting the hardcoded paragraph/heading `(BlockKind, BlockBody)` dispatch arms in `block_view`, `render_body`, `from_core`, and `to_core`.

**Architecture:** Mirror the existing `list`/`code` base-plugin migration exactly: add `base_plugins/paragraph` + `base_plugins/heading` manifests (native-claiming, builtin), embed them in `load_base_plugins`, stamp `PluginMeta` on paragraph/heading blocks in the constructors and `from_core`, register `editor_for` arms, route serialization through the native `from_core`/`to_core` paths, and delete the now-dead hardcoded arms. `BlockKind` is KEPT (it stays as the body-shape signal; heading level is mirrored into `attrs["level"]` exactly like `code` mirrors `lang`).

**Tech Stack:** Rust workspace (`lopress-plugin` manifests/registry, `lopress-editor` model + UI), the existing plugin-capability machinery (editor registry, native registry, registry-driven from_core/to_core).

---

## Task 1: Base-plugin manifests for `paragraph` and `heading`

**Files:**
- Create: `base_plugins/paragraph/manifest.toml`
- Create: `base_plugins/heading/manifest.toml`

- [ ] **Step 1: Create `base_plugins/paragraph/manifest.toml`:**

```toml
# Built-in "base" plugin: the paragraph block, claiming the native core
# `paragraph` type. Embedded at compile time via include_str! — see
# load_base_plugins.
name    = "lopress-paragraph"
version = "0.1.0"

[[blocks]]
name    = "paragraph"
editor  = "paragraph"
native  = "paragraph"
builtin = true
```

- [ ] **Step 2: Create `base_plugins/heading/manifest.toml`:**

```toml
# Built-in "base" plugin: the heading block, claiming the native core
# `heading` type. Embedded at compile time via include_str! — see
# load_base_plugins.
name    = "lopress-heading"
version = "0.1.0"

[[blocks]]
name    = "heading"
editor  = "heading"
native  = "heading"
builtin = true

[blocks.attrs]
level = { type = "number", ui = "hidden" }
```

---

## Task 2: Embed manifests in `load_base_plugins` + unit tests

**Files:**
- Modify: `crates/lopress-plugin/src/registry.rs` (the `BASE_MANIFESTS` array in `load_base_plugins`, ~line 70; and add tests in the `mod tests` block)

- [ ] **Step 1: Write the failing tests** — append to the `mod tests` block in `crates/lopress-plugin/src/registry.rs`:

```rust
    #[test]
    fn base_plugins_include_paragraph() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_p, decl) = reg
            .native_block("paragraph")
            .expect("paragraph native block");
        assert_eq!(decl.editor.as_deref(), Some("paragraph"));
        assert_eq!(decl.native.as_deref(), Some("paragraph"));
        assert!(decl.builtin);
        assert!(decl.attrs.is_empty());
    }

    #[test]
    fn base_plugins_include_heading() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_p, decl) = reg
            .native_block("heading")
            .expect("heading native block");
        assert_eq!(decl.editor.as_deref(), Some("heading"));
        assert_eq!(decl.native.as_deref(), Some("heading"));
        assert!(decl.builtin);
        assert!(decl.attrs.contains_key("level"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-plugin base_plugins_include_paragraph base_plugins_include_heading`
Expected: FAIL (`paragraph native block` / `heading native block` — not registered yet).

- [ ] **Step 3: Register both in `load_base_plugins`** — extend the `BASE_MANIFESTS` array (currently list/code/more/image/separator/table):

```rust
        const BASE_MANIFESTS: &[&str] = &[
            include_str!("../../../base_plugins/list/manifest.toml"),
            include_str!("../../../base_plugins/code/manifest.toml"),
            include_str!("../../../base_plugins/more/manifest.toml"),
            include_str!("../../../base_plugins/image/manifest.toml"),
            include_str!("../../../base_plugins/separator/manifest.toml"),
            include_str!("../../../base_plugins/table/manifest.toml"),
            include_str!("../../../base_plugins/paragraph/manifest.toml"),
            include_str!("../../../base_plugins/heading/manifest.toml"),
        ];
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-plugin base_plugins_include_paragraph base_plugins_include_heading`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add base_plugins/paragraph/manifest.toml base_plugins/heading/manifest.toml crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): register paragraph and heading base plugins"
```

---

## Task 3: `PluginMeta` constructors + `EditorBlock` constructor updates

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs` (add `PluginMeta::paragraph()` and `PluginMeta::heading(level: u8)`; update `EditorBlock::paragraph` and `EditorBlock::heading` to stamp `plugin: Some(...)`)

- [ ] **Step 1: Write the failing tests** — append to the `mod tests` block in `types.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod paragraph_heading_meta_tests {
    use super::*;

    #[test]
    fn paragraph_block_carries_plugin_meta() {
        let b = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
        let meta = b.plugin.as_ref().expect("paragraph must carry PluginMeta");
        assert_eq!(&*meta.block_type_name, "paragraph");
        assert_eq!(meta.editor.as_deref(), Some("paragraph"));
        assert_eq!(meta.native.as_deref(), Some("paragraph"));
        assert!(meta.builtin);
        assert!(meta.attrs.is_empty());
    }

    #[test]
    fn heading_block_carries_plugin_meta_with_level() {
        let b = EditorBlock::heading(3, vec![InlineRun::plain("title")]);
        let meta = b.plugin.as_ref().expect("heading must carry PluginMeta");
        assert_eq!(&*meta.block_type_name, "heading");
        assert_eq!(meta.editor.as_deref(), Some("heading"));
        assert_eq!(meta.native.as_deref(), Some("heading"));
        assert!(meta.builtin);
        assert_eq!(
            meta.attrs.get("level").and_then(Value::as_u64),
            Some(3)
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor paragraph_block_carries_plugin_meta heading_block_carries_plugin_meta`
Expected: FAIL (`paragraph must carry PluginMeta` — `b.plugin` is `None`).

- [ ] **Step 3: Add `PluginMeta::paragraph()`** (next to `PluginMeta::list()` in the `impl PluginMeta` block):

```rust
    /// The canonical `PluginMeta` for a built-in paragraph block.
    ///
    /// Mirrors what `from_core` stamps for a `paragraph` core block, so a
    /// paragraph created inside the editor (e.g. via `ChangeType` from the
    /// toolbar or slash menu) carries the same plugin identity as one loaded
    /// from disk — taking the plugin render path and native serialization.
    /// `attr_decls` is empty: the paragraph is `builtin`, so the attr form
    /// is suppressed.
    pub fn paragraph() -> Self {
        Self {
            block_type_name: Rc::from("paragraph"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("paragraph")),
            native: Some(Rc::from("paragraph")),
        }
    }

    /// The canonical `PluginMeta` for a built-in heading block.
    ///
    /// Mirrors what `from_core` stamps for a `heading` core block, so a
    /// heading created inside the editor (e.g. via `ChangeType` from the
    /// toolbar or slash menu) carries the same plugin identity as one loaded
    /// from disk. `attrs["level"]` mirrors `BlockKind::Heading(level)` so
    /// the heading widget can read the level from attrs (not from the enum).
    /// `attr_decls` is empty: the heading is `builtin`, so the attr form
    /// is suppressed.
    pub fn heading(level: u8) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("level".to_string(), Value::Number(serde_json::Number::from(level)));
        Self {
            block_type_name: Rc::from("heading"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("heading")),
            native: Some(Rc::from("heading")),
        }
    }
```

- [ ] **Step 4: Update `EditorBlock::paragraph`** to stamp `plugin: Some(PluginMeta::paragraph())`:

**Before:**
```rust
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }
```

**After:**
```rust
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(runs),
            plugin: Some(PluginMeta::paragraph()),
        }
    }
```

- [ ] **Step 5: Update `EditorBlock::heading`** to stamp `plugin: Some(PluginMeta::heading(level))`:

**Before:**
```rust
    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Heading(level.clamp(1, 6)),
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }
```

**After:**
```rust
    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        let level = level.clamp(1, 6);
        Self {
            id: BlockId::new(),
            kind: BlockKind::Heading(level),
            body: BlockBody::Inline(runs),
            plugin: Some(PluginMeta::heading(level)),
        }
    }
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p lopress-editor paragraph_block_carries_plugin_meta heading_block_carries_plugin_meta`
Expected: PASS (both).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs
git commit -m "feat(editor): stamp PluginMeta on paragraph and heading constructors"
```

---

## Task 4: `from_core` — delete hardcoded paragraph/heading arms, add native arms

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs` (delete the hardcoded `"paragraph"`/`"heading"` arms in `block_from_core`; add `Some("paragraph")`/`Some("heading")` arms to `native_block_from_core`; add `native_paragraph_from_core` and `native_heading_from_core` helpers)

- [ ] **Step 1: Write the failing test** — append to the existing tests in `from_to_core_tests.rs` (or add a new test in `from_core.rs` `mod tests` if one exists):

```rust
#[test]
fn paragraph_round_trips_via_native_path() {
    // After migration, paragraph blocks must route through the native
    // registry path — not the hardcoded arm — proving the migration works.
    let src = "A plain paragraph.\n\nAnother one.\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    // Sanity: the editor classifies it correctly.
    for b in &editor.blocks {
        assert!(
            b.plugin.is_some(),
            "loaded paragraph must carry PluginMeta"
        );
        let meta = b.plugin.as_ref().unwrap();
        assert_eq!(meta.block_type_name.as_ref(), "paragraph");
        assert_eq!(meta.native.as_deref(), Some("paragraph"));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn heading_round_trips_via_native_path() {
    // After migration, heading blocks must route through the native registry
    // path — not the hardcoded arm.
    let src = "# h1\n\n## h2\n\n### h3\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    for b in &editor.blocks {
        assert!(
            b.plugin.is_some(),
            "loaded heading must carry PluginMeta"
        );
        let meta = b.plugin.as_ref().unwrap();
        assert_eq!(meta.block_type_name.as_ref(), "heading");
        assert_eq!(meta.native.as_deref(), Some("heading"));
        assert!(meta.attrs.contains_key("level"));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor paragraph_round_trips_via_native_path heading_round_trips_via_native_path`
Expected: FAIL (heading levels may not match — the hardcoded arm doesn't stamp `PluginMeta`, so `to_core` goes through the `(BlockKind::Heading,…)` arm instead of the native path).

- [ ] **Step 3: Delete the hardcoded `"paragraph"` and `"heading"` arms** in `block_from_core` — replace the two explicit arms and the `other` catch-all with a single lookup:

**Before:**
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

**After:**
```rust
fn block_from_core(b: &Block, registry: &PluginRegistry) -> EditorBlock {
    match registry.native_block(b.r#type.as_str()) {
        Some((_plugin, decl)) => native_block_from_core(b, decl),
        None => match registry.block(b.r#type.as_str()) {
            Some((_plugin, decl)) => plugin_block_from_core(b, decl),
            None => EditorBlock::opaque(
                b.r#type.clone(),
                serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
            ),
        },
    }
}
```

- [ ] **Step 4: Add `Some("paragraph")` / `Some("heading")` arms to `native_block_from_core`** and the helper functions:

Update the `native_block_from_core` match:

```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        Some("code") => native_code_from_core(b, decl),
        Some("paragraph") => native_paragraph_from_core(b, decl),
        Some("heading") => native_heading_from_core(b, decl),
        Some("image") => native_image_from_core(b, decl),
        Some("separator") => EditorBlock::separator(),
        Some("table") => native_table_from_core(b),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

Add the two helper functions (place them after `native_code_from_core`, before `native_image_from_core`):

```rust
/// Native-paragraph body parser. Reads inline text from `b.text`, parses
/// it into `InlineRun`s, and stamps `PluginMeta` so the block routes
/// through the plugin view and serializes back via `to_core`'s native
/// branch.
fn native_paragraph_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let text = b.text.as_deref().unwrap_or("");
    let mut block = EditorBlock::paragraph(parse_inline(text));
    block.plugin = Some(PluginMeta {
        block_type_name: Rc::from(decl.name.as_str()),
        attrs: serde_json::Map::new(),
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    });
    block
}

/// Native-heading body parser. Reads `level` from `b.attrs["level"]`,
/// parses inline text from `b.text`, stamps `PluginMeta` with `attrs["level"]`
/// mirrored (so the heading widget reads level from attrs), and stamps
/// `PluginMeta` so the block routes through the plugin view.
fn native_heading_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let level = b
        .attrs
        .get("level")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1)
        .clamp(1, 6);
    let text = b.text.as_deref().unwrap_or("");

    let mut block = EditorBlock::heading(level, parse_inline(text));
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "level".to_string(),
        serde_json::Value::Number(serde_json::Number::from(level)),
    );
    block.plugin = Some(PluginMeta {
        block_type_name: Rc::from(decl.name.as_str()),
        attrs,
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    });
    block
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-editor paragraph_round_trips_via_native_path heading_round_trips_via_native_path`
Expected: PASS (both). Also run `cargo test -p lopress-editor` to confirm no regression in existing `from_to_core_tests`.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/from_core.rs
git commit -m "refactor(editor): route paragraph/heading through native from_core path"
```

---

## Task 5: `to_core` — add native paragraph/heading arms, delete hardcoded arms

**Files:**
- Modify: `crates/lopress-editor/src/model/to_core.rs` (add `paragraph`/`heading` body cases in `native_block_to_core`; delete the `(BlockKind::Paragraph,…)`/`(BlockKind::Heading,…)` hardcoded arms in `block_to_core`)

- [ ] **Step 1: Write the failing test** — append to the existing tests in `from_to_core_tests.rs`:

```rust
#[test]
fn paragraph_to_core_serializes_via_native_path() {
    // A paragraph block with PluginMeta must serialize through
    // native_block_to_core (not the hardcoded arm).
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let src = "A paragraph.\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);

    let block = &editor.blocks[0];
    assert!(block.plugin.is_some());
    let meta = block.plugin.as_ref().unwrap();
    assert_eq!(meta.native.as_deref(), Some("paragraph"));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.blocks[0].r#type, "paragraph");
    assert_eq!(core_back.blocks[0].text.as_deref(), Some("A paragraph.\n"));
}

#[test]
fn heading_to_core_serializes_via_native_path_with_level() {
    // A heading block with PluginMeta must serialize through native_block_to_core,
    // carrying the level in attrs.
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let src = "## h2\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);

    let block = &editor.blocks[0];
    assert!(block.plugin.is_some());
    let meta = block.plugin.as_ref().unwrap();
    assert_eq!(meta.native.as_deref(), Some("heading"));
    assert_eq!(meta.attrs.get("level").and_then(Value::as_u64), Some(2));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.blocks[0].r#type, "heading");
    assert_eq!(core_back.blocks[0].attrs, json!({ "level": 2 }));
    assert_eq!(core_back.blocks[0].text.as_deref(), Some("h2\n"));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor paragraph_to_core_serializes_via_native_path heading_to_core_serializes_via_native_path_with_level`
Expected: FAIL (the hardcoded `(BlockKind::Heading,…)` arm in `block_to_core` still fires — check `core_back.blocks[0].attrs` for `"level"` presence).

- [ ] **Step 3: Add `paragraph` and `heading` body cases to `native_block_to_core`** (place them before the `_` fallback arm):

```rust
        BlockBody::Inline(runs) if core_type == "paragraph" => Block {
            r#type: core_type.to_string(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        BlockBody::Inline(runs) if core_type == "heading" => {
            let level = meta
                .attrs
                .get("level")
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "level": level }),
                children: vec![],
                text: Some(serialize_inline(runs)),
            }
        }
```

The complete `native_block_to_core` match after the edit:

```rust
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => {
            // ... existing list arm ...
        }
        BlockBody::Code(text) => {
            // ... existing code arm ...
        }
        BlockBody::Table(data) => {
            // ... existing table arm ...
        }
        BlockBody::Inline(runs) if core_type == "paragraph" => Block {
            r#type: core_type.to_string(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        BlockBody::Inline(runs) if core_type == "heading" => {
            let level = meta
                .attrs
                .get("level")
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "level": level }),
                children: vec![],
                text: Some(serialize_inline(runs)),
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

- [ ] **Step 4: Delete the hardcoded `(BlockKind::Paragraph,…)` and `(BlockKind::Heading,…)` arms** from `block_to_core`. After this migration, every paragraph/heading block carries `PluginMeta` with `native` set, so they always take the plugin path (the `if let Some(meta) = &b.plugin` branch at the top of `block_to_core`). The hardcoded arms are now dead code.

**Before:**
```rust
fn block_to_core(b: &EditorBlock) -> Block {
    // Plugin-flagged blocks: a `native` claim serializes as bare native
    // markdown of that core type; otherwise the comment container is used.
    if let Some(meta) = &b.plugin {
        // ... read-more special case ...
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
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            // ... existing opaque arm ...
        }
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    }
}
```

**After:**
```rust
fn block_to_core(b: &EditorBlock) -> Block {
    // Plugin-flagged blocks: a `native` claim serializes as bare native
    // markdown of that core type; otherwise the comment container is used.
    if let Some(meta) = &b.plugin {
        // ... read-more special case ...
        return match &meta.native {
            Some(core_type) => native_block_to_core(b, meta, core_type),
            None => plugin_block_to_core(b, meta),
        };
    }
    match (&b.kind, &b.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.to_string(),
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
```

- [ ] **Step 5: Run to verify it compiles and passes**

Run: `cargo test -p lopress-editor paragraph_to_core_serializes_via_native_path heading_to_core_serializes_via_native_path_with_level`
Expected: PASS (both). Also run `cargo test -p lopress-editor` to confirm no regression.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/to_core.rs
git commit -m "refactor(editor): route paragraph/heading serialization through native to_core path"
```

---

## Task 6: `editor_for` — add paragraph/heading arms + widget adapters

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` (add `"paragraph"` and `"heading"` arms to `editor_for`; add `paragraph_editor_widget` and `heading_editor_widget` adapter functions; update the existing test that asserts `editor_for("paragraph").is_none()`)

- [ ] **Step 1: Write the failing test** — update the existing test in `editor_registry.rs` `mod tests`:

```rust
    #[test]
    fn editor_for_resolves_paragraph_and_heading() {
        assert!(editor_for("paragraph").is_some());
        assert!(editor_for("heading").is_some());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-editor editor_for_resolves_paragraph_and_heading`
Expected: FAIL (`paragraph` and `heading` not yet registered).

- [ ] **Step 3: Add `"paragraph"` and `"heading"` arms to `editor_for`** and the two adapter functions:

Update the `editor_for` match:

```rust
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        "code" => Some(code_editor_widget),
        "paragraph" => Some(paragraph_editor_widget),
        "heading" => Some(heading_editor_widget),
        "more" => Some(read_more::read_more_widget),
        "separator" => Some(separator::separator_widget),
        "image" => Some(image::image_widget),
        "table" => Some(table::table_editor_widget),
        _ => None,
    }
}
```

Add the two adapter functions (place them after `code_editor_widget`, mirroring the pattern):

```rust
/// The `editor = "paragraph"` widget. Extracts runs from the block's
/// `BlockBody::Inline` and calls `paragraph::render_paragraph_editable`.
fn paragraph_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Inline(runs) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry paragraph: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    paragraph::render_paragraph_editable(runs, block.id, env).into_any()
}

/// The `editor = "heading"` widget. Extracts runs from the block's
/// `BlockBody::Inline` and reads `level` from `PluginMeta.attrs["level"]`
/// (mirrored from `BlockKind::Heading(level)`), then calls
/// `heading::render_heading_editable`.
fn heading_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Inline(runs) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry heading: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    let level = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("level"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1);
    heading::render_heading_editable(level, runs, block.id, env).into_any()
}
```

- [ ] **Step 4: Update the existing test** — remove the `assert!(editor_for("paragraph").is_none())` from the `editor_for_resolves_list_and_rejects_unknown` test, and change it to `.is_some()`:

```rust
    #[test]
    fn editor_for_resolves_list_and_rejects_unknown() {
        assert!(editor_for("list").is_some());
        assert!(editor_for("code").is_some());
        assert!(editor_for("more").is_some());
        assert!(editor_for("paragraph").is_some());
        assert!(editor_for("bogus").is_none());
    }
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-editor editor_for_resolves_paragraph_and_heading editor_for_resolves_list_and_rejects_unknown`
Expected: PASS (both).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "feat(editor): register paragraph and heading in editor_for"
```

---

## Task 7: Delete dead hardcoded arms in `render_body` and `block_view`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs` (the `render_body` function — delete the `(BlockKind::Paragraph,…)` and `(BlockKind::Heading,…)` fallback arms)
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (the `block_view` function — delete the `(BlockKind::Paragraph,…)` and `(BlockKind::Heading,…)` fallback arms)

- [ ] **Step 1: Delete the hardcoded `(BlockKind::Paragraph,…)` and `(BlockKind::Heading,…)` arms from `render_body`** in `plugin.rs`. After this migration, paragraph/heading blocks always carry `PluginMeta` with `editor` set, so they take the registry path. The fallback arms are now dead code.

**Before:**
```rust
fn render_body(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    use crate::ui::blocks::editor_registry::editor_for;

    // Registry path: a manifest `editor` key with a registered widget wins.
    if let Some(key) = block.plugin.as_ref().and_then(|m| m.editor.as_deref()) {
        if let Some(widget) = editor_for(key) {
            return widget(block, env);
        }
    }

    // Fallback: editor keys not yet migrated to the registry (paragraph,
    // heading, code) still dispatch on the Rust `BlockKind` enum.
    let block_id = block.id;
    match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            paragraph::render_paragraph_editable(runs, block_id, env).into_any()
        }
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            heading::render_heading_editable(*level, runs, block_id, env).into_any()
        }
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
            list::editable_list_view(items, block_id, *ordered, env)
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] plugin block {:?}: kind/body mismatch ({:?} + {:?})",
                block.id, block.kind, block.body
            );
            crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    }
}
```

**After:**
```rust
fn render_body(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    use crate::ui::blocks::editor_registry::editor_for;

    // Registry path: a manifest `editor` key with a registered widget wins.
    if let Some(key) = block.plugin.as_ref().and_then(|m| m.editor.as_deref()) {
        if let Some(widget) = editor_for(key) {
            return widget(block, env);
        }
    }

    // Fallback: editor keys not yet migrated to the registry (code) still
    // dispatch on the Rust `BlockKind` enum.
    let block_id = block.id;
    match (&block.kind, &block.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
            list::editable_list_view(items, block_id, *ordered, env)
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] plugin block {:?}: kind/body mismatch ({:?} + {:?})",
                block.id, block.kind, block.body
            );
            crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    }
}
```

- [ ] **Step 2: Delete the hardcoded `(BlockKind::Paragraph,…)` and `(BlockKind::Heading,…)` arms from `block_view`** in `mod.rs`. After this migration, paragraph/heading blocks always carry `PluginMeta`, so they always take the plugin path (`if block.plugin.is_some()`).

**Before:**
```rust
    let body = match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            paragraph::render_paragraph_editable(runs, block.id, env).into_any()
        }
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            heading::render_heading_editable(*level, runs, block.id, env).into_any()
        }
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env)
        }
        (BlockKind::Opaque { .. }, BlockBody::Opaque(_)) => {
            // ...
        }
        _ => {
            // ...
        }
    };
```

**After:**
```rust
    let body = match (&block.kind, &block.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env)
        }
        (BlockKind::Opaque { .. }, BlockBody::Opaque(_)) => {
            // Opaque blocks load from disk with unknown/removed plugin types.
            // Route through the fallback so they're visible and recoverable,
            // not a silent drop or a read-only card with no toolbar.
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
        // Body/kind mismatch — render fallback so content is visible and recoverable.
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] block {:?}: kind/body mismatch ({:?} + {:?})",
                block_id, block.kind, block.body
            );
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    };
```

- [ ] **Step 3: Run to verify it compiles and passes**

Run: `cargo test -p lopress-editor`
Expected: PASS (all existing tests still pass — paragraph/heading now flow through the plugin path).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "refactor(editor): delete dead paragraph/heading hardcoded dispatch arms"
```

---

## Task 8: Round-trip verification — all tests green

**Files:**
- Run: `cargo test --workspace` (full round-trip suite)
- Run: `bash scripts/check.sh` (full gate)

- [ ] **Step 1: Run the full round-trip suite**

Run: `cargo test -p lopress-editor from_to_core`
Expected: PASS (all round-trip tests, including the new paragraph/heading native-path tests).

- [ ] **Step 2: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS (formatting, clippy, and tests all green).

- [ ] **Step 3: Commit any fmt-only changes** — Task 8 changes no logic, but `cargo fmt
--all` inside `check.sh` may have reformatted files touched in Tasks 1–7. If
`git status --short` shows anything, stage it **by name** — NEVER `git add -A` (the tree
has `.claude/settings.local.json` and gitignored `.pi-delegations/` that must not be
swept):

```bash
git status --short
# Only if there are fmt changes to source files, stage those paths by name:
git add base_plugins crates/lopress-plugin/src crates/lopress-editor/src crates/lopress-editor/tests
git commit -m "chore: fmt after Stage A round-trip verification"
```
If `git status --short` shows nothing (besides the unrelated `.claude/settings.local.json`),
skip this commit entirely.

---

## Summary of tasks

| # | Task | Files |
|---|------|-------|
| 1 | Base-plugin manifests for `paragraph` and `heading` | `base_plugins/paragraph/manifest.toml`, `base_plugins/heading/manifest.toml` |
| 2 | Embed manifests in `load_base_plugins` + unit tests | `crates/lopress-plugin/src/registry.rs` |
| 3 | `PluginMeta::paragraph()` + `PluginMeta::heading(level)` + update constructors | `crates/lopress-editor/src/model/types.rs` |
| 4 | `from_core` — delete hardcoded arms, add native arms | `crates/lopress-editor/src/model/from_core.rs` |
| 5 | `to_core` — add native arms, delete hardcoded arms | `crates/lopress-editor/src/model/to_core.rs` |
| 6 | `editor_for` — add paragraph/heading arms + widget adapters | `crates/lopress-editor/src/ui/blocks/editor_registry.rs` |
| 7 | Delete dead hardcoded arms in `render_body` and `block_view` | `plugin.rs`, `mod.rs` |
| 8 | Round-trip verification — all tests green | (no file changes) |

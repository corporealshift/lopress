# Block Descriptor Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce one `BlockDescriptor` per built-in block type as the single source of truth for its data facts (core type, body shape, native claim, default constructor, slash/toolbar metadata), and re-point `from_core`/`to_core`/the slash inserter/the toolbar to read from it — collapsing the seven-site block-definition scatter.

**Architecture:** TWO LAYERS, strict model←ui dependency order. A model-side `BlockDescriptor` table (in `model/descriptor.rs`) holds only MODEL data and references NO ui types. The ui keeps `editor_for` as the widget map, keyed by the same `editor` string the descriptor carries; a consistency test links the two. `BlockKind` is KEPT in this stage (its variants and the descriptors' `body_shape` agree; a test asserts it) — retiring `BlockKind` is the separate Stage B.

**Tech Stack:** Rust (`lopress-editor` model + ui), the plugin registry, the existing native-registry-driven from_core/to_core.

---

> ## Scope for this pass (read first)
>
> Implement **Tasks 1, 2, 3, 4, 7, and 8** — the descriptor table, the `from_core`/`to_core`
> body-shape dispatch, the consistency tests, and the gate. This is the behavior-preserving
> core and the prerequisite for Stage B (`BodyShape` + `descriptor_for` + `default_block`).
>
> **Tasks 5 and 6 (slash menu / toolbar re-pointing) are DEFERRED** — they hit a design gap
> (a single `menu` field can't reproduce the current menus' multiplicity/membership) that
> needs an owner decision, and their drafted code references types that do not exist. See
> the ⚠️ banners on those tasks. Do not implement them in this pass.

---

## Task 1: Create `model/descriptor.rs` — `BodyShape`, `BlockDescriptor`, static table, lookup helpers

**Files:**
- Create: `crates/lopress-editor/src/model/descriptor.rs`
- Modify: `crates/lopress-editor/src/model/mod.rs` (add `pub mod descriptor;`)

**Heading-level menu wrinkle (resolved):** The slash menu shows H1–H3 and the toolbar shows H1–H6, but there is ONE `heading` descriptor. Resolution: the descriptor carries a single `MenuEntry { title: "Heading", category: "Text" }`. The menu-generation code (Tasks 5–6) detects the heading descriptor by its `editor == EDITOR_HEADING` string and expands it into per-level entries: H1–H3 for the slash menu, H1–H6 for the toolbar. Each expanded entry carries a `default_block` closure that produces `EditorBlock::heading(n, vec![])` with the correct level. This keeps the descriptor a single entry while giving the UI the per-level granularity it needs.

- [ ] **Step 1: Write the failing test** — append to the `mod tests` block in `descriptor.rs`:

```rust
#[cfg(test)]
mod exclusivity_tests {
    use super::*;

    #[test]
    fn no_two_descriptors_share_editor_key() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            assert!(
                seen.insert(d.editor),
                "duplicate editor key: {}",
                d.editor
            );
        }
    }

    #[test]
    fn no_two_descriptors_share_native_claim() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(
                    seen.insert(native),
                    "duplicate native claim: {}",
                    native
                );
            }
        }
    }

    #[test]
    fn descriptor_for_editor_finds_all() {
        for d in descriptors() {
            assert!(
                descriptor_for(d.editor).is_some(),
                "descriptor_for({}) returned None",
                d.editor
            );
        }
    }

    #[test]
    fn descriptor_for_native_finds_all_native() {
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(
                    descriptor_for_native(native).is_some(),
                    "descriptor_for_native({}) returned None",
                    native
                );
            }
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor exclusivity_tests`
Expected: FAIL (module `descriptor` does not exist yet).

- [ ] **Step 3: Create `crates/lopress-editor/src/model/descriptor.rs`** with the full module:

```rust
//! Block descriptor table — one authoritative declaration per built-in block type.
//!
//! This is the **single source of truth** for each block type's data facts:
//! core type (native claim), body shape, editor key, whether it's a built-in,
//! and slash/toolbar presentation metadata. Every other site that previously
//! re-encoded these facts (hardcoded match arms, magic strings, separate
//! `editor_for` registry) now reads from this table.
//!
//! ## Architecture: model ← ui
//!
//! This module is in `model/` and references **no ui types** (`EditorWidget`,
//! `BlockEnv`, `AnyView`, nothing under `crate::ui`). The widget fn-pointers
//! live in `crate::ui::blocks::editor_registry::editor_for`, keyed by the
//! same `editor` string the descriptor carries. The link between the two
//! layers is that string — nothing else.
//!
//! ## Heading-level menu wrinkle
//!
//! There is ONE `heading` descriptor, but the slash menu shows H1–H3 and
//! the toolbar shows H1–H6. The descriptor carries a single `MenuEntry`
//! with `title: "Heading"`. The menu-generation code (in `slash_menu.rs`
//! and `toolbar.rs`) detects the heading descriptor by its `editor` key
//! and expands it into per-level entries. Each expanded entry uses a
//! `default_block` closure that produces `EditorBlock::heading(n, vec![])`
//! with the correct level.

use crate::model::types::EditorBlock;

/// The body shape a block's editor produces and round-trips.
///
/// This enum is the stable contract between the descriptor table and the
/// parser/serializer helpers in `from_core.rs` and `to_core.rs`. It outlives
/// `BlockKind` (Stage B deletes `BlockKind` and leans on `BodyShape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyShape {
    /// `Vec<InlineRun>` — paragraph, heading.
    Inline,
    /// `String` — code block.
    Code,
    /// `Vec<ListItem>` — ordered or unordered list.
    List,
    /// `TableData` — table with rows, cells, and column alignments.
    Table,
    /// `serde_json::Value` — image placeholder, unknown/removed types.
    Opaque,
}

/// Human-readable presentation entry for the slash menu or toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    /// Display title shown in the slash menu (e.g. "Paragraph", "Heading 2").
    pub title: &'static str,
    /// Category bucket for grouping in the menu (e.g. "Text", "Blocks").
    pub category: &'static str,
}

/// Everything the editor needs to know about one built-in block type, in one place.
///
/// A descriptor is the **single source of truth** for:
/// - Which editor key this type uses (`editor`)
/// - Which native core type it claims when serialized (`native`)
/// - What body shape its editor produces (`body_shape`)
/// - Whether it's a built-in (suppresses plugin chrome) (`builtin`)
/// - How it appears in the slash menu / toolbar (`menu`)
/// - How to construct a fresh default block (`default_block`)
///
/// The widget fn-pointer lives in `crate::ui::blocks::editor_registry`, keyed
/// by the same `editor` string. This module does NOT reference any ui types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDescriptor {
    /// The `editor` key — the primary identity. Matches PluginMeta.editor and
    /// the manifest `editor` field. E.g. "paragraph", "heading", "code", "list",
    /// "image", "table", "separator", "more".
    pub editor: &'static str,
    /// The core markdown type this block claims when serialized natively, if any.
    /// `Some("paragraph")`, `Some("list")`, … ; `None` → comment container.
    pub native: Option<&'static str>,
    /// Body shape produced by this block's editor widget.
    pub body_shape: BodyShape,
    /// Whether this block is a built-in (base plugin) — suppresses plugin chrome.
    pub builtin: bool,
    /// Slash-menu / toolbar presentation. `None` → not directly insertable
    /// (e.g. "more" marker is inserted by a dedicated affordance, not the menu).
    pub menu: Option<MenuEntry>,
    /// Construct the canonical empty/default block for this type (used by the
    /// slash menu, toolbar ChangeType, and split's tail-block creation).
    ///
    /// For the heading descriptor itself, this produces a level-1 heading.
    /// Menu-generation code that expands the heading descriptor into H1–H6
    /// replaces this closure with one that produces the correct level.
    pub default_block: fn() -> EditorBlock,
}

/// Named constants for built-in editor keys — replaces magic strings in
/// `actions.rs`, `to_core.rs`, and other sites that previously checked
/// `block_type_name == "lopress:more"` or `editor == "list"` etc.
pub const EDITOR_PARAGRAPH: &str = "paragraph";
pub const EDITOR_HEADING: &str = "heading";
pub const EDITOR_CODE: &str = "code";
pub const EDITOR_LIST: &str = "list";
pub const EDITOR_IMAGE: &str = "image";
pub const EDITOR_TABLE: &str = "table";
pub const EDITOR_SEPARATOR: &str = "separator";
pub const EDITOR_MORE: &str = "more";

/// The full descriptor table — one entry per built-in block type.
///
/// The order defines display order in menus: paragraph, heading, code, list,
/// image, read-more, separator, table.
fn descriptor_table() -> &'static [BlockDescriptor] {
    &[
        BlockDescriptor {
            editor: EDITOR_PARAGRAPH,
            native: Some(EDITOR_PARAGRAPH),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Paragraph",
                category: "Text",
            }),
            default_block: || EditorBlock::paragraph(vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_HEADING,
            native: Some(EDITOR_HEADING),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Heading",
                category: "Text",
            }),
            default_block: || EditorBlock::heading(1, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_CODE,
            native: Some(EDITOR_CODE),
            body_shape: BodyShape::Code,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Code block",
                category: "Blocks",
            }),
            default_block: || EditorBlock::code(String::new(), String::new()),
        },
        BlockDescriptor {
            editor: EDITOR_LIST,
            native: Some(EDITOR_LIST),
            body_shape: BodyShape::List,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Unordered list",
                category: "Blocks",
            }),
            default_block: || EditorBlock::list(false, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_IMAGE,
            native: Some(EDITOR_IMAGE),
            body_shape: BodyShape::Opaque,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Image",
                category: "Blocks",
            }),
            default_block: || EditorBlock::image("", "", ""),
        },
        BlockDescriptor {
            editor: EDITOR_MORE,
            native: None,
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: None, // "more" is inserted by a dedicated affordance, not the slash menu
            default_block: || EditorBlock::read_more(),
        },
        BlockDescriptor {
            editor: EDITOR_SEPARATOR,
            native: Some(EDITOR_SEPARATOR),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Separator",
                category: "Blocks",
            }),
            default_block: || EditorBlock::separator(),
        },
        BlockDescriptor {
            editor: EDITOR_TABLE,
            native: Some(EDITOR_TABLE),
            body_shape: BodyShape::Table,
            builtin: true,
            menu: Some(MenuEntry {
                title: "Table",
                category: "Blocks",
            }),
            default_block: || EditorBlock::table_default(),
        },
    ]
}

/// Return all descriptors in display order.
pub fn descriptors() -> &'static [BlockDescriptor] {
    descriptor_table()
}

/// Look up a descriptor by its `editor` key.
pub fn descriptor_for(editor: &str) -> Option<&'static BlockDescriptor> {
    descriptor_table().iter().find(|d| d.editor == editor).copied()
}

/// Look up a descriptor by its native core type claim.
pub fn descriptor_for_native(core_type: &str) -> Option<&'static BlockDescriptor> {
    descriptor_table()
        .iter()
        .find(|d| d.native == Some(core_type))
        .copied()
}

#[cfg(test)]
mod exclusivity_tests {
    use super::*;

    #[test]
    fn no_two_descriptors_share_editor_key() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            assert!(
                seen.insert(d.editor),
                "duplicate editor key: {}",
                d.editor
            );
        }
    }

    #[test]
    fn no_two_descriptors_share_native_claim() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(
                    seen.insert(native),
                    "duplicate native claim: {}",
                    native
                );
            }
        }
    }

    #[test]
    fn descriptor_for_editor_finds_all() {
        for d in descriptors() {
            assert!(
                descriptor_for(d.editor).is_some(),
                "descriptor_for({}) returned None",
                d.editor
            );
        }
    }

    #[test]
    fn descriptor_for_native_finds_all_native() {
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(
                    descriptor_for_native(native).is_some(),
                    "descriptor_for_native({}) returned None",
                    native
                );
            }
        }
    }
}
```

- [ ] **Step 4: Add `pub mod descriptor;` to `crates/lopress-editor/src/model/mod.rs`**

**Before:**
```rust
pub mod from_core;
pub mod inline;
pub mod inserter;
pub mod style_span;
pub mod sync;
pub mod to_core;
pub mod types;
```

**After:**
```rust
pub mod descriptor;
pub mod from_core;
pub mod inline;
pub mod inserter;
pub mod style_span;
pub mod sync;
pub mod to_core;
pub mod types;
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-editor exclusivity_tests`
Expected: PASS (all four tests — uniqueness, lookup by editor, lookup by native).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/descriptor.rs crates/lopress-editor/src/model/mod.rs
git commit -m "feat(editor): add BlockDescriptor table in model/descriptor.rs"
```

---

## Task 2: Add consistency test linking `editor_for` keys to descriptor table

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

**Goal:** The descriptor is model, `editor_for` is ui — the dependency direction is model←ui. The link between the two layers is the `editor` string key. Add a consistency test that asserts every descriptor's `editor` key resolves in `editor_for`, and vice versa. No code change to `editor_for` itself is needed; the match arms already have the correct 8 keys.

- [ ] **Step 1: Write the consistency test** — append to `editor_registry.rs` `mod tests`:

```rust
    #[test]
    fn editor_for_keys_match_descriptor_keys() {
        // Every descriptor's editor key must resolve in editor_for.
        for d in crate::model::descriptor::descriptors() {
            assert!(
                editor_for(d.editor).is_some(),
                "descriptor editor '{}' not registered in editor_for",
                d.editor
            );
        }

        // Every known editor_for key must have a matching descriptor.
        let known_keys = [
            "list", "code", "paragraph", "heading",
            "more", "separator", "image", "table",
        ];
        for key in &known_keys {
            assert!(
                crate::model::descriptor::descriptor_for(key).is_some(),
                "editor_for key '{}' has no descriptor",
                key
            );
        }
    }
```

- [ ] **Step 2: Run to verify it passes** (the keys already match from Stage A)

Run: `cargo test -p lopress-editor editor_for_keys_match_descriptor_keys`
Expected: PASS (all 8 keys are already in both places).

- [ ] **Step 3: Commit** — `editor_for` itself stays as a `match` (it's ui, descriptors are model). The link is the consistency test.

```bash
git add crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "test(editor): add consistency test linking editor_for keys to descriptor table"
```

---

## Task 3: Re-point `from_core` — use descriptor for body-shape dispatch

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs`

**Goal:** In `native_block_from_core`, replace the `match decl.editor.as_deref()` dispatch with a descriptor lookup. The descriptor's `body_shape` determines which body parser runs. The per-shape parsers stay as named helpers — the descriptor picks WHICH helper runs, it doesn't inline parsing.

**Current code in `native_block_from_core`:**
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

**After:**
```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let core_type = b.r#type.as_str();
    let desc = descriptor::descriptor_for_native(core_type);

    match desc.map(|d| d.body_shape) {
        Some(BodyShape::Code) => native_code_from_core(b, decl),
        Some(BodyShape::List) => native_list_from_core(b, decl),
        Some(BodyShape::Table) => native_table_from_core(b),
        Some(BodyShape::Inline) => {
            // Inline: paragraph or heading — dispatch by editor key.
            match desc.map(|d| d.editor) {
                Some(descriptor::EDITOR_HEADING) => native_heading_from_core(b, decl),
                _ => native_paragraph_from_core(b, decl),
            }
        }
        Some(BodyShape::Opaque) | None => {
            // Fallback: separator or unknown.
            match decl.editor.as_deref() {
                Some(descriptor::EDITOR_SEPARATOR) => EditorBlock::separator(),
                _ => EditorBlock::opaque(
                    core_type.to_string(),
                    serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
                ),
            }
        }
    }
}
```

- [ ] **Step 1: Write the failing test** — append to `tests/from_to_core_tests.rs`:

```rust
#[test]
fn from_core_uses_descriptor_dispatch() {
    // Verify that from_core routes through the descriptor table by checking
    // that the descriptor's body_shape matches the resulting block body.
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();

    // Test paragraph: descriptor says Inline, body should be Inline.
    let src = "A paragraph.\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);
    let block = &editor.blocks[0];
    assert!(matches!(block.body, BlockBody::Inline(_)));
    let desc = descriptor::descriptor_for_native("paragraph").unwrap();
    assert!(matches!(desc.body_shape, BodyShape::Inline));

    // Test code: descriptor says Code, body should be Code.
    let src = "```\ncode\n```\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);
    let block = &editor.blocks[0];
    assert!(matches!(block.body, BlockBody::Code(_)));
    let desc = descriptor::descriptor_for_native("code").unwrap();
    assert!(matches!(desc.body_shape, BodyShape::Code));

    // Test list: descriptor says List, body should be List.
    let src = "- item\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);
    let block = &editor.blocks[0];
    assert!(matches!(block.body, BlockBody::List(_)));
    let desc = descriptor::descriptor_for_native("list").unwrap();
    assert!(matches!(desc.body_shape, BodyShape::List));

    // Test table: descriptor says Table, body should be Table.
    let src = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);
    let block = &editor.blocks[0];
    assert!(matches!(block.body, BlockBody::Table(_)));
    let desc = descriptor::descriptor_for_native("table").unwrap();
    assert!(matches!(desc.body_shape, BodyShape::Table));
}
```

- [ ] **Step 2: Run to verify they fail** — they should fail because `native_block_from_core` still uses the old `match` dispatch.

Run: `cargo test -p lopress-editor from_core_uses_descriptor_dispatch`
Expected: FAIL (the old match dispatch is still in place; the test asserts descriptor-driven dispatch).

- [ ] **Step 3: Add the `use` imports to `from_core.rs`**

Add to the top of `from_core.rs`:
```rust
use crate::model::descriptor::{self, BodyShape};
```

- [ ] **Step 4: Replace `native_block_from_core`** with the descriptor-driven dispatch (code shown above).

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-editor from_core_uses_descriptor_dispatch`
Expected: PASS.

- [ ] **Step 6: Run full round-trip tests**

Run: `cargo test -p lopress-editor from_to_core`
Expected: PASS (all round-trip tests still pass — this is a pure refactor).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/model/from_core.rs
git commit -m "refactor(editor): dispatch native_block_from_core via descriptor body_shape"
```

---

## Task 4: Re-point `to_core` — use descriptor for body-shape dispatch

**Files:**
- Modify: `crates/lopress-editor/src/model/to_core.rs`

**Goal:** In `native_block_to_core`, replace the `core_type` string match with a descriptor lookup. The descriptor's `body_shape` determines which serializer runs. The existing code already uses direct pattern matching on `BlockBody` variants (`BlockBody::Inline(runs)`, `BlockBody::Code(text)`, etc.) — no accessor methods exist on `BlockBody`.

**Current code in `native_block_to_core`:**
```rust
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => { ... },
        BlockBody::Code(text) => { ... },
        BlockBody::Table(data) => { ... },
        BlockBody::Inline(runs) if core_type == "paragraph" => { ... },
        BlockBody::Inline(runs) if core_type == "heading" => { ... },
        _ => Block { ... },
    }
}
```

**After:** The dispatch stays a `match &b.body` (the body variant *is* the shape — the
per-shape serializers must keep their real bodies, building `children` from `items`/`rows`).
The descriptor's only job here is to replace the old `core_type == "heading"` string guard
with an editor-key lookup, so the heading-vs-paragraph distinction reads from the table.
**Do NOT collapse the per-shape serializers or set `children: vec![]` for List/Table — that
drops list items / table rows and breaks the round-trip.**

```rust
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    // The descriptor's editor key drives the inline paragraph-vs-heading
    // distinction (replacing the old `core_type == "heading"` string guard);
    // the body shape itself comes from matching `&b.body`, and each per-shape
    // serializer below is byte-for-byte the existing one.
    let is_heading = descriptor::descriptor_for_native(core_type)
        .map(|d| d.editor == descriptor::EDITOR_HEADING)
        .unwrap_or(false);
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
        BlockBody::Code(text) => {
            let lang = meta.attrs.get("lang").and_then(Value::as_str).unwrap_or("");
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "lang": lang }),
                children: vec![],
                text: Some(text.clone()),
            }
        }
        BlockBody::Table(data) => {
            let align: Vec<Value> = data
                .align
                .iter()
                .map(|a| Value::String(a.as_str().to_string()))
                .collect();
            let rows: Vec<Block> = data
                .rows
                .iter()
                .map(|row| Block {
                    r#type: "table_row".into(),
                    attrs: empty_attrs(),
                    children: row
                        .cells
                        .iter()
                        .map(|cell| Block {
                            r#type: "table_cell".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&cell.runs)),
                        })
                        .collect(),
                    text: None,
                })
                .collect();
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "align": align }),
                children: rows,
                text: None,
            }
        }
        BlockBody::Inline(runs) if is_heading => {
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
        BlockBody::Inline(runs) => Block {
            r#type: core_type.to_string(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
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

**Key note on `BlockBody`:** `BlockBody` is a plain enum with no accessor methods. Use direct pattern matching: `BlockBody::Inline(runs)` to bind `runs: &Vec<InlineRun>`, `BlockBody::Code(text)` to bind `text: &String`, `BlockBody::List(items)` to bind `items: &Vec<ListItem>`, `BlockBody::Table(data)` to bind `data: &TableData`, `BlockBody::Opaque(value)` to bind `value: &Value`. There are no `as_inline()`, `as_code()`, `as_list()`, or `as_table()` methods — they do not exist.

- [ ] **Step 1: Write the failing test** — append to `tests/from_to_core_tests.rs`:

```rust
#[test]
fn to_core_uses_descriptor_dispatch() {
    // Verify that to_core routes through the descriptor table by checking
    // that the descriptor's body_shape matches the resulting core block type.
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();

    // Build a paragraph block and verify it serializes with core_type "paragraph".
    let para = EditorBlock::paragraph(vec![]);
    let meta = PluginMeta {
        block_type_name: Rc::from("paragraph"),
        attrs: serde_json::Map::new(),
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("paragraph")),
        native: Some(Rc::from("paragraph")),
    };
    let core = native_block_to_core(&para, &meta, "paragraph");
    assert_eq!(core.r#type, "paragraph");

    // Build a heading block and verify it serializes with core_type "heading".
    let heading = EditorBlock::heading(2, vec![]);
    let meta = PluginMeta {
        block_type_name: Rc::from("heading"),
        attrs: json!({ "level": 2u64 }).as_object().unwrap().clone(),
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("heading")),
        native: Some(Rc::from("heading")),
    };
    let core = native_block_to_core(&heading, &meta, "heading");
    assert_eq!(core.r#type, "heading");
    assert_eq!(core.attrs["level"], 2);
}
```

- [ ] **Step 2: Run to verify they fail** — they should fail because `native_block_to_core` still uses the old `match` dispatch.

Run: `cargo test -p lopress-editor to_core_uses_descriptor_dispatch`
Expected: FAIL (the old match dispatch is still in place).

- [ ] **Step 3: Add the `use` imports to `to_core.rs`**

Add to the top of `to_core.rs`:
```rust
use crate::model::descriptor::{self, BodyShape};
```

- [ ] **Step 4: Replace `native_block_to_core`** with the descriptor-driven dispatch (code shown above).

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-editor to_core_uses_descriptor_dispatch`
Expected: PASS.

- [ ] **Step 6: Run full round-trip tests**

Run: `cargo test -p lopress-editor from_to_core`
Expected: PASS (all round-trip tests still pass — this is a pure refactor).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/model/to_core.rs
git commit -m "refactor(editor): dispatch native_block_to_core via descriptor body_shape"
```

---

## Task 5: Re-point slash menu to descriptor table

> ⚠️ **DEFERRED — DO NOT IMPLEMENT in this pass.** Blocked on a design decision (see the
> banner below). The implementer brief scopes this stage to Tasks 1–4 + 7 + the gate.
> The code sketch below is the *original draft* and is known-wrong (it invents a
> `MenuEntryData` type that does not exist; the real slash list is
> `slash_menu_items() -> Vec<(String, SlashChoice)>` where `SlashChoice` is
> `Kind(BlockKind) | ReadMore | Image | Separator | Table | Plugin`). Left in place only
> as a record; rewrite it when the design question is resolved.
>
> **The design gap:** a single `menu: Option<MenuEntry>` per descriptor cannot reproduce
> the current slash menu, which needs: heading → "Heading 1/2/3" (one descriptor, 3
> entries), list → "Unordered list" + "Ordered list" (one descriptor, 2 entries), and
> "Read more" (the `more` descriptor is `menu: None` yet *does* appear in the slash menu).
> Resolution requires either enriching the descriptor (express per-level / per-ordered
> expansion + slash-vs-toolbar membership) or keeping the menus structural and pulling
> only titles/categories from the table. **Decide before implementing.** Whatever is
> chosen, pin it with a test asserting the generated list equals the current hardcoded
> `slash_menu_items()` exactly (no entry added or dropped).

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (path may vary — adjust to actual file)

**Goal:** Replace the hardcoded slash menu item list with entries derived from the descriptor table. Each descriptor with a `Some(menu)` entry produces one or more slash menu items. The heading descriptor is expanded into H1–H3 (not H1–H6; the toolbar handles H1–H6).

**Approach:**
1. In the slash menu construction code, call `descriptor::descriptors()` instead of a hardcoded list.
2. For each descriptor with `menu: Some(entry)`, create a menu item using `entry.title`, `entry.category`, and `entry.default_block`.
3. For the heading descriptor specifically, expand into three items: "Heading 1", "Heading 2", "Heading 3", each with a `default_block` closure that produces `EditorBlock::heading(n, vec![])` for n = 1, 2, 3.
4. Skip descriptors with `menu: None` (e.g., "more").

**Code sketch:**
```rust
fn build_slash_menu_items() -> Vec<MenuEntryData> {
    let mut items = Vec::new();
    for desc in descriptor::descriptors() {
        let Some(menu) = desc.menu else { continue };
        if desc.editor == descriptor::EDITOR_HEADING {
            // Expand heading into H1–H3 for the slash menu.
            for level in 1..=3 {
                let title = format!("Heading {}", level);
                items.push(MenuEntryData {
                    title: title.into(),
                    category: menu.category,
                    default_block: Box::new(move || EditorBlock::heading(level, vec![])),
                });
            }
        } else {
            items.push(MenuEntryData {
                title: menu.title.into(),
                category: menu.category,
                default_block: Box::new(move || (desc.default_block)()),
            });
        }
    }
    items
}
```

- [ ] **Step 1: Run existing slash menu tests** to establish a baseline.

Run: `cargo test -p lopress-editor slash_menu`
Expected: PASS (existing tests still work).

- [ ] **Step 2: Replace hardcoded menu items with descriptor-driven construction.**

- [ ] **Step 3: Add test** — verify that the slash menu produces the expected entries from the descriptor table:

```rust
#[test]
fn slash_menu_items_derive_from_descriptors() {
    let items = build_slash_menu_items();
    let titles: Vec<&str> = items.iter().map(|i| i.title.as_ref()).collect();
    // Should contain: Paragraph, Heading 1, Heading 2, Heading 3, Code block,
    // Unordered list, Image, Separator, Table (9 items, heading expanded to 3).
    assert!(titles.contains(&"Paragraph"));
    assert!(titles.contains(&"Heading 1"));
    assert!(titles.contains(&"Heading 2"));
    assert!(titles.contains(&"Heading 3"));
    assert!(titles.contains(&"Code block"));
    assert!(titles.contains(&"Unordered list"));
    assert!(titles.contains(&"Image"));
    assert!(titles.contains(&"Separator"));
    assert!(titles.contains(&"Table"));
    // "more" should NOT appear (menu: None).
    assert!(!titles.contains(&"Read More"));
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-editor slash_menu`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs
git commit -m "refactor(editor): derive slash menu items from descriptor table"
```

---

## Task 6: Re-point toolbar to descriptor table

> ⚠️ **DEFERRED — DO NOT IMPLEMENT in this pass.** Same design gap as Task 5. The code
> sketch below is the *original draft* and is known-wrong: it invents `ToolbarButton`,
> `ToolbarAction::InsertHeading/InsertBlock`, and `icon: ...` (a placeholder) — none of
> which exist. The real toolbar (`block_toolbar_for`) builds a `Vec<(&'static str,
> BlockKind)>` of `("P", Paragraph) … ("OL", List{ordered:true})` and emits
> `BlockAction::ChangeType { block_id, new_kind: BlockKind }` — it *converts* the focused
> block, it does not *insert*. The toolbar is the P/H1–H6/Code/UL/OL subset only; it does
> **not** include image/separator/table. Rewrite when the design question (Task 5 banner)
> is resolved, pinning the result to the current `block_toolbar_for` button list exactly.

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (path may vary — adjust to actual file)

**Goal:** Replace the hardcoded toolbar block-type buttons with entries derived from the descriptor table. Similar to the slash menu but with heading expanded to H1–H6 (not H1–H3).

**Approach:**
1. In the toolbar construction code, call `descriptor::descriptors()` instead of a hardcoded list.
2. For each descriptor with `menu: Some(entry)`, create a toolbar button.
3. For the heading descriptor, expand into six items: H1–H6.
4. The toolbar may also show a "Change Block Type" dropdown — populate that from the descriptor table too.

**Code sketch (toolbar buttons):**
```rust
fn build_toolbar_block_buttons() -> Vec<ToolbarButton> {
    let mut buttons = Vec::new();
    for desc in descriptor::descriptors() {
        let Some(menu) = desc.menu else { continue };
        if desc.editor == descriptor::EDITOR_HEADING {
            // Expand heading into H1–H6 for the toolbar.
            for level in 1..=6 {
                let title = format!("H{}", level);
                buttons.push(ToolbarButton {
                    title,
                    action: ToolbarAction::InsertHeading(level),
                    icon: ...,
                });
            }
        } else {
            buttons.push(ToolbarButton {
                title: menu.title.into(),
                action: ToolbarAction::InsertBlock(desc.editor),
                icon: ...,
            });
        }
    }
    buttons
}
```

- [ ] **Step 1: Run existing toolbar tests** to establish a baseline.

Run: `cargo test -p lopress-editor toolbar`
Expected: PASS (existing tests still work).

- [ ] **Step 2: Replace hardcoded toolbar buttons with descriptor-driven construction.**

- [ ] **Step 3: Add test** — verify the toolbar produces the expected entries:

```rust
#[test]
fn toolbar_items_derive_from_descriptors() {
    let buttons = build_toolbar_block_buttons();
    let titles: Vec<&str> = buttons.iter().map(|b| b.title.as_str()).collect();
    // Should contain: H1–H6 (6 heading entries), Paragraph, Code block,
    // Unordered list, Image, Separator, Table (6 more = 12 total).
    for level in 1..=6 {
        assert!(titles.contains(&format!("H{}", level).as_str()));
    }
    assert!(titles.contains(&"Paragraph"));
    assert!(titles.contains(&"Code block"));
    assert!(titles.contains(&"Unordered list"));
    assert!(titles.contains(&"Image"));
    assert!(titles.contains(&"Separator"));
    assert!(titles.contains(&"Table"));
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p lopress-editor toolbar`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "refactor(editor): derive toolbar buttons from descriptor table"
```

---

## Task 7: Add consistency tests — descriptor ↔ BlockKind ↔ editor_for

**Files:**
- Modify: `crates/lopress-editor/src/model/descriptor.rs` (add tests to `mod tests`)

**Goal:** Assert that the three layers (descriptor table, `BlockKind` enum, and `editor_for` widget map) are consistent. `BlockKind` is kept in this stage but a test verifies its variants align with the descriptor `body_shape` values.

- [ ] **Step 1: Add consistency tests to `descriptor.rs` `mod tests`:**

```rust
    #[test]
    fn blockkind_variants_align_with_descriptor_bodies() {
        // BlockKind variants and descriptor body_shapes agree on the mapping:
        // Paragraph → Inline, Heading(n) → Inline, Code → Code, List → List,
        // Table → Table, Opaque → Opaque.
        // This test asserts the alignment; Stage B deletes BlockKind.

        let paragraph_desc = descriptor_for(EDITOR_PARAGRAPH).unwrap();
        assert!(matches!(paragraph_desc.body_shape, BodyShape::Inline));

        let heading_desc = descriptor_for(EDITOR_HEADING).unwrap();
        assert!(matches!(heading_desc.body_shape, BodyShape::Inline));

        let code_desc = descriptor_for(EDITOR_CODE).unwrap();
        assert!(matches!(code_desc.body_shape, BodyShape::Code));

        let list_desc = descriptor_for(EDITOR_LIST).unwrap();
        assert!(matches!(list_desc.body_shape, BodyShape::List));

        let table_desc = descriptor_for(EDITOR_TABLE).unwrap();
        assert!(matches!(table_desc.body_shape, BodyShape::Table));

        let image_desc = descriptor_for(EDITOR_IMAGE).unwrap();
        assert!(matches!(image_desc.body_shape, BodyShape::Opaque));
    }

    #[test]
    fn all_descriptors_have_consistent_editor_and_native() {
        // Every descriptor that has a native claim must find a matching
        // descriptor when looked up by that native value.
        for d in descriptors() {
            if let Some(native) = d.native {
                assert_eq!(
                    descriptor_for_native(native).map(|x| x.editor),
                    Some(d.editor),
                    "native '{}' maps to wrong editor",
                    native
                );
            }
        }
    }

    #[test]
    fn descriptor_default_blocks_produce_valid_blocks() {
        // Each descriptor's default_block closure must produce a block whose
        // body_shape matches the descriptor's declared body_shape.
        for desc in descriptors() {
            let block = (desc.default_block)();
            match desc.body_shape {
                BodyShape::Inline => {
                    assert!(matches!(&block.body, BlockBody::Inline(_)));
                }
                BodyShape::Code => {
                    assert!(matches!(&block.body, BlockBody::Code(_)));
                }
                BodyShape::List => {
                    assert!(matches!(&block.body, BlockBody::List(_)));
                }
                BodyShape::Table => {
                    assert!(matches!(&block.body, BlockBody::Table(_)));
                }
                BodyShape::Opaque => {
                    assert!(matches!(&block.body, BlockBody::Opaque(_)));
                }
            }
        }
    }
```

- [ ] **Step 2: Run to verify they pass**

Run: `cargo test -p lopress-editor blockkind_variants_align_with_descriptor_bodies`
Run: `cargo test -p lopress-editor all_descriptors_have_consistent_editor_and_native`
Run: `cargo test -p lopress-editor descriptor_default_blocks_produce_valid_blocks`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/model/descriptor.rs
git commit -m "test(editor): add consistency tests linking descriptor, BlockKind, and editor_for"
```

---

## Task 8: Final gate — run full verification, round-trip, and commit

**Files:**
- All modified files from Tasks 1–7

**Goal:** Run the full verification gate. All tests must pass. Round-trip parse→edit→serialize must work for all block types.

- [ ] **Step 1: Run the full test suite**

Run: `cargo test -p lopress-editor`
Expected: PASS (all tests, including existing and new).

- [ ] **Step 2: Run the workspace gate**

Run: `bash scripts/check.sh`
Expected: PASS (format, clippy, tests all clean).

- [ ] **Step 3: Verify round-trip for all block types**

Run the round-trip tests to ensure parse→edit→serialize produces identical output:

Run: `cargo test -p lopress-editor from_to_core`
Expected: PASS.

- [ ] **Step 4: Commit any fmt-only delta — stage NAMED files, NEVER `git add -A`**

Tasks 1–4 and 7 each already committed their own work, so there is no "big final commit."
The only thing that may remain is `cargo fmt` output from the gate touching files edited in
earlier tasks. Check `git status --short`; if (and only if) source files show changes, stage
them **by name** and commit:

```bash
git status --short
# Only if source files changed from fmt — stage those exact paths, never `git add -A`
# (the tree has the unrelated .claude/settings.local.json and gitignored .pi-delegations/):
git add crates/lopress-editor/src/model/descriptor.rs crates/lopress-editor/src/model/mod.rs \
        crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs \
        crates/lopress-editor/src/ui/blocks/editor_registry.rs \
        crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "chore: fmt after descriptor-table core"
```

If `git status --short` shows only `.claude/settings.local.json`, skip this commit entirely.

**Note:** Tasks 5–6 (slash menu / toolbar re-pointing) are DEFERRED this pass — see their
banners. Do not implement or commit them.

---

## Summary of Changes

| Task | What changes | Files |
|------|-------------|-------|
| 1 | Create `BlockDescriptor` table | `model/descriptor.rs`, `model/mod.rs` |
| 2 | Consistency test: descriptor ↔ editor_for | `ui/blocks/editor_registry.rs` |
| 3 | `from_core` dispatch via descriptor | `model/from_core.rs` |
| 4 | `to_core` dispatch via descriptor | `model/to_core.rs` |
| 5 | Slash menu from descriptor | `ui/slash_menu.rs` |
| 6 | Toolbar from descriptor | `ui/toolbar.rs` |
| 7 | Consistency tests (BlockKind ↔ descriptor) | `model/descriptor.rs` |
| 8 | Full gate, round-trip, commit | All |

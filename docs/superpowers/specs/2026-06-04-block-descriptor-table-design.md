# Block Descriptor Table — One Source of Truth Per Block Type

**Date:** 2026-06-04
**Author:** Kyle
**Status:** draft — design review output, pending implementation planning
**Related:**
- `docs/superpowers/specs/2026-06-04-everything-is-a-plugin-and-retire-blockkind-design.md` (consumes this table)
- `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (the registry this generalizes)

---

## 1. Background

Adding or changing one built-in block type today means editing **seven disconnected
sites**, with no compiler check that they agree:

1. `BlockKind` + `BlockBody` variants — `crates/lopress-editor/src/model/types.rs`
2. `EditorBlock::xxx()` + `PluginMeta::xxx()` constructors — `types.rs:155-401`
3. `block_from_core` parse arm — `crates/lopress-editor/src/model/from_core.rs:154-166`
4. `block_to_core` serialize arm — `crates/lopress-editor/src/model/to_core.rs:78-156`
5. `editor_for` registry — `crates/lopress-editor/src/ui/blocks/editor_registry.rs:35-45`
6. slash-menu inserter entry — `crates/lopress-editor/src/ui/slash_menu.rs`, `model/inserter.rs`
7. toolbar `ChangeType` entry — `crates/lopress-editor/src/ui/toolbar.rs`

The recent `table`/`separator` work touched every one of these. Each new block re-encodes
the same handful of facts (core type name, editor key, native-vs-container serialization,
default body, display title) in seven places, in seven shapes. When they disagree, the
failure is silent: a block that parses but won't serialize, or serializes but has no
inserter entry.

The 2026-05-17 spec's `editor_for` and native-registry were the first move toward
data-driven dispatch. This spec finishes that idea: **one descriptor per block type that
every other site reads from.**

---

## 2. Goal

A single `BlockDescriptor` table is the authoritative declaration of every built-in
block type. `from_core`, `to_core`, `editor_for`, the slash inserter, and the toolbar
all *read from* it. Adding a block type is one new descriptor plus its widget function —
not a seven-file scavenger hunt. Disagreement becomes structurally impossible because
there is only one copy of each fact.

---

## 3. The descriptor

A new module `crates/lopress-editor/src/model/descriptor.rs`:

```rust
/// The body shape a block's editor produces and round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyShape {
    Inline,   // Vec<InlineRun>
    Code,     // String
    List,     // Vec<ListItem>
    Table,    // TableData
    Opaque,   // serde_json::Value (image placeholder, unknown types)
}

/// Everything the editor needs to know about one built-in block type, in one place.
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
    /// The editor widget constructor (the EditorWidget fn pointer).
    pub widget: EditorWidget,
    /// Whether this block is a built-in (base plugin) — suppresses plugin chrome.
    pub builtin: bool,
    /// Slash-menu / toolbar presentation. `None` → not directly insertable
    /// (e.g. "more" marker is inserted by a dedicated affordance, not the menu).
    pub menu: Option<MenuEntry>,
    /// Construct the canonical empty/default block for this type (used by the
    /// slash menu, toolbar ChangeType, and split's tail-block creation).
    pub default_block: fn() -> EditorBlock,
}

pub struct MenuEntry {
    pub title: &'static str,     // "Paragraph", "Heading 2", "Bulleted list"
    pub category: &'static str,  // "Text", "Blocks", …
}
```

The table itself is a `&'static [BlockDescriptor]` plus lookup helpers:

```rust
pub fn descriptor_for(editor: &str) -> Option<&'static BlockDescriptor>;
pub fn descriptor_for_native(core_type: &str) -> Option<&'static BlockDescriptor>;
pub fn descriptors() -> &'static [BlockDescriptor];  // for the inserter / toolbar
```

---

## 4. What each site reads instead of re-encoding

| Site | Today | After |
|------|-------|-------|
| `editor_for(key)` | hand-written `match` arm per type (`editor_registry.rs:35`) | `descriptor_for(key).map(\|d\| d.widget)` |
| native lookup in `from_core` | `native_block_from_core` per-key `match` (`from_core.rs:154`) | `descriptor_for_native(core_type)` → `body_shape` drives the body parser |
| `to_core` native arm | per-body-shape `match` (`to_core.rs:78`) | dispatch on `descriptor.body_shape` |
| slash inserter (built-ins) | hardcoded list in `slash_menu`/`inserter` | iterate `descriptors()` filtered by `menu.is_some()` |
| toolbar type-cycler | hardcoded P/H1–H6/Code/list buttons | built from `descriptors()` with `menu` |
| `coerce_body_to_editor` / `body_matches_editor` | `match` on kind/key (`actions.rs:859,913`) | `descriptor_for(key).body_shape` |
| `default_block` for split tail / ChangeType | scattered `EditorBlock::xxx()` ctors | `descriptor.default_block()` |

The per-shape body **parsers** (inline/code/list/table) and **serializers** stay as
named helpers — the descriptor's `body_shape` selects which helper runs, it does not
inline the parsing. DRY without a god-function.

---

## 5. Relationship to plugin manifests

Built-in descriptors are the `&'static` table in this module. **User plugins** are not
in the static table — they come from `PluginRegistry` at runtime. The two are unified
behind the lookup helpers: `descriptor_for(key)` checks the static table first; a future
extension can fall through to a runtime registry view for plugin-contributed editor keys
(currently always the generic attr-form path). This spec only tables the built-ins; the
seam for runtime descriptors is documented but not built (mirrors how 05-17 documented
the asset seam without wiring it).

Crucially, the descriptor's facts (`editor`, `native`, `builtin`) must **match** the
corresponding base-plugin `manifest.toml`. A debug-assert / test cross-checks the static
table against the loaded base-plugin registry at startup so they can't silently drift
(see §7).

---

## 6. Magic-string elimination

The scattered identity checks (`&*meta.block_type_name == "lopress:more"` —
`actions.rs:241`, `to_core.rs:25`; `editor == "list"` etc.) are replaced by descriptor
lookups or named constants exported from this module:

```rust
pub const EDITOR_MORE: &str = "more";
pub const EDITOR_SEPARATOR: &str = "separator";
// …
```

`is_read_more(block)` (`actions.rs:237`) becomes
`block.plugin.editor.as_deref() == Some(EDITOR_MORE)`. One module owns the strings.

---

## 7. Testing

- **Table/manifest consistency** (new test): for every base-plugin block loaded by
  `load_base_plugins()`, assert a matching `BlockDescriptor` exists with the same
  `editor`, `native`, and `builtin`. This is the guard that prevents the table and the
  manifests from drifting — the single most valuable test in this spec.
- **`descriptor_for` / `descriptor_for_native` round-trip**: every descriptor is found
  by both its editor key and (if native) its core type; no two descriptors share an
  editor key or a native claim (the exclusivity invariant from 05-17, now table-checked).
- **Inserter built from descriptors** (`slash_menu_tests.rs`): the built-in slash-menu
  entries equal the `descriptors()` filtered by `menu.is_some()` — re-pointed off the
  old hardcoded list.
- **Existing round-trip suite stays green** — this is a pure refactor; on-disk output is
  byte-identical.

---

## 8. Sequencing

This table is the natural prerequisite for **Stage B** of the
everything-is-a-plugin spec (`2026-06-04-everything-is-a-plugin-and-retire-blockkind-design.md` §10):
`BlockKind` retirement re-keys dispatch onto editor-key → body-shape, which is exactly
what `BodyShape` + `descriptor_for` provide.

Recommended order:
1. Everything-is-a-plugin **Stage A** (paragraph/heading carry `PluginMeta`).
2. **This spec** — introduce the descriptor table; re-point `editor_for`, the inserter,
   the toolbar, and the conversion helpers to read from it. `BlockKind` still exists but
   is now redundant with the table.
3. Everything-is-a-plugin **Stage B** — delete `BlockKind`, leaning on `BodyShape`.

This spec is independently shippable after Stage A: it consolidates the seven sites even
while `BlockKind` still exists (the descriptor's `body_shape` and the enum agree; a test
asserts it). That makes Stage B a deletion, not a re-architecture.

Expect a single implementation plan (one focused refactor), unlike the two-stage
everything-is-a-plugin effort.

---

## 9. Non-goals

- No on-disk format change.
- No runtime/plugin-contributed descriptors beyond the documented seam — built-ins only.
- No removal of the per-shape body parser/serializer helpers; the table selects them, it
  doesn't absorb them.
- `BlockKind` deletion is the *other* spec; this one can land with `BlockKind` still
  present.

---

## 10. Decisions

### One static descriptor table, not a builder-registered map
A `&'static [BlockDescriptor]` with `fn`-pointer widgets keeps dispatch a pure lookup —
no global mutable state, no init order, matching the 05-17 decision to make `editor_for`
a `match`-based free function. The table is just that `match` turned into data.

### `BodyShape` enum instead of reusing `BlockBody` discriminants
A dedicated 5-variant enum is the stable contract the descriptor declares; `BlockBody`
is the runtime payload. Keeping them separate means the descriptor doesn't need a sample
`BlockBody` value and the "what shape does this editor produce" question has a
first-class type. This enum outlives `BlockKind` and absorbs its body-shape role.

### Table cross-checked against manifests by test, not merged with them
The base-plugin manifests stay the on-disk source for plugin loading; the static table is
the in-Rust source for editor wiring. A consistency test binds them. Merging them (e.g.
generating one from the other) is more machinery than the built-in set justifies.

### Built-ins only; runtime descriptor seam documented, not built
Same staging philosophy as 05-17's asset seam — design the extension point, implement it
when a user-plugin custom editor actually needs it.

---

## 11. Open questions for the planner

- **`default_block` vs. existing `EditorBlock::xxx()` ctors**: fold the existing
  constructors (`EditorBlock::table_default`, `::separator`, …) into the descriptors'
  `default_block` pointers, or have `default_block` call them? Proposal: descriptors
  point at the existing ctors (no logic moves), so this stays a wiring change.
- **Where the table lives**: `model/descriptor.rs` (proposed) vs. alongside
  `editor_registry.rs` in `ui/blocks/`. The table references `EditorWidget` (a `ui` type)
  *and* model ctors — confirm the dependency direction during planning to avoid a
  `ui → model → ui` cycle. Likely the widget pointers are injected, keeping the table in
  `model`.

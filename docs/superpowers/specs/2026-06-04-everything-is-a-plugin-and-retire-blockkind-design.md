# Everything Is a Plugin — Finish the Migration and Retire `BlockKind`

**Date:** 2026-06-04
**Author:** Kyle
**Status:** draft — design review output, pending implementation planning
**Related:**
- `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (the capability model this finishes)
- `docs/superpowers/specs/2026-06-04-block-descriptor-table-design.md` (the consolidation this depends on)
- `docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md` (the `EditBlockBody` / generic-undo groundwork)

---

## 1. Background

The 2026-05-17 spec built the plugin-capability machinery (editor registry, native
registry, registry-driven `from_core`/`to_core`) and migrated `list`. Subsequent work
migrated `code`, `image`, `table`, and `separator` the same way. Every one of those
block types now carries `PluginMeta` and routes through the plugin path
(`editor_for(key)`).

**Two block types never migrated: `paragraph` and `heading`.** They alone have
`plugin: None` and dispatch through hardcoded `(BlockKind, BlockBody)` match arms in
four places:

- `block_view` — `crates/lopress-editor/src/ui/blocks/mod.rs:84-134`
- `render_body` (the plugin path's own fallback) — `crates/lopress-editor/src/ui/blocks/plugin.rs:388-444`
- `block_from_core` — `crates/lopress-editor/src/model/from_core.rs:33-46`
- `block_to_core` — `crates/lopress-editor/src/model/to_core.rs:38-73`

Because two types still need it, `BlockKind` survives as a **parallel identity** next to
`PluginMeta`. That duality is the core architectural smell from the 2026-06-04 review:

1. **Two dispatch tables.** `editor_for(key)` (string → widget) *and* the
   `(BlockKind, BlockBody)` matches above. `code` appears in both.
2. **Hand-maintained mirror invariants.** `code` lang lives in `BlockKind::Code.lang`
   *and* `PluginMeta.attrs["lang"]`; `list` ordered in `BlockKind::List` *and*
   `attrs["ordered"]`. `apply_edit_attrs` has bespoke sync code
   (`crates/lopress-editor/src/actions.rs:208-219`) to keep the `lang` mirror honest.
3. **Magic-string identity checks** scattered across the model
   (`&*meta.block_type_name == "lopress:more"` in `actions.rs:241`, `to_core.rs:25`).

The 2026-05-17 spec deliberately deferred `BlockKind` retirement: "Retiring it is
impossible while paragraph, heading, and code still depend on it; retirement is
reconsidered after they migrate." Code has since migrated. This spec migrates the last
two and **retires `BlockKind` entirely.**

---

## 2. Goal

After this spec, every block — paragraph and heading included — carries `PluginMeta` and
is identified by its `editor` key + body shape. `BlockKind` is deleted from the
codebase. There is exactly one dispatch path, no mirror invariants, and "what kind of
block is this?" has one answer: its plugin identity.

No on-disk format change. No user-visible behavior change. Documents round-trip
byte-identically before and after.

---

## 3. Scope

**In scope**
- Base plugins for `paragraph` and `heading` (manifests + `editor`/`native` keys).
- `editor_for` arms for `"paragraph"` and `"heading"`; reshape their widgets to the
  `EditorWidget` signature.
- Delete the hardcoded `(BlockKind, BlockBody)` arms in `block_view`, `render_body`,
  `block_from_core`, `block_to_core`.
- **Retire `BlockKind`:** remove the enum and every reference, relocating the data it
  carried (`heading` level, `code` lang, `list` ordered) to `PluginMeta.attrs`, and
  re-key all dispatch that matched on it (`apply`, `ChangeType`, `coerce_body_to_kind`,
  `body_matches_kind`) onto body shape + editor key.

**Out of scope**
- The block-descriptor table itself (`docs/superpowers/specs/2026-06-04-block-descriptor-table-design.md`).
  This spec assumes that table exists as the registration source of truth; sequencing in §10.
- Custom JS/WASM plugin editor widgets (still deferred from 05-17).
- Any change to the markdown on disk.

---

## 4. The two new base plugins

Paragraph and heading become native-claiming base plugins, exactly like `list`/`code`.

`base_plugins/paragraph/manifest.toml`:

```toml
[[blocks]]
name    = "paragraph"
editor  = "paragraph"
native  = "paragraph"
builtin = true
```

`base_plugins/heading/manifest.toml`:

```toml
[[blocks]]
name    = "heading"
editor  = "heading"
native  = "heading"
builtin = true

[blocks.attrs]
level = { type = "number", ui = "hidden" }
```

`level` moves from `BlockKind::Heading(u8)` into `attrs["level"]` (an integer 1..=6),
mirroring how `ordered`/`lang` already live in attrs. Paragraph has no attrs.

Both are `builtin = true`, so the plugin view suppresses chrome — they render as plain
editable blocks, identical to today (`plugin.rs:63`).

---

## 5. Editor registry arms

`editor_for` (`crates/lopress-editor/src/ui/blocks/editor_registry.rs:35`) gains:

```rust
"paragraph" => Some(paragraph_editor_widget),
"heading"   => Some(heading_editor_widget),
```

`render_paragraph_editable` and `render_heading_editable` are reshaped to the
`EditorWidget` signature (`fn(&EditorContext) -> AnyView`), pulling runs from
`ctx.block.body` and (for heading) `level` from `ctx.block.plugin.attrs["level"]` — not
from a `BlockKind`. This mirrors `list_editor_widget`/`code_editor_widget` exactly
(`editor_registry.rs:50-111`).

---

## 6. Conversion path

`block_from_core` (`from_core.rs`): delete the `"paragraph"` and `"heading"` arms
(`from_core.rs:33-46`). Both now flow through the native-registry path
(`from_core.rs:48`, `native_block_from_core`), which already dispatches on the editor
key — add `Some("paragraph")` and `Some("heading")` arms to `native_block_from_core`
(`from_core.rs:154-166`) that parse inline runs (and, for heading, `level` from attrs).

`block_to_core` (`to_core.rs`): delete the non-plugin built-in match
(`to_core.rs:38-73`) once no block reaches it. Paragraph/heading serialize via the
native arm (`native_block_to_core`, `to_core.rs:78`) — add `paragraph`/`heading` body
cases there, emitting `r#type: "paragraph"` / `"heading"` core blocks with the inline
text and (heading) the `level` attr.

After this, **every** block has `plugin: Some(_)`. The `plugin: None` branches in
`block_to_core` (`to_core.rs:38`) and `block_view` (`mod.rs:84`) become dead and are
removed.

---

## 7. Retiring `BlockKind`

This is the heart of the spec. `BlockKind`
(`crates/lopress-editor/src/model/types.rs:44-52`) is consumed in these roles; each is
replaced:

### 7.1 Block identity / dispatch
**Today:** `block_view` and `render_body` match `(kind, body)`.
**After:** dispatch is `editor_for(meta.editor)` keyed on the plugin's editor key, with
the body-shape (a `BlockBody` variant) as the secondary discriminant. The descriptor
table (§10, separate spec) supplies the editor key → body-shape mapping for validation.

### 7.2 Heading level / code lang / list ordered
**Today:** carried in `BlockKind::Heading(u8)` / `Code { lang }` / `List { ordered }`,
mirrored into attrs.
**After:** the **only** copy lives in `PluginMeta.attrs` (`level`, `lang`, `ordered`).
The mirror-sync code in `apply_edit_attrs` (`actions.rs:202-219`) is deleted — there is
no second copy to keep in sync.

### 7.3 `apply` action dispatch
**Today:** `apply_split` matches `kind` to choose the tail block's kind
(`actions.rs:288-294`); `apply_change_type` takes a `new_kind: BlockKind` and runs a
`(new_kind, body)` conversion matrix (`actions.rs:530-618`).
**After:**
- `apply_split` produces the tail block by **cloning the source block's `PluginMeta`**
  (editor key + attrs) and giving it an empty body of the same shape — no `BlockKind`
  needed. A heading split yields a heading; a paragraph split yields a paragraph;
  both fall out of "same plugin identity, split body."
- `BlockAction::ChangeType` changes from `new_kind: BlockKind` to
  `new_editor: Rc<str>` (the target editor key) — see §8.

### 7.4 `coerce_body_to_kind` / `body_matches_kind`
**Today:** keyed on `BlockKind` (`actions.rs:859-951`).
**After:** rename to `coerce_body_to_editor` / `body_matches_editor`, keyed on the
editor key string. The body-shape each key expects comes from the descriptor table
(§10). Logic is otherwise identical (the flatten/convert arms are unchanged).

### 7.5 The enum itself
Delete `BlockKind` and `EditorBlock.kind`. `EditorBlock` becomes:

```rust
pub struct EditorBlock {
    pub id: BlockId,
    pub body: BlockBody,
    pub plugin: PluginMeta,   // no longer Option — every block has identity
}
```

`PluginMeta` is no longer `Option` because every block now carries it. This is a large
but mechanical change; the `block.plugin.as_ref()` / `if let Some(meta)` sites
(dozens) collapse to direct field access. `Opaque` blocks still get a `PluginMeta` whose
`editor`/`native` are `None` and whose `block_type_name` is the unknown type — they
already do (`from_core.rs:53`), except today they take `plugin: None`; they gain an
identity meta so the field can be non-optional.

---

## 8. `ChangeType` becomes `new_editor`

`BlockAction::ChangeType { block_id, new_kind: BlockKind }`
(`actions.rs:44-47`) becomes:

```rust
ChangeType {
    block_id: BlockId,
    new_editor: Rc<str>,   // target editor key: "paragraph" | "heading" | "code" | "list"
    attrs: serde_json::Map<String, Value>,  // e.g. {"level": 2} for heading, {"ordered": true} for list
}
```

The toolbar (`crates/lopress-editor/src/ui/toolbar.rs`) emits `new_editor` +
`attrs` instead of constructing a `BlockKind`. `apply_change_type` swaps the block's
`PluginMeta` to the canonical meta for `new_editor` (via the descriptor table's default
constructor) and coerces the body to that editor's body shape. The inverse records the
old editor + old attrs — making `ChangeType` fully reversible (it currently restores
only `kind`, not `body`; see the `NOTE` at `actions.rs:619-626`). This closes the
long-standing lossy-undo gap as a side effect.

---

## 9. Testing

The round-trip suite (`crates/lopress-editor/tests/from_to_core_tests.rs`) is the
primary safety net and must stay byte-identical green at every stage. Specific
additions:

- **Paragraph/heading round-trip** through the native registry path (new cases in
  `from_to_core_tests.rs`): a doc of mixed paragraphs and H1–H6 must round-trip
  identically after the migration as before.
- **Heading `level` in attrs**: a unit test asserting `level` is read from
  `attrs["level"]` and survives `EditAttrs`.
- **`ChangeType` reversibility** (`undo_tests.rs`): para→code→undo restores the
  *body*, not just the type — the new behavior from §8.
- **`split` preserves plugin identity**: splitting a heading yields a heading tail;
  splitting a list item yields a list item (existing tests, re-pointed off `BlockKind`).
- **`size_tests::block_action_size_is_compact`** (`actions.rs:1008`) — re-check the
  40-byte guard after `ChangeType` grows an attrs map (box it if needed, like
  `EditFrontMatter`).
- **Base plugins loaded in every test context** that renders or converts paragraph/
  heading blocks — the same requirement the 05-17 spec imposed for list
  (§12 there). Audit `tests/` for bare `PluginRegistry` construction.

Final e2e via the debug control server (`127.0.0.1:7878`, the `driving-lopress-editor`
capability): open a doc with paragraphs and headings, edit text, split (Enter) and
merge (Backspace), convert types via the toolbar, save, confirm byte-identical
round-trip.

---

## 10. Sequencing and relationship to the descriptor-table spec

This spec and `2026-06-04-block-descriptor-table-design.md` are mutually reinforcing.
Recommended build order:

1. **Stage A — migrate paragraph/heading** (§4–§6). After this, all blocks carry
   `PluginMeta` but `BlockKind` still exists (still `Some`-wrapped). Lower-risk,
   independently shippable, round-trip green.
2. **Descriptor table** (separate spec). Becomes the single source for editor-key →
   body-shape and default constructors — the data §7.4 and §8 read from.
3. **Stage B — retire `BlockKind`** (§7–§8). Build the body-shape/dispatch off the
   descriptor table; delete the enum; make `PluginMeta` non-optional.

Stage A is valuable on its own even if Stage B is deferred — it removes two of the four
hardcoded dispatch arms. Stage B should not start before the descriptor table lands, or
the editor-key → body-shape knowledge gets re-scattered.

When this spec produces implementation plans, expect **at least two staged plans**
(Stage A, Stage B), in the `stageN-*` style of the list-unification effort.

---

## 11. Non-goals

- No markdown-on-disk change; documents round-trip byte-identically.
- No new user-facing behavior (the `ChangeType` undo fix is a correctness improvement,
  not a feature).
- No custom JS/WASM editors.
- The descriptor table's design is a separate spec; this one only consumes it.

---

## 12. Decisions

### Retire `BlockKind` fully rather than keep it as an internal signal
Chosen over keeping it as a body-shape/dispatch signal. Keeping it perpetuates the
duality and the mirror invariants this whole effort targets. With paragraph/heading
migrated and a descriptor table supplying body-shape, `BlockKind` has no remaining
unique responsibility. Cost: a large mechanical refactor (`PluginMeta` non-optional,
`ChangeType` re-keyed). Benefit: one identity, one dispatch, zero mirrors.

### `PluginMeta` becomes non-optional on `EditorBlock`
Once every block has identity, `Option<PluginMeta>` is a lie the type system shouldn't
tell. Direct field access removes dozens of `if let Some(meta)` ladders. Opaque blocks
carry an identity meta with `editor: None`.

### `level`/`lang`/`ordered` live only in `attrs`
Single source of truth. The attr form and serializer already read from attrs; the enum
copy was the redundant one.

### `ChangeType` carries `new_editor: Rc<str>` + `attrs`
Re-keys type conversion onto the plugin identity and, by snapshotting the resulting
body, makes the action fully reversible — fixing the lossy-undo `NOTE` at
`actions.rs:619`.

### Two staged plans, descriptor table in between
Stage A (migrate) is shippable alone and low-risk; Stage B (retire) needs the
descriptor table first. Sequencing the risk mirrors the list-unification stages.

---

## 13. Open questions for the planner

- **`ChangeType` attrs default**: when converting *to* heading, what default `level`?
  Proposal: 2 (the common case; toolbar offers explicit H1–H6 anyway). Confirm during
  planning.
- **`Opaque` identity meta shape**: confirm the exact `PluginMeta` for an unknown-type
  block (`block_type_name` = the unknown type, `editor`/`native` = `None`,
  `attr_decls` empty, `builtin` false) round-trips through `to_core`'s opaque arm
  unchanged.

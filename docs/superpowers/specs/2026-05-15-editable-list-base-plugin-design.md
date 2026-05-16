# Editable List Items — List as a Base Plugin

**Date:** 2026-05-15
**Status:** approved, deferred
**Prerequisite:** `2026-05-15-editor-assessment-fixes-design.md` fully implemented

Lists are currently read-only in the editor. This spec covers making list items
fully editable and, as part of that work, migrating the list block to the plugin
infrastructure — dogfooding the plugin approach with a built-in plugin that
ships as part of the core codebase.

This work is intentionally deferred until after the nine assessment fixes are
complete. Lists remain read-only in the interim.

---

## 1. Base plugin infrastructure

A `base_plugins/` directory lives at the project root alongside `src/` and
`crates/`. Each base plugin is a subdirectory with a `manifest.toml` following
the existing plugin manifest format. These manifests are embedded at compile
time via `include_str!` and loaded into `PluginRegistry` at startup before user
plugins — making them non-removable while dogfooding the plugin approach.

`lopress-plugin`'s manifest loading gains a `from_str` path (alongside the
existing `from_path`). `PluginRegistry::load_base_plugins()` is called first in
the startup sequence.

This sets the pattern for future built-ins. Paragraph, Heading, and Code can
follow the same base plugin registration path when the time comes — no further
architectural work is needed to enable that.

---

## 2. List base plugin manifest

```
base_plugins/
  list/
    manifest.toml
```

```toml
# base_plugins/list/manifest.toml
name    = "lopress:list"
editor  = "list"
builtin = true        # suppresses header strip and attr form chrome

[[attrs]]
name = "ordered"
type = "bool"
ui   = "hidden"
```

---

## 3. Model wiring

`from_core` already populates `PluginMeta` for any block whose type name is in
the registry. Since `lopress:list` is registered, markdown list blocks get:

```rust
plugin: Some(PluginMeta {
    block_type_name: "lopress:list",
    attrs: { "ordered": false/true },
    attr_decls: [AttrDecl { name: "ordered", type: "bool", ui: "hidden" }],
})
```

`block_view`'s `block.plugin.is_some()` check routes list blocks through
`plugin_block_view` — no special `BlockKind::List` arm in the built-in
dispatch. `BlockKind::List { ordered }` stays in the model for action dispatch
and serialization; `ordered` is mirrored in `plugin.attrs` at load/save time.
This is the intentional Level C seam — when the rest of the built-ins follow
this pattern, `BlockKind::List` can be retired.

---

## 4. Plugin block view changes

`plugin_block_view` gains `editor = "list"` handling. When `builtin = true`,
chrome (header strip, attr form) is suppressed. `ordered` is read from
`plugin.attrs["ordered"]`.

---

## 5. New `BlockAction` variants

```rust
SplitListItem         { block_id: BlockId, item_id: BlockId, byte_offset: usize }
MergeListItemWithPrev { block_id: BlockId, item_id: BlockId }
```

Both go through `apply()`. Once the undo system from the prerequisite spec is
in place, these actions receive undo coverage using the same inverse-at-apply
pattern. Inverses:

| Action | Inverse |
|---|---|
| `SplitListItem` | `MergeListItemWithPrev` (and vice versa) |

---

## 6. Fix `apply_split` for lists

Currently falls through to `_ => {}`. New behaviour: treat the flat text as
items joined by `\n` (matching the ctrl serializer). Walk cumulative byte
offsets to find the item containing `byte_offset`, split its runs there, insert
a new `ListItem` after it. Keeps the ctrl API's `Split` command working on lists.

---

## 7. Editable list view

The canonical implementation of `editor = "list"`. Function signature mirrors
what a plugin editor receives: `(items, block_id, ordered, on_action,
focus_target, focus_pub, current_doc)` — no implicit coupling to built-in
dispatch. Each `ListItem` gets a `BlockEditorState` from `build_block_editor`.
The view is a `v_stack` of `[bullet/number] [inline_editor_item]` rows.

Per-item key handler:

| Key | Action |
|---|---|
| Enter | `SplitListItem { block_id, item_id, byte_offset }` |
| Backspace at offset 0, item N > 0 | `MergeListItemWithPrev { block_id, item_id }` |
| Backspace at offset 0, item 0 | `commit_from_editor` + `MergeWithPrev { block_id }` |
| ↑ first line of item 0 | `commit_from_editor` + jump to previous block via `focus_target` |
| ↓ last line of last item | `commit_from_editor` + jump to next block via `focus_target` |
| ↑ / ↓ within list | `commit_from_editor` on current item + move focus between items |

The list view tracks focused item via a `RwSignal<Option<BlockId>>` owned by
the list view. It publishes to `FocusPublisher` when any item is focused, so
the block toolbar appears above the list block as a whole.

---

## Implementation order

1. `lopress-plugin`: add `from_str` loading path; add `builtin` field to `BlockDecl`
2. Base plugin infrastructure: `base_plugins/list/manifest.toml`, `load_base_plugins()`, `include_str!` wiring
3. Model wiring: `from_core` / `to_core` updates for `lopress:list`; `BlockKind::List` mirroring
4. New `BlockAction` variants + `apply` implementations (`SplitListItem`, `MergeListItemWithPrev`, `apply_split` list fix)
5. Undo inverse entries for new action variants
6. `plugin_block_view`: `editor = "list"` dispatch + `builtin` chrome suppression
7. Editable list view: `build_block_editor` per item, per-item key handler, `FocusPublisher` wiring
8. Remove `BlockKind::List` match arm from `block_view` built-in dispatch

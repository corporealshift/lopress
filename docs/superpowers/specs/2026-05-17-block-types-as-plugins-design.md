# Block Types as Plugins — Plugin Capability Model & List Migration

**Date:** 2026-05-17
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/ideas/2026-05-17-block-types-as-plugins.md` (the idea this refines), `docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md` (the partial migration this builds on)

---

## 1. Background

The `feat/editable-list-plugin` branch shipped "list as a base plugin," but it is a partial migration: list routing through the plugin layer is a detour that converges back on hardcoded built-in code. `from_core` has a hardcoded `"list" =>` arm, `to_core` explicitly skips the plugin path for `BlockKind::List`, and `render_body` dispatches on the Rust `BlockKind` enum rather than the manifest. The manifest's `editor` field is decorative — every field of a list block's `PluginMeta` is either suppressed or shadowed by `BlockKind::List`. The plugin layer registers and tolerates a built-in block but does not control it.

This spec finishes the job for list: it defines a real plugin capability model and builds the machinery so that list is genuinely driven by its manifest, while keeping paragraph, heading, and code on their existing hardcoded paths for now (they migrate in follow-up specs using the same machinery).

---

## 2. Scope

- Build the general, reusable machinery (a plugin capability model, an editor registry, and a registry-driven `from_core` / `to_core` conversion path).
- Migrate **only** the list block onto this machinery end-to-end. Paragraph, heading, and code keep their existing hardcoded arms in `block_view` / `from_core` / `to_core` — they migrate later, one at a time, using this same machinery.
- `BlockKind` is **not** retired. It stays. `BlockKind::List { ordered }` is still constructed and retained for action dispatch (`apply`) and as the body-shape signal. Paragraph, heading, and code still depend on it.
- The chosen implementation approach is **"generic registry path with hardcoded fallback"**: `from_core` / `to_core` gain a generic registry-driven path; any block type with a registry entry flows through it, types without one fall back to existing hardcoded arms. List's hardcoded arms are deleted; list rides the registry.

---

## 3. Plugin Capability Model

A plugin can do up to three things, declared per-block in `manifest.toml`. New fields are added to the existing `BlockDecl` struct in `crates/lopress-plugin/src/manifest.rs`.

### Capability #1 — Edit

A block declares `editor = "<key>"`, a built-in editor key (`"list"`, `"paragraph"`, `"heading"`, `"code"`). When omitted, the block gets the existing generic attr-form editor (header strip + attr form + body editor). Custom compiled editor widgets are built-in-only — a user plugin (manifest only, no compiled code) can only pick an existing built-in editor key or omit `editor`. Custom JS/WASM editor UIs remain a deferred escape hatch, out of scope.

### Capability #2 — Transform (on-disk form)

A block declares an **optional** `native = "<core_type>"` field. There is **no** `serialize` field and **no** enum.

- `native` **present** → this block **is** a native markdown construct identified by `<core_type>` (e.g. `native = "list"`). It serializes as bare native markdown (`- item`) with no comment wrapper, round-trips as that `lopress_core` Block type, and the standard markdown→HTML renderer produces build output. `native` is an **exclusive claim**: exactly one plugin may claim a given core type. The plugin registry enforces this — a duplicate `native` claim is a load error, mirroring the existing `DuplicateBlock` check.
- `native` **absent** → the block uses the comment container: `<!-- lopress:x -->…<!-- /lopress:x -->` on disk, identity from the comment marker, attrs stored as JSON in the comment, HTML `template` field rendered at build time. This is the existing, unchanged behavior and the silent default. The overwhelming majority of plugins (all current user plugins) write exactly the same TOML they write today — no new fields.

Because `native` is an exclusive claim and custom editor widgets are built-in-only, native serialization is in practice built-in-only: base plugins claim the native markdown constructs (list, heading, code). A user plugin that wants a "list-like" block (e.g. a `checklist`) cannot claim the `list` core type — that slot is taken — so it uses the comment container; the comment marker disambiguates it from a plain list while its inner content stays native markdown and degrades gracefully in non-lopress tools.

### Capability #3 — Assets

A block declares **optional** `css = [...]` and `js = [...]` arrays of file paths. These are parsed into `BlockDecl` and exposed on the plugin registry. The `<head>`-injection contract (collecting css/js for inclusion in built pages) is documented in this spec as the designed seam but is **not** wired into the build in this spec — the list block needs no assets. Implementing the build-side injection is a follow-up.

---

## 4. The List Base Plugin Manifest

After this spec, `base_plugins/list/manifest.toml`'s block entry is:

```toml
[[blocks]]
name    = "list"
editor  = "list"
native  = "list"
builtin = true

[blocks.attrs]
ordered = { type = "bool", ui = "hidden" }
```

A normal user plugin manifest (e.g. a `video` block) is **unchanged** from today — `name`, `template`, `attrs`, no new fields.

---

## 5. Editor Registry (Capability #1 Machinery)

The built-in editor widgets currently have mismatched signatures. They are unified behind one shape. A new module `crates/lopress-editor/src/ui/blocks/editor_registry.rs` defines:

```rust
pub struct EditorContext<'a> {
    pub block: &'a EditorBlock,
    pub on_action: ActionSink,
    pub focus_target: RwSignal<Option<BlockId>>,
    pub focus_pub: FocusPublisher,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}
pub type EditorWidget = fn(&EditorContext) -> AnyView;
```

The "registry" is a free function `editor_for(key: &str) -> Option<EditorWidget>` — a `match` on the key string returning a function pointer. No global mutable state, no extra parameter threading. It is extensible by adding match arms. Dispatch is data-driven because the key comes from the manifest.

This spec registers **only** the `"list"` key. `editable_list_view` in `crates/lopress-editor/src/ui/blocks/list.rs` is reshaped to the `EditorWidget` signature: it pulls `items` from `ctx.block.body`, and reads `ordered` from `ctx.block.plugin.attrs["ordered"]` — the manifest-driven attr, **not** the `BlockKind::List` enum. (Reading `ordered` from the enum is exactly the shadowing this work eliminates.) Paragraph, heading, and code keep their hardcoded `block_view` arms; migrating them later means reshaping each widget to `EditorWidget` and adding one `editor_for` arm.

---

## 6. Body Shape

Body shape is **derived** from the editor key, not declared in the manifest. `editor = "list"` implies a list body; `editor = "paragraph"` / `"heading"` imply an inline body; `editor = "code"` implies a text body. No extra manifest field is needed.

---

## 7. Native Block Registry (Capability #2 Machinery)

A native block registry is built once from the existing `PluginRegistry` by scanning every `BlockDecl` that has `native` set, producing a lookup keyed on core type: `core_type -> { block name, editor key }`. `PluginRegistry::insert` in `crates/lopress-plugin/src/registry.rs` gains a duplicate-`native`-claim check that returns a load error, mirroring the existing `DuplicateBlock` error path.

---

## 8. Registry-Driven `from_core`

`block_from_core` (in `crates/lopress-editor/src/model/from_core.rs`) consults the native registry first: if the core block's `type` matches a `native` claim, it builds the `EditorBlock` — body parsed according to the editor key's implied body shape — and stamps `PluginMeta`. The list block flows through this generic path. The list-shape body parser is the existing `list_from_core` convertibility check preserved as a helper: a list is convertible only if every `list_item` child contains exactly one `paragraph` child with no further nesting; otherwise the whole list becomes `Opaque` so its structure round-trips verbatim.

The hardcoded `"list" =>` arm in `block_from_core` is deleted. Built-in paragraph/heading/code arms, the comment-plugin path (`registry.block(other)`), and the `Opaque` fallback for unknown types are all unchanged.

---

## 9. Registry-Driven `to_core`

`block_to_core` (in `crates/lopress-editor/src/model/to_core.rs`): a plugin-flagged block whose plugin type has a `native` claim is serialized to a native `lopress_core::Block` of that `core_type`, with the body serialized per its body shape. For list this emits bare `list` / `list_item` core blocks.

The current special-case `if !matches!(b.kind, BlockKind::List { .. })` skip is **replaced** by this general branch:
- plugin block with a native claim → native serialization
- plugin block without → comment-container serialization via the existing `plugin_block_to_core`
- non-plugin block → the existing built-in match arms

List no longer takes a built-in match arm.

---

## 10. `PluginMeta` Change

`PluginMeta` (in `crates/lopress-editor/src/model/types.rs`) gains an `editor: Option<String>` field, populated from the block's `BlockDecl` during `from_core`, so `render_body` can dispatch on it.

---

## 11. `render_body` and `block_view` Wiring

`render_body` in `crates/lopress-editor/src/ui/blocks/plugin.rs` drops its `match (block.kind, block.body)` and instead looks up `editor_for(meta.editor)` and invokes the returned `EditorWidget` with an `EditorContext`.

`block_view` in `crates/lopress-editor/src/ui/blocks/mod.rs` loses its built-in `(BlockKind::List { ordered }, BlockBody::List(items))` arm entirely. Lists always carry `PluginMeta` because the base plugins are always loaded at editor startup, so list blocks always take the plugin path.

---

## 12. Migration Risk — Base Plugins Must Be Loaded

Because `block_view` loses its built-in List arm, any code path that renders a list block **without** the base list plugin registered would render nothing. The real editor always loads base plugins at startup, so production is fine. The risk is in tests: tests that build a bare `PluginRegistry` and render or convert list blocks must call `load_base_plugins()`. Every test context that exercises list rendering or list `from_core` / `to_core` must load base plugins first.

---

## 13. `BlockKind::List` Retention

`BlockKind` stays. `BlockKind::List { ordered }` is still constructed by `from_core` and retained for `apply` / action dispatch and as the body-shape signal. The `ordered` value is mirrored into `PluginMeta.attrs["ordered"]`, and the list editor widget reads `ordered` from `PluginMeta.attrs`, not from the enum. This is an intentional, documented seam: when paragraph, heading, and code later migrate the same way, `BlockKind` can be reconsidered for retirement, but that is out of scope here.

---

## 14. Testing

### Round-trip safety net

The round-trip test suite `crates/lopress-editor/tests/from_to_core_tests.rs` is the primary safety net. A list document must round-trip byte-identically before and after this change. It must stay green at every step of implementation.

### New unit tests

- `editor_for` key lookup
- Building the native block registry from a `PluginRegistry`
- The duplicate-`native`-claim load error
- Manifest parsing of the new `native`, `css`, and `js` fields

### Updated existing tests

- `crates/lopress-editor/tests/list_plugin_meta_tests.rs` and `crates/lopress-editor/tests/plugin_block_tests.rs` are updated for the new `PluginMeta.editor` field.

### Test context requirement

Every test context that renders or converts list blocks must call `load_base_plugins()` on its `PluginRegistry`.

### Final end-to-end verification

After the unit/integration suite passes, verify lists work in the running editor GUI using the control interface — the debug HTTP control server on `127.0.0.1:7878` (the same interface described by the `driving-lopress-editor` capability). The e2e check opens a document containing a list, edits list item text, splits an item (Enter) and merges items (Backspace at offset 0), saves the document, and confirms the saved markdown round-trips correctly as bare native list markdown. This is an explicit, required final step of the testing plan.

---

## 15. Implementation Order

1. `crates/lopress-plugin`: add `native: Option<String>`, `css: Vec<String>`, `js: Vec<String>` fields to `BlockDecl`; add the duplicate-`native`-claim check to `PluginRegistry::insert`.
2. `base_plugins/list/manifest.toml`: add `native = "list"` to the list block entry.
3. New `editor_registry` module: define `EditorContext`, `EditorWidget`, `editor_for`; reshape `editable_list_view` to the `EditorWidget` signature.
4. `PluginMeta`: add the `editor: Option<String>` field.
5. `from_core`: add the generic native-registry path; route `list` through it; delete the hardcoded `"list" =>` arm; keep built-in/comment/opaque fallbacks.
6. `to_core`: add the native-serialization branch for native-claiming plugin blocks; route list through it; remove the `BlockKind::List` skip special-case.
7. `render_body`: dispatch via `editor_for(meta.editor)` instead of `match block.kind`.
8. `block_view`: remove the built-in `BlockKind::List` arm.
9. Tests: update existing tests for the new field, add the new unit tests, ensure all list-touching test contexts load base plugins, and add the final control-interface e2e verification.

---

## 16. Non-Goals / Scope Boundary

- **No on-disk markdown format change.** Lists stay bare `- item`; documents round-trip byte-identically. The built-in list earns bare serialization precisely because it exclusively claims `core_type = "list"`.
- **No user-facing behavior change.** Editing, rendering, and static build output are unchanged.
- **Paragraph, heading, and code are not migrated in this spec.**
- **`BlockKind` is not retired.**
- **Capability #3 (assets) build-side `<head>` injection is not implemented** — only the manifest fields and the documented contract.
- **Custom JS/WASM plugin editor UIs are out of scope.**

---

## 17. Decisions

### Scope = build machinery, migrate list only

Rejected: migrating all four block types at once (too large, big-bang `from_core` / `to_core` rewrite with no incremental fallback); rejected: a list-only special-case with no reusable machinery (contradicts the goal of finishing the plugin layer).

### `BlockKind` kept, not retired

Retiring it is impossible while paragraph, heading, and code still depend on it; retirement is reconsidered after they migrate.

### Approach A — generic registry path with hardcoded fallback

Rejected Approach B (list-specific plugin serialization — not reusable machinery) and Approach C (register all four types now — really the full target state, big-bang).

### Capability #2 expressed as a single optional `native` field, no `serialize` enum

An earlier draft used `serialize = "native" | "comment"`; rejected because it muddled every plugin manifest with a field 99% of plugins would set to the same value, and `"comment"` was opaque jargon. The comment container is now simply the silent default (absence of `native`).

### `native` is an exclusive claim, registry-enforced

This is what makes bare serialization unambiguous: exactly one plugin owns a core type, so an on-disk `- item` list has exactly one possible owner and needs no marker. List-like user plugins must use the comment container.

### Custom editor widgets are built-in-only; user plugins pick a built-in editor key or get the generic attr-form editor

A manifest-only plugin cannot ship a compiled Floem widget. Custom JS/WASM editors deferred.

### Body shape derived from the editor key, not a separate manifest field

Avoids redundant manifest data.

### Editor registry implemented as a `match`-based free function, not a global mutable map

No threading, no shared mutable state, still data-driven via the manifest-supplied key.

---

## 18. Open Questions for Claude

None. All design decisions listed above are resolved. The spec covers every section with concrete decisions and no placeholders.

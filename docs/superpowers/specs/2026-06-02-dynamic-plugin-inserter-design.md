# Dynamic Plugin-Block Inserter — Surface Registered Plugin Blocks in the Slash Menu

**Date:** 2026-06-02
**Author:** Claude (autonomous exploration, commissioned by Kyle)
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/ideas/2026-06-02-editor-parity-plugin-ideas.md` (Tier 0.1 — the unlock),
`docs/superpowers/specs/2026-06-02-template-form-blocks-design.md` (the block class this makes insertable),
`docs/superpowers/specs/2026-06-01-image-block-design.md` (base-plugin insertion precedent)

---

## 1. Background & problem

lopress can *render* and *round-trip* comment-container plugin blocks (HTML-template and the
new markdown-template-form class), but it cannot **insert** them from the GUI. The slash menu
(`crates/lopress-editor/src/ui/slash_menu.rs`) is a hardcoded `SlashChoice` enum — Paragraph,
Headings, Code, Lists, Image, Read more. A plugin block (callout, button, author-bio, …) can
only be added today by hand-editing the markdown comment delimiters.

This makes every block plugin invisible in the editor. Closing this gap is the single
highest-leverage parity feature: once the inserter lists registered plugin blocks, the whole
Tier-1 plugin family (callout, button, embed, details, pullquote, …) becomes reachable with no
further editor work.

## 2. Scope

- A **dynamic section** of the slash menu listing every *insertable* registered plugin block,
  computed from the workspace `PluginRegistry`.
- A generic editor constructor for a fresh plugin comment-container block (default attrs, empty
  body, correct `PluginMeta`) and its insertion via the existing `InsertAfter` action.
- Optional, backward-compatible manifest metadata (`title`, `description`, `category`) on
  `BlockDecl` to drive nice menu labels/grouping; sensible fallback when absent.
- **Out of scope:** fuzzy text filtering of the menu (Tier 0.2, separate), anchored/positioned
  menu placement, drag-from-palette, block categories UI beyond a flat grouped list, and any new
  block *content* (the plugins themselves are separate work).

## 3. What counts as "insertable"

From each `LoadedPlugin` in the registry, a `BlockDecl` is offered in the inserter when it is a
**comment-container plugin block** — i.e. it has a `template` OR a `markdown_template` and is
**not** `builtin` and does **not** claim a `native` core type. Rationale:

- `builtin`/`native` base plugins (list, code, image, more) are already first-class: they have
  dedicated slash entries (Image, Read more) or are reached via ChangeType. Listing them again
  as "plugin blocks" would duplicate and confuse.
- A decl with neither `template` nor `markdown_template` is an editor-only base block; it has no
  static render and is not a user-insertable content block.

So: **offer iff `(template.is_some() || markdown_template.is_some()) && !builtin && native.is_none()`.**

## 4. Data flow

```
Session::plugin_registry()  ──filter──▶  Vec<PluginInserterItem>  ──prop──▶  editor view
        (workspace plugins)                {type_name, title, category,        (ui/mod.rs)
                                            attr_decls, default_attrs}              │
                                                                                    ▼
                                                          editor_pane(…, inserter_items)
                                                                                    │
                                                                                    ▼
                                                    slash_menu(items = builtins + plugins)
                                                                                    │
                                              select Plugin(type_name) ─────────────┘
                                                                                    ▼
                                       InsertAfter { anchor, EditorBlock::from_plugin_item(item) }
```

`PluginInserterItem` is computed **once when the document/workspace view is built** (the registry
is stable for a loaded workspace; `Session::plugin_registry()` recomputes on call, so compute it
at the editing-view boundary and pass an `Rc<[PluginInserterItem]>` down, mirroring how
`on_undo`/`on_redo`/`on_insert_image` are threaded).

## 5. Manifest additions (optional, backward-compatible)

Three optional fields on `BlockDecl` (`crates/lopress-plugin/src/manifest.rs`), all
`#[serde(default)]` — exactly the pattern used by `markdown_template`/`label`/`help`:

- `title: Option<String>` — the inserter menu label. Fallback when absent: derive from
  `name` by stripping a `lopress:` prefix and title-casing (`lopress:author-bio` → "Author bio").
- `description: Option<String>` — secondary line in the menu (optional, may be unused in MVP UI).
- `category: Option<String>` — grouping bucket (e.g. "Text", "Media", "Embeds", "Layout").
  Fallback: "Blocks". The menu groups plugin items by category under labeled subheaders, after
  the built-in entries.

These are additive; existing manifests parse unchanged.

## 6. Editor changes

### 6.1 `SlashChoice` (slash_menu.rs)

Add a variant:

```rust
SlashChoice::Plugin { type_name: Rc<str> }
```

`slash_menu_items()` stays the built-in list. The dynamic plugin items are appended by the
caller (editor_pane), which has the `PluginInserterItem` list — so `slash_menu` itself stays
data-driven and the registry dependency does not leak into the menu widget.

### 6.2 Generic plugin-block constructor (model)

Add `EditorBlock::from_plugin_item(item: &PluginInserterItem) -> EditorBlock`, analogous to
`EditorBlock::read_more()`. It builds a comment-container block:

- `kind = BlockKind::Opaque { type_name: item.type_name.clone() }`
- `body = BlockBody::Opaque(Value::Null)` (empty body; comment container)
- `plugin = Some(PluginMeta { block_type_name: item.type_name, attrs: item.default_attrs.clone(),
  attr_decls: item.attr_decls.clone(), builtin: false, editor: None, native: None, … })`

`default_attrs` is built from the decl's `attrs`: for each `AttrDecl` with a `default`, seed that
value; otherwise omit (the attr form fills it in). This guarantees the freshly inserted block
round-trips and renders immediately (empty fields render empty, exactly like the template-form e2e).

### 6.3 Wiring (editor_pane.rs / ui/mod.rs)

- `editor_pane` gains one parameter: `inserter_items: Rc<[PluginInserterItem]>`.
- `ui/mod.rs` computes the list from the active `Session`'s registry when building the editing
  view and passes it through (same threading as `on_insert_image`).
- In the slash overlay, append a `SlashChoice::Plugin { type_name }` row per item (grouped by
  category) to the built-in `items`, then handle selection:

```rust
SlashChoice::Plugin { type_name } => {
    if let Some(item) = inserter_items.iter().find(|i| i.type_name == type_name) {
        on_action(BlockAction::InsertAfter {
            anchor: block_id,
            new_block: Box::new(EditorBlock::from_plugin_item(item)),
        });
    }
}
```

## 7. Authoring/UX

- The slash menu shows built-in kinds first, then plugin blocks grouped by category subheader.
- A freshly inserted plugin block appears as the standard plugin block view (header strip + attr
  form) — the user fills the form; the live-preview webview shows the rendered output (proven by
  the template-form e2e).
- Empty/required fields are not enforced at insert time (consistent with current attr-form
  behavior); validation is a separate concern.

## 8. Testing

### Manifest
- `title`/`description`/`category` parse; default to `None`; existing manifests unaffected.

### Inserter list computation
- Given a registry with: an HTML-template block, a markdown-template-form block, a `builtin`
  base plugin, and a `native`-claiming block — the computed `Vec<PluginInserterItem>` contains
  exactly the two comment-container blocks, with titles derived/overridden correctly.

### Constructor
- `EditorBlock::from_plugin_item` yields an `Opaque` block with empty body and a `PluginMeta`
  whose attrs equal the decl defaults and whose `attr_decls` match; it `to_core`-serializes to
  a `<!-- lopress:NAME {defaults} -->\n<!-- /lopress:NAME -->` comment container and round-trips.

### End-to-end (control server, the `driving-lopress-editor` capability)
- Scaffold a workspace (`lopress new`) with a callout plugin in `plugins/`, open a post, trigger
  the slash menu, confirm a "Callout" entry appears, select it, and verify via `/state` +
  `/screenshot` that a callout plugin block was inserted with its attr form; save and confirm the
  live preview renders the callout.

## 9. Implementation order

1. `lopress-plugin`: add `title`/`description`/`category` to `BlockDecl` (`#[serde(default)]`).
2. `lopress-editor` model: `PluginInserterItem` type + `EditorBlock::from_plugin_item` + the
   registry→items filter/compute function (unit-tested).
3. `lopress-editor` UI: `SlashChoice::Plugin`, thread `inserter_items` through
   `ui/mod.rs → editor_pane`, append+group plugin rows, handle selection.
4. Tests: manifest parse, list computation, constructor/round-trip, e2e via control server.

## 10. Decisions

- **Filter by capability, not allowlist.** Insertable = comment-container (`template` or
  `markdown_template`, not builtin/native). Keeps base plugins out of the duplicate listing and
  auto-includes any future plugin. Rejected: a hand-maintained list of insertable names.
- **Compute items at the view boundary, pass an `Rc<[…]>` down.** The registry is stable per
  loaded workspace; recomputing per keystroke is wasteful and leaks the registry into the menu
  widget. Rejected: querying the registry inside `slash_menu`.
- **Optional manifest metadata with derived fallback.** A plugin needs zero new fields to appear
  (title derived from `name`); `title`/`category` just make it nicer. Rejected: requiring
  `title`/`category` (would break existing plugins and the author-bio fixture).
- **Reuse `InsertAfter` + `Opaque` block.** No new action or block kind; the inserter produces
  the same block shape the parser already yields for a hand-authored comment container. Rejected:
  a bespoke `InsertPlugin` action.
- **Values/defaults only at insert, empty body.** Mirrors template-form persistence; the block
  renders immediately and the form drives subsequent edits.

## 11. Non-goals

- Fuzzy/text filtering of the menu (Tier 0.2).
- Positioned/anchored menu placement; the centered overlay stays.
- Surfacing base/native blocks as "plugins" (they keep their dedicated entries).
- The dynamic-query / latest-posts feature (separate deferred design).
- Shipping the plugins themselves (callout/button/…) — separate work; this spec only makes
  whatever is registered insertable.

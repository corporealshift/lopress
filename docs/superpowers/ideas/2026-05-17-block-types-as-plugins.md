# Block Types as Plugins — Make the Plugin Layer the Real Control Mechanism

**Date:** 2026-05-17
**Status:** idea — to brainstorm
**Related:** `docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md` (the partial migration this builds on)

## The idea in one sentence

Make the plugin infrastructure genuinely *drive* how block types map between
markdown and the editor — so a block type is defined by its manifest, not by a
hardcoded `BlockKind` variant and hardcoded `from_core` / `to_core` arms.

## Why this doc exists

The `feat/editable-list-plugin` branch shipped "list as a base plugin." It
works, and it should stay. But it is a *partial* migration, and the partialness
is confusing: it looks like the list plugin controls list behaviour, and it
does not.

This doc names the gap and defines the target so we can iterate toward it. The
branch does not need to change for this — this is the next layer.

## Current state (what the branch actually delivered)

A markdown list flows through the editor like this today:

1. `from_core` has a **hardcoded** `"list" => list_from_core(...)` arm. It
   produces `BlockKind::List` / `BlockBody::List`, then *additionally* stamps
   `PluginMeta { block_type_name: "list", builtin: true, ... }` on top by
   looking up the registered base plugin.
2. `block_view` sees `block.plugin.is_some()` and routes through
   `plugin_block_view`.
3. `plugin_block_view` sees `meta.builtin`, suppresses the header strip and
   attr form, and calls `render_body`.
4. `render_body` dispatches on `block.kind` (the Rust enum) — **not** on the
   manifest's `editor = "list"` field — and reaches `editable_list_view`.
5. `to_core` **explicitly skips** the plugin path for `BlockKind::List`; lists
   serialize through the normal built-in match arm.

Net effect: the routing is a detour that converges back on the same built-in
code. Deleting `base_plugins/list/` entirely would leave list editing
**functionally identical** — `block_view`'s built-in `List` arm already routes
to `editable_list_view`, and `to_core` already ignores plugin meta for lists.

Every field of a list block's `PluginMeta` today is either suppressed
(`builtin` hides the chrome) or shadowed by `BlockKind::List` (`ordered` is read
from the enum, not `meta.attrs`). The manifest's `editor` field is decorative.

So the plugin layer for built-ins currently **registers and tolerates** a
built-in block — proof the wiring *can* carry one — but does not **control**
anything. The spec called this the deliberate "Level C seam."

## Target state

A block type is defined by a manifest entry. The core does not special-case it.

- The editor model stops carrying a hardcoded `BlockKind` per block. A block
  carries its **type identity** (the registered block name) plus its attrs and
  body. `BlockKind` is retired, or shrinks to a thin body-shape tag.
- `from_core` / `to_core` become **registry-driven** — no hardcoded
  `"list" => …` / `"heading" => …` arms. The conversion for a block type is
  derived from its manifest.
- Editor dispatch keys on the manifest's `editor` field (`"list"`, `"paragraph"`,
  `"heading"`, `"code"`), looked up in an **editor registry** mapping the
  `editor` string to a widget constructor.
- Built-ins (paragraph, heading, code, list) ship as **base plugins** the same
  way list does now — but the manifest genuinely drives their mapping and
  rendering, so they stop being special.

After this, "add a new first-class block type" is: write a manifest entry,
register an editor widget against an `editor` key. No `BlockKind` variant, no
`from_core`/`to_core` arm.

## The gap — what has to change

1. **Dispatch on `editor`, not `block.kind`.** `render_body` and `block_view`
   currently match the Rust enum. They need to look up `meta.editor` (or the
   block's registered type) in an editor registry.
2. **An editor registry.** Today the built-in editor widgets
   (`render_paragraph_editable`, `render_heading_editable`, `render_code`,
   `editable_list_view`) are hardcoded functions. Need a map
   `editor_key -> constructor`, seeded with the built-in widgets, so dispatch
   is data-driven.
3. **Registry-driven `from_core` / `to_core`.** Replace the hardcoded type arms
   with a generic path that consults the registry for every block type. The
   built-in types become registry entries.
4. **Model change.** Decide what replaces `BlockKind`. Options: retire it
   entirely (block carries a type-name string + attrs), or keep a thin
   `BodyShape { Inline | Text | List | Opaque }` tag while the *type identity*
   moves to the plugin name.
5. **Serialization expression.** Built-ins serialize to *native* markdown
   (`# heading`, `- item`, fenced code); user plugins serialize to
   `<!-- lopress:x -->` comment blocks. The manifest currently cannot express
   "serialize as native markdown form X." A base-plugin manifest needs a way
   to declare its serialization form.

## Open questions for the brainstorm

- **Serialization form.** How does a base plugin declare "I serialize as a
  markdown heading" vs "as a fenced code block" vs "as a comment block"? A
  manifest enum (`serialize = "heading" | "list" | "code" | "comment"`)? Or is
  serialization inherent to the `editor` choice?
- **`BlockKind`'s fate.** Fully retired, or kept as a thin body-shape tag?
  Retiring it touches a lot (`actions.rs`, `from_core`, `to_core`, every view).
- **Body shape.** Paragraph/heading have inline bodies, code has text, list has
  items. The manifest has no notion of body shape today — `editor` implies it.
  Should body shape be explicit in the manifest?
- **Identity vs attrs.** Heading's `level` — is it an attr, or part of the type
  identity (`heading` with `level` attr vs six heading types)? Attr is cleaner.
- **`apply` / actions.** `BlockAction` and `apply` are body-shape oriented
  (`EditInline`, `EditCode`, `SplitListItem`). Do these stay body-shape-keyed
  (likely yes) while only *type identity* goes plugin-driven?
- **Migration order.** List is furthest along. Paragraph/heading/code next.
  Can they migrate one at a time, or does retiring `BlockKind` force a
  big-bang change?
- **`editor` registry ownership.** Lives in `lopress-editor` (the built-in
  widgets are Floem views). User plugins can only pick an *existing* `editor`
  key — they cannot ship a new editor widget (that is the deferred
  "custom JS editor UI" escape hatch). Confirm that constraint.

## Non-goals / scope boundary

- **No on-disk format change.** Markdown stays markdown; this is purely an
  internal-architecture refactor. A document round-trips identically before and
  after.
- **No user-facing behaviour change.** Editing, rendering, and the static build
  output stay the same. This is about *how* the editor is wired, not *what* it
  does.
- Not about custom plugin editor UIs (WASM / JS editors) — that is a separate
  deferred escape hatch.

## Payoff

- One code path for all block types instead of per-type special-casing across
  `from_core`, `to_core`, `block_view`, `render_body`, `plugin_block_view`.
- New first-class blocks become declarative.
- The list branch already proved the routing wiring carries a built-in — this
  finishes the job so the plugin layer earns its place.

## Risk

This is a broad refactor: it touches the editor model (`BlockKind`),
`from_core`/`to_core`, action dispatch, and every block view. It should be
spec'd carefully and migrated incrementally (one block type at a time if the
model change allows it). Strong round-trip test coverage is the safety net —
`from_to_core_tests.rs` and friends must stay green at every step.

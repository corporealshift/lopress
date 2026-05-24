# `ChangeType` leaves block in a kind/body mismatch when the new body shape isn't covered

**Date:** 2026-05-24
**Status:** open
**Severity:** rendering breakage (block becomes invisible) + downstream serialization corruption

## Symptom

Issuing `BlockAction::ChangeType` to convert a block whose current body shape
isn't explicitly handled by the target arm in `apply_change_type` leaves the
block in a half-converted state:

- `block.kind` is updated to the new kind.
- `block.body` keeps its old shape.
- `block.plugin` is not cleared.

The result is a `(kind, body)` discriminant pair that no rendering arm in
`crates/lopress-editor/src/ui/blocks/mod.rs::block_view`'s match recognises, so
the block renders as `empty()` — visually invisible. The document is still in
the editor model but the user has no way to interact with it.

The ctrl `/state` endpoint compounds the confusion: its block-kind dispatch
is keyed on `body` shape, not `kind` (see `crates/lopress-editor/src/ctrl/
mod.rs:197`), so a block with `{kind: Paragraph, body: Code(...)}` is
reported as `"kind": "Code"` over the wire. The on-disk save round-trip
loses the lang attr (the `BlockBody::Code` ctrl arm reads lang from
`b.kind`, falling to `String::new()` when kind no longer matches Code) and
the body bytes become whatever the still-`Code`-shaped body holds.

## Reproduction

Discovered while manually verifying Stage 2 of the code-editor work
(`docs/superpowers/specs/2026-05-23-code-editor-block-and-ui-mod-split-design.md`).

1. Create a code block (toolbar Code button, or `ChangeType` to Code from a
   paragraph).
2. Issue `ChangeType { new_kind: Paragraph }` via the ctrl server, or click
   the toolbar "P" button while the code block is focused.
3. The code block disappears from the editor pane. `/state` reports
   `{kind: "Code", lang: "", text: "..."}` — kind appears unchanged, lang
   was silently cleared.
4. Save the document. The on-disk markdown serializes the block as code
   (the `BlockBody::Code` shape drives `to_core`'s native arm), but with
   an empty lang attribute — the original `rust`/`python`/etc. is lost.

## Root cause

`crates/lopress-editor/src/actions.rs::apply_change_type` (lines 425–475)
matches `(new_kind, current_body)` against a fixed set of arms:

```rust
match (&new_kind, &block.body) {
    (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_)) => {
        block.kind = new_kind.clone();
    }
    (BlockKind::Code { lang }, BlockBody::Inline(runs)) => {
        let text: String = runs.iter().map(|r| r.text.clone()).collect();
        block.kind = BlockKind::Code { lang: lang.clone() };
        block.body = BlockBody::Code(text);
    }
    (BlockKind::List { ordered }, BlockBody::Inline(runs)) => {
        block.kind = BlockKind::List { ordered: *ordered };
        block.body = BlockBody::List(vec![ListItem { id: BlockId::new(), runs: runs.clone() }]);
        block.plugin = Some(PluginMeta::list(*ordered));
    }
    _ => {
        block.kind = new_kind.clone();
    }
}
```

The fallback arm (`_ => ...`) updates `kind` without touching `body` or
`plugin`. Every conversion *out of* a non-Inline body shape lands here:

| From kind | From body | To kind | Reached arm |
|---|---|---|---|
| Paragraph / Heading | Inline | Code | second arm — converts body |
| Paragraph / Heading | Inline | List | third arm — converts body + plugin |
| Code | Code | Paragraph | **fallback — body left as `Code(...)`** |
| Code | Code | Heading | **fallback — body left as `Code(...)`** |
| Code | Code | List | **fallback — body left as `Code(...)`** |
| Code | Code | Code (new lang) | **fallback — kind.lang updates but plugin.attrs.lang doesn't mirror** |
| List | List | Paragraph | **fallback — body left as `List(...)`** |
| List | List | Heading | **fallback — body left as `List(...)`** |
| List | List | Code | **fallback — body left as `List(...)`** |

Eight (out of twelve) conversion directions are broken. Only conversions
*into* Code or List *from* Inline are wired correctly. The
Inline→Inline cases (Paragraph↔Heading levels) also work because both kind
and body shape stay Inline.

The `block.plugin` field is similarly mishandled: converting a list (which
carries `PluginMeta::list`) to a paragraph leaves the plugin meta in place
on the now-Paragraph block, which then routes through the plugin block view
unexpectedly.

## Fix sketch

`apply_change_type` should produce a target body shape that's valid for the
new kind, deriving it from the current body when possible:

- **To Inline (Paragraph / Heading)** from any body:
  - From `Inline(runs)`: keep runs as-is (current behaviour).
  - From `Code(text)`: convert to `Inline(vec![InlineRun::plain(text)])`.
  - From `List(items)`: flatten — join each item's `runs` text with `\n`,
    wrap in a single `InlineRun::plain(...)`. (Loses item boundaries; this
    is the same lossy direction as the inverse — undo restores via the
    snapshot mechanism documented in
    `2026-05-20-list-editor-unification-and-generic-undo-design.md`
    Section 3, "Shift B".)
  - Clear `block.plugin` (no plugin chrome for raw Paragraph/Heading).

- **To Code** from any body:
  - From `Inline(runs)`: stringify runs (current behaviour).
  - From `Code(text)`: keep `text`; only the lang changes. Also mirror
    `lang` into `plugin.attrs["lang"]` if `block.plugin` is `Some`
    (Stage 1 / spec Section 2).
  - From `List(items)`: join each item's flat text with `\n`.
  - Stamp `block.plugin = Some(PluginMeta::code(lang))` so it routes
    through the plugin view — same pattern as the existing `List` arm.

- **To List** from any body:
  - From `Inline(runs)`: wrap into a single ListItem (current behaviour).
  - From `Code(text)`: split `text` on `\n`, one ListItem per line,
    `InlineRun::plain` per item.
  - From `List(items)`: keep items, update `ordered` flag only. Mirror
    `ordered` into `plugin.attrs` (already done by `PluginMeta::list`).

- **Inverse / undo**: the existing inverse only restores `kind`, not
  `body`. That's already documented as lossy (see the NOTE comment at
  line 457). Stage 3 of the list-editor-unification spec is the planned
  fix — `EditBlockBody` snapshots the body alongside `kind` so undo
  restores both. This bugfix doesn't need to expand the inverse contract
  yet; just ensure the canonical action's forward path leaves the block
  in a valid `(kind, body)` shape so the dispatch arms in `block_view`
  always find a match.

## Why this slipped through Stage 1

Stage 1 added the `lang` mirror to `apply_edit_attrs`, not to
`apply_change_type`. The lang-clearing symptom appears to be Stage 1
breaking something, but it's actually a pre-existing kind/body mismatch
hitting the ctrl serializer's lang-derived-from-kind path.

Stage 1's test coverage (`from_to_core_tests.rs`, `actions_tests.rs`)
exercises the model load/save and `EditAttrs` paths but not
`apply_change_type` for the Code↔non-Inline directions. The shape of those
tests is the template for a fix here.

## Test coverage to add

For every (from_kind, from_body, to_kind) triple in the table above:

- Build a block with the from state.
- Apply `ChangeType { new_kind: to_kind }`.
- Assert: `block.kind` is `to_kind`. `block.body` matches the shape `to_kind`
  expects. `block.plugin` is correct (None for Paragraph/Heading, Some(_)
  for List/Code).
- Round-trip via `to_core(from_core(to_core(doc_after_change_type)))` to
  confirm the body shape survives save/load without data loss.

Add these to `crates/lopress-editor/tests/actions_tests.rs` (or a new
focused test file). The list and code mirror tests already in
`from_to_core_tests.rs` are the format reference.

## Related

- The Stage 1 spec/plan handled the `EditAttrs → kind.lang` mirror but not
  the `ChangeType → kind.lang` mirror. Both should follow the same
  symmetric pattern.
- The list-editor-unification spec's "Shift B" section is the long-term
  plan for making `ChangeType` undo lossless. This bug is independent —
  even with the current lossy inverse, the forward path must leave the
  block in a valid shape.

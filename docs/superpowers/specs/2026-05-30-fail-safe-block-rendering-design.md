# Fail-safe block rendering — never render `empty()`, degrade + warn + stay recoverable

**Date:** 2026-05-30
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/ideas/2026-05-30-fail-safe-block-rendering.md` (the idea this refines), `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (open extensibility direction that makes graceful degradation the right shape of fix), `feat/editor-ui-review-fixes` (shipped model coercion `actions.rs::coerce_body_to_kind`)

---

## 1. Background

When a block reaches the renderer in a shape no render arm matches, every render path draws `empty()` — a blank, unfocusable, unselectable gap. The content is still in the model but the user cannot see, edit, retype, or delete it: invisible and unrecoverable.

The triggering case was the 2026-05-30 toolbar-Code regression: a stray `EditBlockBody{Inline}` landed on a block that had just become `Code`, producing `{kind: Code, body: Inline}`, which no arm matched. The four `empty()`-on-mismatch sites are:

- `src/ui/blocks/mod.rs:148` — the `_ => empty()` kind/body mismatch arm in `block_view`.
- `src/ui/blocks/plugin.rs:374` — the `_ => empty()` fallthrough in the plugin built-in dispatch `match`.
- `src/ui/blocks/editor_registry.rs:48` (`list_editor_widget`) and `:75` (`code_editor_widget`) — the `let ... else { empty() }` body-shape guards.

The root harm is **not** that an invalid `(kind, body)` pair can exist — it is that the renderer's response to *any* unexpected input is silent erasure. The codebase is intentionally open (block-types-as-plugins direction), so runtime render failures are inevitable and the right fix is graceful degradation + observability, not a closed-sum-type compile-time guarantee.

---

## 2. Scope

Three layers, three jobs, all shipped together. No layer makes the others redundant — each protects a different surface.

| Layer | Job | Where |
|---|---|---|
| **1. Fallback render view** | Content never disappears; block always focusable + recoverable | new `src/ui/blocks/fallback.rs`; wired into 4 `empty()` sites |
| **2. Commit-source tidy + assert** | A shape-mismatched commit stops being routine, so a `debug_assert` can catch real bugs | `src/ui/toolbar.rs`, `src/ui/blocks/inline_editor.rs`, `src/actions.rs` |
| **3. Parse-boundary routing** | Bad/unknown plugin from disk loads visibly + recoverable, never panics/vanishes | `src/model/from_core.rs` → routes through the same fallback |

The unifying primitive is one `fallback_block_view`. Layers 1 and 3 both render through it; layer 2 makes the in-memory invariant assertable.

---

## 3. Layer 1 — `fallback_block_view`

New module `src/ui/blocks/fallback.rs` exposing roughly:

```rust
pub fn fallback_block_view(
    block: &EditorBlock,
    focus_pub: &FocusPublisher,
    on_action: ActionSink,
) -> AnyView
```

### Visible content

Render `body_to_flat_text(&block.body)` in a selectable text container. `body_to_flat_text` currently lives private in `src/actions.rs:632`; promote it to a shared helper both `actions.rs` and `fallback.rs` can call.

For an `Opaque` body (whose flat text is empty), fall back to the pretty-printed JSON, the same way `src/ui/blocks/opaque.rs` does. The user never loses sight of content.

### Warning chrome

A persistent, non-dismissible inline banner/badge on the block. Exact copy:

> This block couldn't be displayed with its editor — showing its raw content. Change its type or delete it to recover.

Inline and non-blocking — never a modal or popup, because renders fire constantly and reactively and a blocking dialog would re-fire and trap the user. It self-clears because once the block renders normally the fallback view is no longer constructed.

### Focusable → recovery

An `on_event(PointerDown)` handler that sets `focus_pub.block = Some(block_id)` AND clears `focus_pub.editor_and_spans = None`.

Setting `focus_pub.block` is what mounts the toolbar slot (keyed on `focus_pub.block`), giving the user **Change Type** (re-mounts a working editor) and **Delete**.

Clearing `editor_and_spans` is essential: without it the toolbar's pre-commit (`toolbar.rs:78`) reads a *previous* block's inline editor handles and would fire that stale text against this block.

The fallback is **read-only** — no in-place editing, because the body shape is ambiguous and "commit as the current shape" would risk a fresh mismatch. Recovery is via the toolbar only.

---

## 4. Layer 1 wiring — the four `empty()`-on-mismatch sites

All route to `fallback_block_view` instead of `empty()`:

- `src/ui/blocks/mod.rs:148` — the `_ => empty()` kind/body mismatch arm in `block_view`.
- `src/ui/blocks/plugin.rs:374` — the `_ => empty()` fallthrough in the plugin built-in dispatch `match`.
- `src/ui/blocks/editor_registry.rs:48` (`list_editor_widget`) and `:75` (`code_editor_widget`) — the `let ... else { empty() }` body-shape guards.
- The `editor_for(key) == None` case (a manifest `editor` key naming no registered widget) must also route to the fallback — decision 5's "fold every dead-end into one fallback view."

One path, all dead-ends: kind/body mismatch, unknown body shape, missing editor widget.

In debug builds, each site logs the offending `(kind, body, plugin.editor)` before rendering the fallback. No `debug_assert!` or panic at these *render* sites — render must always degrade (asserts live only at the model chokepoint, layer 2, and never on plugin-originated input).

---

## 5. Layer 3 — parse-boundary robustness

`from_core` (`src/model/from_core.rs`) already materializes unknown types as `Opaque` (verbatim round-trip). This layer makes that guarantee total and explicitly routed.

Any block that cannot be classified at load — unknown `native` type, unregistered plugin, malformed attrs — must load into a recoverable block surfaced through the same `fallback_block_view`, never a panic and never a dropped block. A document authored against a plugin you don't have still opens, shows its raw content, warns, and stays deletable/type-changeable.

Concretely this is a hardening + test pass over the existing `Opaque` path:

- Ensure no `unwrap`, `expect`, or `panic` on disk-sourced data in `from_core`.
- Confirm the resulting block renders via the fallback.
- Route every unclassifiable block through the fallback view, not a silent drop.

---

## 6. Layer 2 — tidy the two commit sources, then assert

The model coercion already shipped (`src/actions.rs::coerce_body_to_kind:656`, applied at `apply_edit_block_body:697`) and preserves text by converting a mismatched body to the kind's shape. Rejection of mismatched commits is ruled out — those commits carry real, possibly-uncommitted content, so rejecting them is silent data loss. Coercion stays. The work is to make a shape-mismatched commit genuinely abnormal so an assert is meaningful, and to fix the ordering that caused the regression.

### 6.1 Toolbar pre-commit — commit the actual shape

`src/ui/toolbar.rs:78`. Today the Change-Type button unconditionally pre-commits an `Inline` body before emitting `ChangeType`, regardless of the focused block's kind. Change it to pre-commit the block's *actual* body shape.

The toolbar already receives `current_kind`, so branch on it; for non-inline kinds (Code/List) there is no inline `editor_and_spans` to read, so the pre-commit simply becomes a no-op rather than a wrong-shaped `Inline`.

### 6.2 FocusLost suppression

`src/ui/blocks/inline_editor.rs:286`, which calls `commit_from_editor:658`. The focus-lost commit closure currently captures only `editor_sig`, `spans_sig`, `block_id`, and `on_action` and always emits `EditBlockBody{Inline}`. Give it access to `current_doc` so it can skip committing when the block's kind is no longer inline-bodied (Paragraph/Heading) — i.e. a `ChangeType` swapped the kind under it. This suppresses the exact stray commit behind the regression.

### 6.3 `debug_assert` + keep coercion

`src/actions.rs:697`. Coercion stays (preserves text in release builds, which always degrade and never panic). Add a `debug_assert!` that fires when an `EditBlockBody` arrives shape-mismatched, so CI catches a real internal regression.

**Provenance gate (decided).** The assert must catch *our* bugs but must never fire on plugin-originated input — a misbehaving third-party plugin editor must always degrade gracefully, never crash the editor.

Gating on `block.plugin.is_none()` is WRONG: code blocks are already `plugin: Some(_)` today (flagged by `from_core`), and the original regression WAS a Code block, so that gate would skip exactly the case we care about.

Instead, gate on whether the committing editor is a **built-in widget** (built-in / registry dispatch) versus a **future third-party plugin editor**. No third-party editor-commit path exists yet, so the assert is effectively always-on today (it catches the Code-regression class) and will automatically go silent for genuinely third-party editors once that path is built.

Release builds always coerce + degrade regardless of build profile.

---

## 7. Non-Goals / Scope Boundary

- **No fix for the deeper event-ordering / reactive-granularity root cause.** `ChangeType` calls `current_doc.update()`, which rebuilds the whole editor pane, unmounting the old inline editor, firing `FocusLost`, emitting the stray commit. The layer-2 tidy makes this specific consequence safe and fixes the specific stray commit, but the broader full-document `RwSignal<Option<EditorDoc>>` + full-pane-rebuild-per-edit model is a separate, larger investigation explicitly out of scope.
- **No in-place editing of the fallback.** Read-only by decision; editable plain-text is a clean future follow-up.
- **No closed-sum-type `BlockKind`+`BlockBody` refactor.** Considered and rejected — see idea doc "Alternatives considered."

---

## 8. Testing

All user-visible behavior is verified end-to-end through the debug control server (`ctrl/`, on 127.0.0.1:7878; `/open`, `/action` replay, `/state`, `/screenshot`) and the `driving-lopress-editor` skill.

### Layer 1 — fallback renders content

Drive a block into a mismatch (replay the regression sequence — paragraph → toolbar Code with a trailing stale `EditBlockBody{Inline}`) and assert via `/state` that the fallback renders the block's text and is focusable (the toolbar mounts on it), rather than producing an empty view. Change Type recovers it.

### Layer 1 — dead-end parity

A plugin block whose `editor` key resolves to no widget renders the fallback, not `empty()`.

### Layer 3 — unknown plugin loads visibly

Open a document referencing an unknown/unregistered plugin type and assert via `/state`/`/screenshot` that it loads as a recoverable fallback block (text visible, focusable) and the load does not panic.

### Layer 2 — ordering fix

Replay the regression and confirm via `/state` the block ends as `{kind: Code, body: Code}` (coercion + suppression), with no leftover empty/mismatched block.

### Caveat — the `debug_assert` is a CI tripwire

A `debug_assert!` firing is a debug-build panic in CI and cannot be observed cleanly through the control server. The assert's *negative* guarantee (that tidied built-in commits no longer trip it) is covered by the layer-2 ordering test above. The assert itself is a CI tripwire for future internal regressions rather than something the e2e suite asserts on directly.

---

## 9. Surface Area

### New

- `src/ui/blocks/fallback.rs` (`fallback_block_view`).

### Promoted

- `body_to_flat_text` from private in `src/actions.rs` to a shared helper.

### Edited

- 4 `empty()` sites + the `editor_for == None` case → call the fallback; add debug logging at each.
- `src/ui/toolbar.rs` — pre-commit actual shape.
- `src/ui/blocks/inline_editor.rs` — FocusLost suppression via `current_doc`.
- `src/actions.rs` — `debug_assert` with the built-in-provenance gate.
- `src/model/from_core.rs` — harden to route unclassifiable blocks through the fallback, no panics on disk data.

### Tests

Control-server e2e cases as described in Section 8.

---

## 10. Implementation Order

1. Promote `body_to_flat_text` from private in `src/actions.rs` to a shared helper.
2. New `src/ui/blocks/fallback.rs` — `fallback_block_view` with visible content, warning chrome, and focus handler.
3. Wire the 4 `empty()`-on-mismatch sites + the `editor_for == None` case to call the fallback; add debug logging.
4. Edit `src/ui/toolbar.rs` — pre-commit the block's actual shape instead of unconditionally `Inline`.
5. Edit `src/ui/blocks/inline_editor.rs` — FocusLost suppression via `current_doc`.
6. Add `debug_assert` in `src/actions.rs` with built-in-provenance gate.
7. Harden `src/model/from_core.rs` — no panics on disk data, route unclassifiable blocks through the fallback.
8. Write control-server e2e tests for all four layers.

---

## 11. Decisions

### 1. Graceful degradation, not compile-time totality

**Chosen:** a runtime fallback view.
**Rejected:** collapsing `BlockKind`+`BlockBody` into one enum (~26-file refactor that fights the open block-types-as-plugins direction and only closes named failure modes).

### 2. Scope = all three layers

**Chosen:** ship fallback view + commit-source tidy/assert + parse-boundary routing together.
**Rejected:** fallback-only, or fallback+parse-boundary while deferring layer 2.

### 3. Fallback editability → read-only + toolbar recovery

**Chosen:** read-only fallback; recovery via toolbar (Change Type / Delete).
**Rejected:** in-place editing (would risk a fresh mismatch given the ambiguous body shape).

### 4. Warning → persistent, non-dismissible, inline

**Chosen:** persistent inline banner/badge; self-clears when the block renders normally.
**Rejected:** dismissible banner (hides a real problem) and modal/popup (re-fires on reactive renders, traps the user).

### 5. Plugin-failure parity → fold every dead-end into one fallback

**Chosen:** kind/body mismatch, unknown body shape, `editor_for(key) == None` — all route to the same `fallback_block_view`. One path, all dead-ends.

### 6. Model guard → keep coercion, tidy commit sources, then assert

**Chosen:** coercion stays (preserves text), commit sources are tidied so mismatches stop being routine, then a `debug_assert` catches real bugs.
**Rejected:** rejecting mismatched commits (silent data loss); assertion-only with no UI change (does nothing for a release-build user who still loses content).

### 7. Assert provenance gate → built-in-editor provenance, NOT `plugin.is_none()`

**Chosen:** gate on whether the committing editor is a built-in widget (built-in/registry dispatch) versus a future third-party plugin editor.
**Rejected:** `plugin.is_none()` gate (code blocks are plugin-flagged today, so it would skip the exact regression class). Plugin-originated input must always degrade, never assert/panic.

### 8. Testing → full e2e via the control server

**Chosen:** all user-visible behavior verified via control-server e2e (`/state`, `/screenshot`, `/action` replay).
**Note:** the `debug_assert` panic is a CI tripwire and is explicitly not asserted via e2e.

---

## 12. Open Questions for Claude

None. All design decisions listed above are resolved. The spec covers every section with concrete decisions and no placeholders.

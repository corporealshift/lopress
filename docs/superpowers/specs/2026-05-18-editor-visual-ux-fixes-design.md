# Editor Visual / UX Fixes — Design Spec (Spec A)

**Date:** 2026-05-18
**Author:** Kyle (designed with Claude)
**Status:** Approved — ready for implementation planning
**Source:** User-reported issues from `driving-lopress-editor` debug skill

---

## Scope

This document covers **visual and UX fixes only** for the lopress editor (`lopress-editor` crate). A separate "Spec B" will cover performance work (workspace-open speed, file-open speed, virtualization, undo-snapshot cloning, etc.). Spec A is intentionally scoped to fix the four reported problems without touching any performance-sensitive paths.

---

## Problem Statement

The lopress editor has four visual/UX problems:

1. **The text cursor (caret) is not visible**, making it hard to know where editing happens.
2. **The editor pane does not scroll** — the whole document cannot be seen.
3. **Individual blocks do not wrap appropriately** and show their own scrollbars.
4. **There is no indication of which block currently has focus.**

### Root-cause diagnosis

- **Problems 2 and 3** share a single root cause. Floem's `editor_container_view` (used by `inline_editor.rs::editable_inline`) wraps every block's editor in its own `scroll` container via `editor_content → scroll({...})`. This means each block is independently scrollable (showing its own scrollbars — problem 3), and mouse-wheel events over a block are consumed by that inner scroll and never bubble to the editor pane's outer `scroll`, so the pane never scrolls (problem 2).

- **Problem 1:** The caret only paints when `is_active` is true. lopress reads the editor's `active` flag via `with_untracked` in the visibility closure passed to the editor view, so the caret paint is not invalidated when focus changes. The caret color may also not contrast against the white background.

- **Problem 4:** The pane already tracks the focused block via a `focus_pub.block` signal (`FocusPublisher`). A focus border simply needs to be rendered.

---

## Section 1 — Block rendering: drop the inner scroll, size to wrapped content

### Change

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, function `editable_inline`, replace the call to Floem's higher-level `editor_container_view` with a direct call to the lower-level public `editor_view(editor_sig, is_active)`. The higher-level wrapper adds a gutter and the per-block `scroll`; using `editor_view` directly drops both.

### Event wiring

Because `editor_container_view` / `editor_content` previously wired several events, lopress must re-wire them itself on the `editor_view` result:

| Event | Action |
|-------|--------|
| `PointerDown` | Call `id.request_active()`, `id.request_focus()`, then `editor.pointer_down(pointer_event)` |
| `PointerMove` | `editor.pointer_move` |
| `PointerUp` | `editor.pointer_up` |
| `FocusGained` / `FocusLost` | Notify the editor's focus triggers |
| `KeyDown` | Dispatch through the existing key handler |

The content view's `ViewId` must be stored into `editor.editor_view_id` (the existing focus-targeting effect depends on this). The gutter is dropped entirely (it is already hidden today via `GutterClass`).

### Block height

Today the block declares a fixed height of `text.split('\n').count() × line_height`, i.e. counting only hard newlines. This is wrong once text wraps. Change it to the editor's **visual line count** — `editor.last_vline() + 1` — multiplied by `line_height`.

This height must be recomputed reactively so the block reflows when either the text changes or the column width changes (wrapping reflows at the 720 px max content width). Floem's editor wraps at editor width by default (`WrapMethod::EditorWidth`).

### Outcome

Blocks wrap at the column width, grow vertically to fit their content, show no inner scrollbar, and mouse-wheel events bubble up to the editor pane's scroll.

---

## Section 2 — Cursor visibility

### Diagnosis

Use the `driving-lopress-editor` debug skill to focus a block and screenshot to confirm caret behavior.

### Fixes

**(a) Reactive `is_active` read.** The `is_active` closure passed to the editor view currently reads the editor `active` flag via `with_untracked`, so the caret paint is not invalidated when focus changes. Make this read **tracked** (reactive) so paint re-runs when `active` flips.

**(b) Explicit dark caret color.** Set an explicit dark caret color on the editor style so the caret contrasts against the white editing background.

### Verification

Confirm the caret blink state (`cursor_info.hidden`) is not stuck hidden.

---

## Section 3 — Focus indicator

In `crates/lopress-editor/src/ui/blocks/mod.rs`, function `block_view`, the block row gets a reactive border keyed on `focus_pub.block.get() == Some(block_id)`.

- When the block is focused: render a **1 px subtle accent border** (light blue).
- When not focused: render a **same-width transparent border** so the layout does not shift when focus changes.

Apply a small border-radius for polish.

---

## Section 4 — Pane scroll verification

Once the per-block inner scrolls are removed (Section 1), the editor pane's existing outer `scroll` in `crates/lopress-editor/src/ui/editor_pane.rs` works automatically, because the block column's total height becomes the sum of the now-correct per-block heights.

This section is **verification only** — no new code beyond confirming a long document scrolls.

---

## Testing

### Manual verification

Via the `driving-lopress-editor` skill:

1. Caret is visible in a focused block.
2. A long paragraph wraps within the column with no inner scrollbar.
3. The focus border appears on the focused block.
4. A long document scrolls in the pane.

### Unit test

The block-height computation given a visual line count produces the expected pixel height.

---

## Non-goals / out of scope

- **All performance work** — that is Spec B (a separate spec): workspace-open speed, file-open speed, virtualization, undo-snapshot cloning, etc.
- **Replacing the per-block Floem `Editor` wholesale.** Spec A keeps the existing `Editor` per block and only changes how it is mounted and sized.

---

## Resolved decisions and tradeoffs

### Decision: use `editor_view` directly instead of `editor_container_view`

**Rejected alternative:** Keep `editor_container_view` and try to neutralize/hide the inner `scroll` via styling.

**Reasoning:** The inner scroll still exists and may still swallow wheel events — fighting the framework instead of using its intended lower-level API.

**Cost of chosen approach:** lopress takes on ~15 lines of pointer/key event wiring that `editor_content` previously did for it. **Benefit:** full control and a clean fix.

### Decision: block height derives from visual line count

Block height is computed from the editor's visual line count (wrapped lines), recomputed reactively on text or width change.

**Rejected:** The current hard-newline count, which is wrong for any wrapped text.

### Decision: focus border uses same-width transparent border in unfocused state

**Reasoning:** Avoids layout shift when focus changes, rather than toggling border presence.

### Decision: scope split

The reported issues are split into Spec A (visual/UX, this doc) and Spec B (performance), and Spec A is done first.

---

## Open questions for Claude

None identified. All design decisions in this spec are settled.

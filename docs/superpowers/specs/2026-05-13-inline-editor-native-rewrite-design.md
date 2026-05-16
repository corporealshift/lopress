# Inline Editor Native Rewrite — Design Spec

**Date:** 2026-05-13
**Status:** Approved
**Goal:** Replace the hand-rolled inline editor widget with Floem's native `text_editor_keys` primitive, fixing broken word wrap, incorrect click-to-place cursor, the spurious line break after the caret, and the non-functional Shift+Enter soft break.

---

## Problem Statement

The current `editable_inline` widget (`src/ui/blocks/inline_editor.rs`) is entirely hand-rolled:

- The caret is a fake 1 px `empty()` span injected inline into an `h_stack_from_iter` of text spans. This causes a visible line break to appear after every cursor position.
- Word wrap is not implemented. Long paragraphs overflow the window instead of reflowing.
- Click-to-place uses `GeometryCache::approximate_for` which assigns every character a fixed width of `0.55 × font_size`. This is wrong for proportional fonts and lands the cursor in the wrong position.
- `↑`/`↓` navigation jumps immediately to adjacent blocks, even when the current paragraph wraps onto multiple visual lines.
- Shift+Enter is silently dropped (`return true` with no action).

These are structural failures that cannot be patched. The approximation model is wrong at the root.

Floem 0.2 ships a full `text_editor_keys` primitive backed by `floem_editor_core` (the Lapce editor core). It provides real glyph-position text layout, visual-line-aware cursor navigation, correct word wrap, and a real blinking cursor. This rewrite replaces the hand-rolled widget with that primitive.

---

## Architecture

Each inline block gets one `RwSignal<Editor>` (the Floem editor signal, which the custom key handler closure receives on every keypress). Style information is factored out of `Vec<InlineRun>` into a parallel `Vec<StyleSpan>` (character-range → style flags), stored as a `RwSignal<Vec<StyleSpan>>` alongside the editor and fed to a custom `Styling` implementation so Floem renders bold/italic/code/link natively.

Both signals — `RwSignal<Editor>` and `RwSignal<Vec<StyleSpan>>` — live in `AppState` as `HashMap<BlockId, …>` so they survive across re-renders of the editor pane. When a block is first rendered its entry is initialized from the block's `Vec<InlineRun>`. When focus leaves, or any block action commits, the rope text and style spans are read back and converted to `Vec<InlineRun>` for the `BlockAction::EditInline` chokepoint.

The `DocSelection` / `SelectionContext` / `GeometryCache` layer is **removed entirely**. Single-block selection is owned by the native editor. Cross-block operations (multi-block delete, copy, cut, paste) are handled by the custom key handler reading cursor position from the editor signal and emitting `BlockAction` variants, but the selection-projection and geometry approximation machinery is gone.

**Hard constraint:** All code from the old approach is deleted, not left alongside. No dead code, no commented-out blocks. The PR must have zero references to `SelectionContext`, `GeometryCache`, `DocSelection`, `BlockSelection`, `ToggleInlineRange`, or `DeleteRange` when it merges.

---

## Data Model

### `StyleSpan`

```rust
// src/ui/blocks/style_span.rs

pub struct StyleSpan {
    pub start: usize,          // inclusive char offset from block start
    pub end: usize,            // exclusive char offset
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}
```

### `InlineRunStyling`

Implements Floem's `Styling` trait. Holds `RwSignal<Vec<StyleSpan>>`. On each text-layout pass Floem calls it with a line range; `InlineRunStyling` walks the spans overlapping that range and applies `Attrs` via `AttrsList`:

- `bold` → `font_weight(Weight::BOLD)`
- `italic` → `font_style(Style::Italic)`
- `code` → monospace family + light background
- `link` → link color (existing `LINK_COLOR` constant)

### Sync conversions

New file: `src/model/sync.rs`

**`inline_runs_to_rope_and_spans(runs: &[InlineRun]) -> (Rope, Vec<StyleSpan>)`**
Concatenates run texts into a rope. Produces one `StyleSpan` per run using accumulated char offsets. Adjacent runs with identical style flags coalesce into a single span. `'\n'` characters in run text survive unchanged — they become real line breaks in the rope and create soft line breaks within the block.

**`rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun>`**
Slices rope text at each span boundary, produces one `InlineRun` per span. `'\n'` in a span's text is preserved in the `InlineRun.text` field and serialized to the markdown `<br>` equivalent on save.

Both conversions are the complete round-trip: `runs → (rope, spans) → runs` is identity for any valid input.

### `AppState` additions

```rust
block_editors: HashMap<BlockId, RwSignal<Editor>>,
block_style_spans: HashMap<BlockId, RwSignal<Vec<StyleSpan>>>,
```

When `editor_pane` renders, it ensures an entry exists for each block in the doc. Stale entries (blocks removed since last render) are pruned on doc load.

---

## Key Handler

The closure passed to `text_editor_keys` receives `(RwSignal<Editor>, &KeyPress, Modifiers)` and returns `CommandExecuted::Yes` (handled) or `CommandExecuted::No` (fall through to Floem's default).

| Key | Action |
|-----|--------|
| `Enter` (no modifiers) | Commit runs via `EditInline`; emit `BlockAction::Split` at rope cursor offset. Return `Yes`. |
| `Shift+Enter` | Insert `'\n'` at cursor into the rope. Floem's word-wrap shows a new visual line inside the same block. Return `Yes`. |
| `Backspace` at offset 0 | If cursor is at position 0, commit + emit `BlockAction::MergeWithPrev`. Return `Yes`. Otherwise return `No` (Floem handles within-block backspace). |
| `↑` on first visual line | Query the editor's `VisualLine` layout; if cursor is on visual line 0 of this block, commit + set `focus_target` to previous block id. Return `Yes`. Otherwise `No`. |
| `↓` on last visual line | Same pattern for the last visual line → `focus_target` to next block id. Return `Yes`. Otherwise `No`. |
| `Ctrl+B/I/E/K` | Read editor's current selection byte range, translate to char offsets, apply toggle to `RwSignal<Vec<StyleSpan>>` (all-set → clear, else set). Return `Yes`. |
| `Ctrl+A` | Pass through (`No`). Floem's default selects all text in the editor. |
| `Ctrl+C/X/V` | Pass through (`No`) for single-block. Multi-block paste goes through `BlockAction::PasteBlocks` as today. |

Cross-block focus uses the existing `RwSignal<Option<BlockId>>` (`focus_target`). When the key handler fires a cross-block jump it sets `focus_target`; a `create_effect` in each block's view calls `view_id.request_focus()` when its id appears. No geometry cache needed — the adjacent block's native editor restores its own cursor to end/start naturally.

---

## Files Changed

### Deleted entirely

| File | Reason |
|------|--------|
| `src/selection.rs` | `DocSelection`, `GeometryCache`, `BlockSelection`, `project()` — all replaced by native editor selection |
| `src/ui/sel_ctx.rs` | `SelectionContext` wrapper — gone |

### Rewritten

| File | What changes |
|------|-------------|
| `src/ui/blocks/inline_editor.rs` | Stripped to: pure-data helpers (`insert_char`, `backspace`, `delete`, `toggle_inline`) operating on `Vec<StyleSpan>`; `InlineRunStyling` struct + `Styling` impl; `editable_inline` rebuilt around `text_editor_keys`. Geometry cache effect, all `DocSelection` / `SelectionContext` references, and `LocalSelection` removed. |
| `src/state.rs` | Adds `block_editors` and `block_style_spans` maps; removes any `SelectionContext`/`GeometryCache` fields. |

### New files

| File | Contents |
|------|----------|
| `src/model/sync.rs` | `inline_runs_to_rope_and_spans`, `rope_and_spans_to_runs` |
| `src/ui/blocks/style_span.rs` | `StyleSpan` type, `InlineRunStyling` struct, `Styling` impl |

### Lightly touched

| File | What changes |
|------|-------------|
| `src/ui/editor_pane.rs` | Passes editor/span signals down instead of `sel_ctx`; removes `SelectionContext` construction; keeps `focus_target` and slash menu wiring. |
| `src/ui/blocks/mod.rs`, `paragraph.rs`, `heading.rs`, `list.rs` | Update call sites to pass new signals instead of `sel_ctx`. |
| `src/ui/toolbar.rs` | Reads `RwSignal<Vec<StyleSpan>>` from `focus_pub` instead of `focus_pub.runs`; emits style toggles via the `Ctrl+B/I/E/K` path. |
| `src/actions.rs` | Remove `BlockAction::ToggleInlineRange` and `BlockAction::DeleteRange` (cross-block selection ops that no longer exist). `EditInline` keeps its current shape. |
| `src/model/mod.rs` | Add `pub mod sync`. |

---

## Scope & Limitations

**Multi-block selection is not supported in this rewrite.** Each block has its own native editor; Floem has no mechanism for a selection that spans two separate editor views. Shift+↑/↓ that reaches a block boundary collapses the selection and moves focus to the adjacent block (same as a non-extending jump). Shift+click on a different block is not intercepted — it will behave as a click (collapse, move focus).

`BlockAction::DeleteRange`, `BlockAction::ToggleInlineRange`, and all multi-block clipboard operations that depended on `DocSelection` are removed. `Ctrl+C/X` on a single block's selection works via the native editor clipboard. Multi-block copy/cut is out of scope and can be revisited in a follow-up.

This is an intentional trade-off: single-block editing works correctly; cross-block selection is deferred.

---

## Testing

### Automated

- **`tests/sync_tests.rs`** — round-trip tests for `inline_runs_to_rope_and_spans` → `rope_and_spans_to_runs`:
  - Plain text, single run
  - Bold/italic/code/link runs
  - Mixed adjacent runs, some with matching styles (coalesce check)
  - Runs containing `'\n'` (soft break round-trip)
  - Empty block (empty runs vec)
- **`tests/style_span_tests.rs`** — `toggle_inline` unit tests on `Vec<StyleSpan>`:
  - All-set → clears
  - Partial-set → sets all
  - Collapsed selection is no-op
  - Adjacent same-style spans coalesce after toggle

### Manual smoke (required before commit)

1. Open a long paragraph — confirm it wraps at the window edge.
2. Click mid-wrapped-line — confirm caret lands at the clicked character, not at a wrong offset.
3. No spurious line break visible after the cursor position.
4. `Shift+Enter` — inserts a visible line break within the block (does not split into two blocks).
5. `↑`/`↓` — navigates within wrapped lines before jumping to an adjacent block.
6. `Enter` — splits block at cursor. `Backspace` at block start — merges with previous block.
7. Select text, `Ctrl+B` — text renders bold in the editor. `Ctrl+B` again — bold cleared.
8. `Ctrl+I`, `Ctrl+E` — italic and code toggles work on selection.

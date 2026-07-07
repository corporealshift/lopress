---
name: building-on-floem
description: Use when writing or debugging any floem UI code in the lopress editor — new widgets, panes, overlays, styling, signals/reactivity, focus handling, or symptoms like collapsed/ballooned layout, click-dead overlays, lost edits on blur, or scroll position resetting.
---

# Building on Floem (lopress editor)

## Overview

The lopress editor is built on **floem 0.2**, a reactive Rust GUI framework. This skill captures how this codebase uses it and the traps that have each cost a real debugging session. When something looks impossible ("it paints but ignores clicks"), check the gotcha table before assuming your logic is wrong.

## Core model

- **Views are built once** by plain functions returning `impl IntoView` / `AnyView`. There is no re-render pass; **reactivity lives in closures** — `.style(move |s| …)`, `dyn_container(...)`, event handlers — that re-run when the signals they read change.
- **State is `RwSignal<T>`** (`floem::reactive`). Read with `.get()` (tracks) or `.with_untracked()`/`.get_untracked()` (doesn't). Writing with `.set()`/`.update()` re-fires every tracking closure.
- **`dyn_container(source, view_fn)` does NO value diffing** — it fires on *every* update of the source signal and destroys + rebuilds its child. Treat it as "nuke and rebuild this subtree", not React-style reconciliation.
- Widgets compose with `h_stack`/`v_stack`/`stack`, `v_stack_from_iter`, `scroll`, `label`, `button`, `empty`, and the `Decorators` trait for `.style()`/`.on_event()`.
- Event handlers return `EventPropagation::Continue` or `::Stop`.
- **Logging:** `eprintln!` gated `#[cfg(debug_assertions)]`. There is no `log`/`tracing` in this codebase — don't import one.

## Structuring for rebuild containment

Because `dyn_container` rebuilds wholesale, **where you put it decides what survives an edit**:

- Put `dyn_container` as **deep** as possible; keep stateful nodes **outside** it. In `ui/editor_pane.rs` the `dyn_container` is the *child* of the `scroll` — so edits rebuild only the block column and the scroll node keeps its offset. When the `scroll` was inside, every keystroke snapped the viewport to the top.
- The whole block column rebuilds on **every** `current_doc.update()` — including the commit that fires when an inline editor loses focus. Consequence: clicking any button blurs the editor → commit → rebuild → the per-block UI you clicked is destroyed before it can act. **UI that must survive interaction with a focused block must be pane-level** (see `ui/link_bar.rs`), capturing its target/selection at click time.
- Per-block widgets own their local signals (runs, caret); those die on rebuild. Anything that must persist across edits belongs in pane-level signals threaded down via `BlockEnv` (`ui/blocks/env.rs`).

## Layout rules

- `width_full()` is a **percentage** — it needs a definite parent width to resolve against. A `scroll` child's width is *indefinite*, so `width_full` there collapses to min-content unless **every intermediate container** (especially `dyn_container`) also sets `width_full`. This is load-bearing in `editor_pane.rs` and caused the collapsed-editor-width bug.
- Text measured at width 0 (mid-rebuild) wraps one char per line; if you cache heights, **ignore readings taken at width < 1** and keep the previous value, or focused blocks balloon to thousands of pixels.
- `text_input` with Auto width sizes to ~20 chars of *text* even when the box stretches via `flex_grow` — set explicit `width_full()`/`width(px)` on inputs showing pre-filled values.

## Hit-testing and overlays

floem 0.2 does **not** hit-test absolutely-positioned children that overflow above/left of their parent (negative inset). They paint fine but are click-dead. Pattern that shipped: keep chrome **in-flow** in a fixed-height slot and pull content up with a compensating negative margin (see `TOOLBAR_HEIGHT_PX` in `ui/blocks/mod.rs`).

## Focus and committing edits

- `on_event(EventListener::FocusLost)` on a `text_editor` **never fires** — focus lives on the inner editor view. Commit via the inner editor's `editor_view_focus_lost` Trigger, and read the text from the live `doc().text()`, **not** the Rope you passed at construction (that's a stale clone). This shipped as attr textareas saving empty.
- Focus handoff to a new block goes through the pane-level `focus_target: RwSignal<Option<BlockId>>`; don't invent a parallel mechanism.

## Quick-reference gotcha table

| Symptom | Cause | Fix |
|---|---|---|
| Content collapses to min-content inside a scroll | `width_full` can't resolve through content-sized intermediates | `width_full` on **every** layer, esp. `dyn_container` |
| Scroll jumps to top on every edit | `scroll` node is inside the rebuilt `dyn_container` | Move `dyn_container` inside the `scroll` |
| Overlay paints but ignores clicks | Absolute child overflows above/left of parent — not hit-tested | In-flow slot + compensating negative margin |
| `FocusLost` on `text_editor` never fires | Outer view never has focus | Inner editor's `editor_view_focus_lost` Trigger + live `doc().text()` |
| Clicking a button destroys the UI it targets | Blur → commit → `current_doc.update()` → pane rebuild | Make the UI pane-level; capture target at click time |
| Focused block renders enormously tall | Height cached from a width-0 layout pass | Height memo ignores width<1 readings |
| `text_input` clips long values | Auto width sizes the text, not the box | Explicit `width_full`/`width(px)` |

## Debugging method

1. Reproduce in the live editor on a scratch site (`driving-lopress-editor` skill).
2. Paint each suspect layer a loud `.style(|s| s.background(...))` color, `/screenshot`, and find the exact layer where layout or hit-testing breaks — guessing from code rarely finds these.
3. Check the gotcha table; several entries masquerade as logic bugs.
4. Trust `cargo build` over rust-analyzer squiggles after rapid edits — RA diagnostics lag.

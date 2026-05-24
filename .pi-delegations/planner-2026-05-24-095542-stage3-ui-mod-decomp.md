You are an implementation planner. Read the spec yourself and decide the
decomposition: map out which files to create or modify and what each is
responsible for, then break the work into tasks — each a self-contained,
independently testable change — in a sensible order. Expand every task into
bite-sized steps: write the failing test, run it to confirm it fails, write
the minimal implementation with real code, run the test to confirm it passes,
commit. Every code step must contain actual, complete code — never "TBD",
never "add error handling", never "similar to Task N". Use exact file paths
and exact commands with their expected output.

## Write this implementation plan

Read the spec at
`docs/superpowers/specs/2026-05-23-code-editor-block-and-ui-mod-split-design.md`
— plan **Section 4 (`ui/mod.rs` decomposition)** only. This is a pure
move-and-rename refactor; no behaviour changes.

Write the plan to
`docs/superpowers/plans/2026-05-24-stage3-ui-mod-decomposition.md`.

**Prerequisite check:** Stages 0, 1, and 2 are all committed on
`feat/code-editor-block` (the implementer can verify with
`git log --oneline | head -25`). One follow-on fix landed for Stage 2
(`8a7e772 fix(editor): re-point built-in BlockKind::Code arm in block_view
too`) and a separate `apply_change_type` bug was fixed in `bb36cb9` plus
test commits. None of those touched `ui/mod.rs`, so Section 4's file
layout snapshot in the spec is still accurate.

The Stage 2 plan and the Stage 1 plan are the closest format references:
- `docs/superpowers/plans/2026-05-23-stage2-code-editor-widget.md`
- `docs/superpowers/plans/2026-05-23-stage1-code-base-plugin-and-mirror.md`

Both use the same "for qwen" preamble, heredoc commit messages, and
`Co-Authored-By: Qwen <noreply@anthropic.com>` trailer.

## Required plan header

```markdown
# Stage 3 — `ui/mod.rs` decomposition

> **For the implementer (qwen):** execute this plan task-by-task in order.
> **Prerequisite:** Stages 0, 1, and 2 (plus the two follow-on fixes) must
> be fully committed on this branch before Task 1 starts. Verify with
> `git log --oneline | head -25` — you should see commits up through
> `327161b test(editor): add round-trip tests for all 8 ChangeType
> conversion directions`. If they aren't there, stop and report.
>
> This stage is a **pure refactor** — no behaviour changes. The whole test
> suite must stay green after every commit. If a commit causes any test
> to fail, the move was wrong; stop and report rather than rewriting the
> test.
>
> Commit per task. After every commit run `cargo test --workspace` and
> `cargo check --workspace`. Use heredoc commit messages with the
> `Co-Authored-By: Qwen <noreply@anthropic.com>` trailer.

**Goal:** Break the ~666-line `crates/lopress-editor/src/ui/mod.rs` (with
a ~400-line `editing_view` function) into small sibling modules under
`ui/editing/`, each owning one clear responsibility. `editing_view`
becomes ~80 lines that assembles the pieces. No behaviour changes; tests
prove correctness.

**Architecture:** Module-per-responsibility under a new `ui/editing/`
directory. Each module exports plain functions that take the signals they
need as arguments and return a closure or start an effect. Signals stay
owned by `editing_view`'s body — no shared state struct. Stage 2's
private `defer_focus` duplicate in `ui/blocks/code_editor.rs` is unified
with the list block's via the new `ui/editing/focus.rs::defer_focus` as
part of this stage.

**Tech stack:** Rust 2021, Floem reactive UI. Verification:
`cargo test --workspace`, `cargo check --workspace`.

---
```

## Scope

Section 4 of the spec. One focused refactor. Single plan's worth of work
even though the diff touches many files — every change is "move
code without modification" or "add tiny re-export shim".

Out of scope:
- Any behaviour change (this is a structural refactor).
- Touching modules outside `ui/`.
- Adding new features.
- Migrating paragraph/heading to the registry.

## Conventions

- **Test framework:** Rust `#[test]`. The bar is the existing test suite
  staying green. No new tests strictly required for the move itself — a
  pure refactor leaves observable behaviour unchanged. The planner MAY
  add small unit tests for any of the extracted helpers if they're
  self-contained (e.g., `kind_tag`, `focus_after_apply`); that's a nice-
  to-have, not a requirement.
- **Run commands:** `cargo test --workspace` between every task;
  `cargo check --workspace` for fast incremental sanity.
- **Commit-message style:** Conventional commits, heredoc form, `Co-
  Authored-By: Qwen <noreply@anthropic.com>` trailer. Use
  `refactor(editor):` for the moves.

## Concrete file inventory (verified — use these in the plan)

### Target module tree under `crates/lopress-editor/src/ui/`

```
ui/
  mod.rs              -- root_view, StateTag, MAX_RECENTS only (~80 lines)
  editing/
    mod.rs            -- editing_view: assembles the pieces (~80 lines)
    focus.rs          -- focus_block_for, focus_after_apply, defer_focus
    pane_key.rs       -- KindTag, kind_tag, build_pane_key closure factory
    action_sink.rs    -- build_action_sink(...) returning the on_action ActionSink
    undo_redo.rs      -- build_undo(...), build_redo(...) each returning Rc<dyn Fn()>
    save_pipeline.rs  -- SavePipeline struct + start_save_pipeline(...)
    new_doc.rs        -- DocKind, make_new_doc_action
    ctrl_wire.rs      -- #[cfg(debug_assertions)] wire_ctrl(...)
```

### Where the moved code currently lives in `ui/mod.rs`

Use these line ranges as the lift targets. The implementer should re-verify
with the file at HEAD before each task — line numbers may drift slightly.

| Target module | Current location | Notes |
|---|---|---|
| `focus.rs::focus_block_for` | `ui/mod.rs:137-148` | Moved verbatim |
| `focus.rs::focus_after_apply` | `ui/mod.rs:155-164` | Moved verbatim |
| `focus.rs::defer_focus` | (new) | NEW — the `floem::action::exec_after(Duration::ZERO, move \|_\| focus_target.set(Some(target_id)))` helper; signature `pub fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId)`. Body is currently duplicated in `ui/blocks/list.rs::defer_focus` (lines 115-119) and `ui/blocks/code_editor.rs::defer_focus` (lines 234-238). After extraction, both callers `use crate::ui::editing::focus::defer_focus` and delete their private copies. |
| `pane_key.rs::KindTag` + `kind_tag` | `ui/mod.rs:580-597` | Moved verbatim |
| `pane_key.rs::build_pane_key` | `ui/mod.rs:396-405` (the `pane_key` closure) | Wrap the existing closure body in `pub fn build_pane_key(current_doc: RwSignal<Option<EditorDoc>>) -> impl Fn() -> Option<Vec<(BlockId, KindTag, bool)>> + Copy` |
| `action_sink.rs::build_action_sink` | `ui/mod.rs:249-318` (the `on_action` closure) | Wrap in `pub fn build_action_sink(current_doc, focus_target, slash_menu_open, undo_stack, mark_dirty) -> ActionSink`. Inputs: all `RwSignal<_>` / `Rc<dyn Fn()>`. Returns the closure. |
| `undo_redo.rs::build_undo` | `ui/mod.rs:320-350` | Wrap in `pub fn build_undo(undo_stack, current_doc, focus_target, mark_dirty) -> Rc<dyn Fn()>` |
| `undo_redo.rs::build_redo` | `ui/mod.rs:352-377` | Wrap in `pub fn build_redo(undo_stack, current_doc, focus_target, mark_dirty) -> Rc<dyn Fn()>` |
| `save_pipeline.rs::SavePipeline` + `start_save_pipeline` | `ui/mod.rs:237-247` (signal creation), `438-490` (debounce + status polls) | Bundle signals + spawn the debounce + start status polls. Signature per spec Section 4. |
| `new_doc.rs::DocKind` + `make_new_doc_action` | `ui/mod.rs:599-666` | Moved verbatim |
| `ctrl_wire.rs::wire_ctrl` | `ui/mod.rs:500-543` (the `#[cfg(debug_assertions)] if let Some((ctrl_handle, ctrl_action_rx)) = ctrl { ... }` block) | Wrap in `#[cfg(debug_assertions)] pub fn wire_ctrl(ctrl: (CtrlHandle, Receiver<CtrlActionEnvelope>), current_doc, current_path, on_action)` |

### Re-exports

Keep `ui/mod.rs::root_view` as the public entry point. Inside
`editing_view`, import from `crate::ui::editing::{focus, pane_key,
action_sink, undo_redo, save_pipeline, new_doc, ctrl_wire}` (or use
`pub use` shims under `ui/editing/mod.rs`). The planner picks whichever
import style produces less syntactic noise; both work.

### Stage 2's `defer_focus` duplicate cleanup

After Stage 2, `crates/lopress-editor/src/ui/blocks/code_editor.rs` carries
a private `defer_focus` function (lines 234-238) and the
`crates/lopress-editor/src/ui/blocks/list.rs` carries another at lines
115-119. Both are bit-identical. As part of this stage's Task that
introduces `focus.rs::defer_focus`, BOTH callers should be updated to
`use crate::ui::editing::focus::defer_focus;` and their private copies
deleted. This is the planned-temporary-duplication cleanup the Stage 2
spec called out.

## Suggested task decomposition (planner may revise)

This is the smallest sensible split. Each task is a tight commit: move
code, update imports, run tests. Six tasks total.

1. **Create `ui/editing/` skeleton + extract `focus.rs`** — create the
   directory and empty `editing/mod.rs`. Move `focus_block_for` and
   `focus_after_apply` from `ui/mod.rs` into `editing/focus.rs`. Add
   `defer_focus` to `focus.rs`. In `ui/mod.rs`, import the moved functions
   from `crate::ui::editing::focus`. Update `ui/blocks/list.rs` and
   `ui/blocks/code_editor.rs` to `use crate::ui::editing::focus::
   defer_focus` and delete their private copies. Run `cargo test
   --workspace`. Commit.

2. **Extract `pane_key.rs`** — move `KindTag`, `kind_tag`, and the
   `pane_key` closure into a `build_pane_key` function in
   `editing/pane_key.rs`. Update `ui/mod.rs::editing_view` to call
   `build_pane_key(current_doc)`. Commit.

3. **Extract `new_doc.rs`** — move `DocKind` and `make_new_doc_action`.
   Commit.

4. **Extract `save_pipeline.rs`** — define the `SavePipeline` struct
   bundle and `start_save_pipeline` function. Move the signal creation
   for `build_status_sig`, `dirty_sig`, `save_error_sig`,
   `serve_status_sig`, the `mark_dirty` builder, the `debounce_action`
   block, and the `start_build_status_poll` / `start_serve_status_poll`
   calls. `editing_view` body shrinks correspondingly. Commit.

5. **Extract `action_sink.rs` and `undo_redo.rs`** — move the
   `on_action` closure body into `build_action_sink` and the `on_undo`
   / `on_redo` builders into `undo_redo.rs`. Both modules can be a single
   commit if the diff stays small; the planner may split into two if
   either chunk is large. Commit.

6. **Extract `ctrl_wire.rs` + final cleanup** — move the debug HTTP
   wire-up into `wire_ctrl(...)` gated on `#[cfg(debug_assertions)]`.
   `editing_view` now matches the spec's sketch (~80 lines). Final
   `cargo test --workspace` and `cargo check --workspace` clean.
   Commit.

The planner may merge tasks 5 and 6 if both extractions feel cohesive,
or split task 4 into "signals + mark_dirty" and "debounce + polls" if
the change footprint is large.

## Notes for the planner

- The current `editing_view` captures many `Rc<RefCell<Option<EditingState>>>`
  clones across closures. When moving code into modules, those `Rc`
  clones become function arguments. The planner should make argument
  lists explicit and ordered (signals first, then `Rc<dyn Fn()>` callbacks,
  then `Rc<RefCell<_>>` handles).
- `save_pipeline::SavePipeline` is a plain bag of signals (no methods that
  hold state). Per the spec: `pub struct SavePipeline { mark_dirty,
  dirty_sig, save_error_sig, build_status_sig, serve_status_sig }`.
- `editing_view` after the split should match the sketch in the spec's
  Section 4 (~9-step assembly with comments). Pi is welcome to slightly
  reorder for readability, but the steps should remain a flat
  signals→builders→sidebar→pane→inspector→footer→ctrl→assembly flow.
- The planner should explicitly note: this stage adds no tests beyond
  what's needed to verify the refactor compiles and the existing suite
  passes. The bar is "cargo test --workspace passes after every commit."

## Things the planner should not do

- Do not add tests for moved code beyond small focused units (`kind_tag`,
  `focus_after_apply`) — full suite is the regression test.
- Do not modify behaviour. If a closure's body is moved, its content
  stays byte-identical (modulo imports and the few argument-binding
  lines a function wrapper adds).
- Do not delete `ui/mod.rs` — `root_view` lives there.
- Do not touch `ui/blocks/*` except for the `defer_focus` unification
  callouts in Task 1.
- Do not change `Cargo.toml` or any dependency.

## Done when

The plan file exists at the path above, maps the file structure (with
verified paths and line numbers from the inventory above), decomposes the
work into ordered tasks each producing one commit (six total), expands
every task into bite-sized steps with complete code blocks (no "TBD" /
"similar to Task N"), and ends with a final `cargo test --workspace`
verification.

## On completion

Reply with a concise summary: the plan file path and the list of task
titles.

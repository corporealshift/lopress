Execute the implementation plan at
`docs/superpowers/plans/2026-05-24-stage3-ui-mod-decomposition.md`.

The plan is the source of truth. Read it end-to-end before starting Task 1.
This stage is a **pure refactor** — no behaviour changes. The whole test
suite must stay green after every commit. If a commit causes any test
to fail, the move was wrong; stop and report rather than rewriting the
test.

Each task: write the moved code per the plan, register the new module in
`editing/mod.rs` (each task has an explicit step for this — don't skip it,
the build fails without it), update `ui/mod.rs`'s imports, run
`cargo test --workspace` and `cargo check --workspace`, commit.

The plan's preamble lists the prerequisites: Stages 0, 1, 2, plus the two
follow-on fixes (`8a7e772` and `bb36cb9` + its surrounding test commits).
Verify with `git log --oneline | head -25` before starting.

## Constraints

- **Do not modify behaviour.** This is a refactor. Closure bodies move
  byte-identically (modulo imports and function-wrapper argument binding).
- **Do not delete `ui/mod.rs`.** `root_view`, `StateTag`, and `MAX_RECENTS`
  live there.
- **Do not touch any module outside `ui/`** except for the two
  `defer_focus` import updates in `ui/blocks/list.rs` and
  `ui/blocks/code_editor.rs` (Task 1 covers these explicitly).
- **Do not add or modify tests** except where the plan calls it out — the
  refactor's bar is the existing test suite staying green.

## Stop conditions

If at any point a snippet in the plan doesn't match the file on disk, or
a `cargo test --workspace` run fails, stop and report rather than
improvising. The plan was checked against the file state at commit
`1878a5f` (the delegation brief for the change-type fix); newer commits
may have shifted line numbers slightly. If line ranges look off but the
target code is recognisable, proceed and report the drift.

Branch: `feat/code-editor-block`. Do not switch branches.

## Report

When done, reply with:
- The six commit hashes (one per task).
- `wc -l crates/lopress-editor/src/ui/mod.rs` final value.
- `cargo test --workspace` final tally.
- Anything surprising you found while moving code, especially:
  - Closure capture patterns that didn't translate cleanly into function
    arguments.
  - Import resolution issues (the plan uses `crate::` paths consistently;
    if you ran into anything weird, flag it).
  - Whether `editing_view` ended up shorter or longer than the spec's
    target of ~80 lines.

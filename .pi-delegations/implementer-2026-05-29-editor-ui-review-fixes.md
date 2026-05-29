You are an implementation engineer. Execute the implementation plan at
`docs/superpowers/plans/2026-05-27-editor-ui-review-fixes.md` end-to-end.

The plan is the source of truth. Read it from top to bottom before starting
Task 1, then execute each task in order: write the code, run the cargo
verification commands, and commit per task with the heredoc commit messages
the plan already provides. One commit per task. Use `superpowers:executing-plans`
or `superpowers:subagent-driven-development` discipline (the plan's preamble
says so).

## Branch

Work on `feat/editor-ui-review-fixes`. Do NOT switch branches, do NOT create
new branches, do NOT merge or open a PR. Claude reviews your diff after you
return.

## Base-state caveat — line numbers have drifted, the real code wins

The plan and spec were written against commit `849c418` on
`feat/code-editor-block`. You are starting from `main`, which since then has
absorbed a separate memory-optimization pass. That pass edited several of the
exact files this plan touches:

- `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (Task 5)
- `crates/lopress-editor/src/ui/editing/action_sink.rs` (Task 3)
- `crates/lopress-editor/src/actions.rs` (Task 7)
- `crates/lopress-editor/src/ui/blocks/code_editor.rs` (Task 6)

So the plan's cited line numbers and some surrounding-code snippets will NOT
match what is on disk. This is expected. For every step:

1. Grep / read the real current code to locate the construct the step
   describes (a handler, a function, a match arm) — do not trust the line
   number.
2. Apply the change the step intends, adapted to the real surrounding code.
3. If a step's snippet is so different from what you find that you cannot tell
   what the intended change is, STOP and report that step rather than guessing.

In particular, watch for the memory pass having changed signatures or types
the plan assumes (e.g. `BlockAction` variants may now be boxed — Task 7 adds
`EditFrontMatter`; if heavy variants are boxed, match that convention rather
than reverting it; the plan's performance-contract note already says
front-matter is small and should not be boxed).

## Section 2 / Task 8 is a mandatory investigation — do not skip to Task 9

Task 8 instruments a toolbar button's `.action()` with a debug print, runs the
app, and clicks with a real mouse to determine WHY toolbar clicks don't fire.
Task 9's structural fix is only correct for Outcome A. If you get Outcome B or
C, follow that branch instead of implementing Task 9 blindly. Record the
diagnosis in the relevant commit message, as the plan instructs.

If you cannot drive a real mouse / observe the running GUI yourself, do the
parts you can (the instrumentation builds and compiles), then hand back Task 8
as "needs human: real-mouse diagnosis" and do NOT implement Task 9 on a guess.
Continue with the other tasks that don't depend on the Section 2 outcome
(Tasks 1-7; note Tasks 9/11 depend on Task 8's structural decision, and
Task 10/Section 9 depends on Task 9).

## Manual GUI verification

Many tasks have a "Manual verification" step using the `driving-lopress-editor`
skill (HTTP control server at `127.0.0.1:7878`). After Task 4 lands, `/open`
and `/close` make this easier. If you can drive the editor, do the manual
checks. If you cannot, complete the code + cargo verification for that task,
commit, and record the manual checks as "needs human" in your final report so
Claude can drive them during review. Never skip the cargo verification
(`cargo check`/`cargo test`).

## Gates (run on every turn automatically)

The repo has a Stop hook that runs `cargo fmt --all`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace`. The final state of every task must pass all three.
Honor `AGENTS.md`: no `unwrap`/`expect` in production code, justify every
`#[allow]`/`#[expect]`, no lossy `as` casts, prefer pattern matching over
discriminant checks.

## Commit discipline

- One commit per task, with `git add <exact files>` — never `git add -A` / `.`.
- Use the heredoc commit messages from the plan verbatim.
- Never amend, never `--no-verify`.

## Report back

When all tasks are complete (or when you hit a blocker), reply with:
- The list of tasks completed (with their commit subjects).
- Any task handed back as "needs human" and why.
- The Section 2 diagnosis outcome (A/B/C) and what you did about it.
- Anything where the real code diverged enough from the plan that you made a
  judgment call — call those out explicitly so the reviewer can check them.

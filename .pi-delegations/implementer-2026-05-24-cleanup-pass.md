Three small cleanups on `feat/code-editor-block`. Pick the right approach
for each yourself; no plan provided. Commit each as a separate small
commit, conventional commit style, `Co-Authored-By: Qwen <noreply@anthropic.com>`
trailer. Run `cargo test --workspace` and `cargo check --workspace` before
each commit.

## Cleanup 1: investigate the `_bs` rename in `save_pipeline.rs`

In `crates/lopress-editor/src/ui/editing/save_pipeline.rs:84`, commit
`693edd4` renamed `let bs = build_status_sig;` to `let _bs = build_status_sig;`
to silence an unused-variable warning. The underscore suggests
`build_status_sig` was meant to be referenced inside the debounce closure
that follows — likely to flip build status when save fails or to clear
errors on save success — but the move-and-rename refactor dropped the use.

Compare against the original code in `git log -p` for `ui/mod.rs` around
the `debounce_action` block before commit `382709f` (the save_pipeline
extraction). If the original closure referenced `bs` or `build_status_sig`
to drive build status, restore that usage. If it was always unused, just
delete the `_bs` binding outright.

The commit message should explain which it was (missed wiring vs. dead
code from the start). If it's missed wiring and you can't tell what the
right behavior is from the diff, stop and report rather than guessing.

## Cleanup 2: delete the dead `on_action_mark_dirty` clone

In `crates/lopress-editor/src/ui/mod.rs`, the `on_action_mark_dirty`
variable (a `Rc::clone(&save.mark_dirty)`) is now orphaned — the
`on_action` closure that referenced it moved into
`action_sink::build_action_sink` in commit `3756e5a`. The clone in
`mod.rs` is dead.

Find and delete the line. Verify nothing else references
`on_action_mark_dirty` in `mod.rs` (it should be one binding only).
`cargo check` and `cargo test` should stay clean.

## Cleanup 3: delete `ui/blocks/code.rs`

The read-only `code::render_code` is no longer referenced — both the
plugin.rs fallback (commit `54b7dd8`) and the blocks/mod.rs built-in arm
(commit `8a7e772`) now route to `code_editor::editable_code_view`.

Delete `crates/lopress-editor/src/ui/blocks/code.rs` and remove the
`pub mod code;` line from `crates/lopress-editor/src/ui/blocks/mod.rs`.
Search the workspace for any remaining `use crate::ui::blocks::code` or
`code::render_code` references — if anything still imports the module,
flag it and stop rather than deleting.

`cargo check --workspace` and `cargo test --workspace` should stay
clean after the delete.

## Constraints

- Three separate commits, one per cleanup, in any order you prefer.
- Do NOT change behavior. If cleanup 1 turns out to be missed wiring,
  restoring the original use is fine; introducing new behavior is not.
- Stop and report if any cleanup reveals something more complex than
  expected — these are meant to be 5-minute fixes.

## Report

When done, reply with:
- Three commit hashes.
- For cleanup 1: which it was (missed wiring restored, or dead code
  deleted).
- For cleanup 3: any references you found that prevented deletion
  (should be none).
- `cargo test --workspace` final tally.

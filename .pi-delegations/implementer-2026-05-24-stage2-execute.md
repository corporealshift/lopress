Execute the implementation plan at
`docs/superpowers/plans/2026-05-23-stage2-code-editor-widget.md`.

The plan is the source of truth. Read it end-to-end before starting Task 1,
then execute each task in order: write the code exactly as specified, run
the cargo verification commands, commit per task with the heredoc commit
messages already provided. The plan's preamble lays out the prerequisites
(Stage 1 commits must be present — they are, verify with
`git log --oneline | head -10`), the per-task expected output, and the
"stop and report" rule when a snippet doesn't match what you find on disk.

Task 1.5 is a self-review checklist — run it AFTER writing the
`code_editor.rs` file and BEFORE running `cargo test` in Step 1.6. It
catches the common keymap mistakes faster than the compiler does.

Task 3 is manual GUI verification. If you cannot drive the editor GUI
yourself, complete Tasks 1 and 2, run the cargo verification, and hand
back a checklist marking each of the 14 manual checks as "needs human" so
the reviewer can drive them. Do NOT skip the cargo verification.

Branch: `feat/code-editor-block`. Do not switch branches.

Report back when all tasks are complete (or when you hit a blocker that
requires the reviewer's input).

You are an implementation engineer continuing earlier work. Execute Tasks 9,
10, and 11 of the plan at
`docs/superpowers/plans/2026-05-27-editor-ui-review-fixes.md`, then verify (and
if needed fix) the `/open` ctrl endpoint from Task 4.

The plan is the source of truth. Tasks 1–7 and Task 12 are already done and
committed on this branch — do NOT redo them. Read the plan's Tasks 9, 10, 11
end-to-end before starting.

## Branch

Continue on `feat/editor-ui-review-fixes`. Do NOT switch/create branches, do
NOT merge or open a PR. Claude reviews after you return.

## Task 8 diagnosis — RESOLVED: Outcome A (human-verified)

The plan's Task 8 is a mandatory real-mouse investigation that gates Task 9.
It has been performed by a human and the result is **Outcome A**:

- With a paragraph focused, the toolbar renders correctly above the block.
- Clicking **H1** (a block-type button) under a real mouse does nothing — the
  block stays a paragraph.
- Selecting text and clicking **B** (an inline-flag button) under a real mouse
  does nothing either.
- i.e. BOTH the type buttons and the flag buttons are dead under real mouse,
  while the toolbar itself is visible and laid out fine.

This matches the spec's Section 2 hypothesis: the editor surface captures the
pointer event for its whole bounding box before the toolbar button can
hit-test it. So **implement Task 9 as written** (move the focus-border /
click-capturing container to wrap only the editor `row`, leaving the toolbar
outside it). Do NOT re-run the instrumentation/eprintln investigation — the
diagnosis is settled.

Bake the diagnosis into Task 9's commit message body, e.g.:
"diagnosis: human-verified real-mouse clicks on both toolbar type buttons (H1)
and inline-flag buttons (B) do not fire; the editor surface absorbs the
pointer event before the button hit-tests (Outcome A)."

## Base-state caveat — the real code wins

The plan's line numbers are stale (a memory-optimization pass moved things).
For every step: grep/read the real current code to find the construct the step
describes; apply the intended change adapted to what's actually there; STOP and
report if a snippet is too different to map confidently.

Current structure I can confirm:
- `crates/lopress-editor/src/ui/toolbar.rs`: `block_toolbar_for` builds a
  `buttons` vec, wraps it in `button_row` (the `h_stack_from_iter(...)` with the
  bg/border style), and returns `v_stack((button_row, url_row))`.
- `crates/lopress-editor/src/ui/blocks/mod.rs`: holds `block_view`, where the
  toolbar slot and the editor `row` are composed and the focus border lives.

## Order and scope

1. **Task 9** (Section 2 structural fix) — toolbar outside the focus border in
   `block_view`. One commit.
2. **Task 10 / Section 9** (toolbar visual separation) — builds on Task 9. One
   commit.
3. **Task 11 / Section 6** (layout-jump / reserve toolbar height) — builds on
   Task 9. One commit.

Then the `/open` verification below as its own commit (only if a fix is needed).

## Gates

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` must all pass at the end of each task. Honor
`AGENTS.md` (no unwrap/expect in prod, justify every #[allow]/#[expect], no
lossy `as`, pattern-match over discriminant checks). One commit per task with
`git add <exact files>` (never `-A`/`.`), heredoc messages, never amend, never
`--no-verify`.

## Manual verification you CANNOT do — hand it back

Tasks 9–11 acceptance is fundamentally real-mouse (the whole point of Task 9 is
that physical clicks on toolbar buttons start working). You cannot drive a real
mouse, so after implementing + passing the cargo gates + committing, hand back a
checklist marked "needs human real-mouse verification" covering:
- Task 9: focus a paragraph, click H1 with a real mouse → block becomes a
  heading; click B on a selection → bold toggles; click x → block deletes.
- Task 10: toolbar reads as a visually distinct panel; focus border no longer
  wraps the toolbar.
- Task 11: clicking between blocks no longer shifts the document vertically.
Do the cargo verification regardless; never skip it.

## Additional — verify and (if broken) fix `/open` (Task 4 gap)

In an earlier (unreliable) session, `POST /open` appeared to return 404 for a
valid absolute path. Verify this yourself — you can hit the ctrl server with
curl/Invoke-RestMethod; no real mouse needed:

1. `cargo run` (debug). With NO workspace open (welcome screen), POST
   `/open {"path":"<absolute path to a real .md inside a known workspace>"}`.
   The spec (Section 12) requires this to resolve which workspace the path
   belongs to, open that workspace if not already open, and open the file —
   returning `200 {"status":"opened"}` with `/state` then showing
   `doc_open:true`.
2. If it instead returns 404 (or only works once a workspace is already open),
   that's the bug: the open path isn't auto-resolving/opening the workspace
   from the welcome state. Fix it per Section 12 so the absolute-path
   acceptance passes. Also confirm the relative-path base matches the spec's
   examples (`posts/foo.md`, `pages/foo.md` relative to the content root) and
   adjust if it's resolving against the wrong root.
3. If `/open` already satisfies the acceptance, just say so — no change needed.

Commit any `/open` fix separately (it's a Task 4 follow-up, not part of 9–11).

## Report back

- Commits made (subjects), in order.
- The needs-human real-mouse checklist for Tasks 9–11.
- `/open` verification result: did it pass as-is, or what did you fix?
- Any place the real code diverged enough from the plan that you made a
  judgment call.

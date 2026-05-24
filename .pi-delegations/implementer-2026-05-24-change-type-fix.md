Fix the `ChangeType` body-mismatch bug documented at
`docs/superpowers/ideas/2026-05-24-change-type-body-mismatch.md`.

The bug doc is the source of truth. Read it end-to-end before starting.
It enumerates:
- The exact symptom (block renders blank, lang silently cleared on save).
- The exact location: `crates/lopress-editor/src/actions.rs::apply_change_type`
  (lines 425–475).
- The full table of 12 conversion directions and which 8 are broken.
- A concrete fix sketch for each arm — keep `block.body` shape-valid for
  the new kind, derive content from the current body, update `block.plugin`
  to match (Some for Code/List, None for Paragraph/Heading).
- A test-coverage list (every (from_kind, from_body, to_kind) triple).

Branch: `feat/code-editor-block`. Do not switch branches.

## Implementation order

Pure TDD per arm. One commit per logically grouped chunk so qwen-style
commits stay small. Suggested grouping:

1. **`test(editor):` add failing tests for all broken arms.** Add tests to
   `crates/lopress-editor/tests/actions_tests.rs` (or a new
   `change_type_conversion_tests.rs` if the existing file is overgrown — pick
   per the file's current size). One `#[test]` per (from_kind, from_body,
   to_kind) triple from the bug doc's table. Each test:
   - Builds a block in the from state (use `EditorBlock::code`,
     `EditorBlock::list`, etc. — check
     `crates/lopress-editor/src/model/types.rs` for the constructors).
   - For from-state blocks that should carry `PluginMeta`, stamp it
     manually (matching what `from_core` produces).
   - Wraps it in an `EditorDoc` (use the existing `doc_with` helper in
     `actions_tests.rs`).
   - Applies `ChangeType { new_kind: to_kind }` via `actions::apply`.
   - Asserts:
     - `block.kind` matches `to_kind`.
     - `block.body` shape matches what `to_kind` expects (`Inline` for
       Paragraph/Heading, `Code` for Code, `List` for List).
     - `block.body` content matches the conversion rule from the bug doc.
     - `block.plugin` is `None` for Paragraph/Heading, `Some(_)` with the
       right `block_type_name` for Code/List.

   Run `cargo test --workspace -- change_type_` — every new test should
   FAIL with the current implementation. The failure messages are the
   characterization of the bug.

2. **`fix(editor):` rewrite `apply_change_type` body/plugin handling.**
   Replace the match in `apply_change_type` (lines 433–456 in the current
   file) with an explicit arm for every `(new_kind, current_body)` combo
   from the table. Keep the existing function signature and inverse-action
   contract (the inverse still restores `kind` only — Stage 3 of the list-
   editor-unification spec will fix lossy-undo separately; this fix does
   NOT expand the inverse).

   After the fix, every test from step 1 should PASS. Run
   `cargo test --workspace` — full suite green.

3. **(If applicable) `test(editor):` add round-trip tests.** For each
   conversion direction, add a test that applies `ChangeType` then runs
   `to_core(from_core(to_core(doc)))` to confirm the body shape survives
   save/load (the bug doc calls out lang-clearing as a save-corruption
   symptom; this catches that). Reference shape: the existing tests in
   `crates/lopress-editor/tests/from_to_core_tests.rs`.

   The round-trip tests may already pass after step 2 (if the fix is
   correct end-to-end), in which case mark them as additional safety net,
   not a separate fix.

## Constraints

- **Do not change `apply_change_type`'s signature or its inverse-action
  contract.** The inverse still uses old_kind only; lossy undo is a
  separate planned fix per the bug doc's Related section.
- **Do not modify** any file outside `crates/lopress-editor/src/actions.rs`
  and its tests (and possibly imports in the test file). The Stage 1+2
  files are in active use; this is a narrow fix.
- **Match the existing conventions:**
  - Conventional commits, heredoc form for multi-line messages,
    `Co-Authored-By: Qwen <noreply@anthropic.com>` trailer.
  - `cargo test --workspace` and `cargo check --workspace` must be clean
    before every commit.

## Stop conditions

If anything outside the bug doc's enumerated arms shows up (e.g., a
conversion direction the bug doc didn't list, or a snippet doesn't match
the file), stop and report. The bug doc is comprehensive — if the code
disagrees, the bug doc is wrong, and that's a finding worth flagging.

## Report

When done, reply with:
- A table mapping each conversion direction to test name and PASS/FAIL.
- The commit hashes for the test commit, the fix commit, and any
  round-trip commit.
- `cargo test --workspace` final tally.
- Anything surprising you found while implementing (especially around
  the `PluginMeta` plumbing for List/Code).

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
— but plan **only Section 0** (the core block-type rename from `code_block` to
`code`). The rest of the spec (Section 1 onward — the editable code widget,
the `BlockKind::Code` mirror, the `ui/mod.rs` decomposition) is **out of scope
for this plan**; later stages get their own plans.

Write the plan to
`docs/superpowers/plans/2026-05-23-stage0-rename-code-block-to-code.md`,
starting with the required header block below. Produce the File Structure map
and the task decomposition yourself, then expand every task into bite-sized
steps. An existing plan under `docs/superpowers/plans/` is a useful format
reference — `docs/superpowers/plans/2026-05-22-stage5-ctrl-api-result-reporting.md`
is the closest analogue (it has the same "for qwen" implementer preamble and
the same characterization-test-first approach to a careful refactor).

## Required plan header

```markdown
# Stage 0 — Rename core block type `code_block` → `code`

> **For the implementer (qwen):** execute this plan task-by-task in order. You
> have full git and the cargo toolchain — commit per task, run the verification
> suite before each commit, and report back when all tasks are done. Treat me
> as a senior reviewer on call: if a test fails or a snippet here doesn't match
> the file you find, stop and report rather than improvising.

**Goal:** Collapse the awkward `code_block` / `code` naming split by renaming
core's internal block type `"code_block"` to `"code"` across the workspace,
matching how the list block uses a single `"list"` name everywhere.

**Architecture:** Pure internal string rename. `.md` source files do not
change (the type name is never written to markdown — only used in the in-memory
`lopress_core::Block.r#type` field). The rename touches one type-emission site
in lopress-core's parser, one in the serializer, one in lopress-build's
renderer, one in lopress-editor's `from_core`, two in `to_core`, plus any test
strings asserting the old name. Round-trip integration tests prove
byte-identical markdown output across the rename.

**Tech stack:** Rust 2021 edition (Cargo workspace). `cargo test` for the
test runner; `cargo check --workspace` and `cargo test --workspace` are the
verification commands.

---
```

## Scope

The spec section being planned (Section 0) is one focused refactor — a
single-name rename across five Rust files in three crates, plus their tests
and a workspace-wide build verification. Single plan's worth of work. Do
not split into sub-plans.

## Conventions

- **Test framework:** Built-in Rust `#[test]` via `cargo test`. The codebase
  uses both unit tests inside `#[cfg(test)] mod tests` blocks (most modules)
  and external `tests/<name>.rs` integration tests. Round-trip integration
  tests for the renamer live in `crates/lopress-core/tests/roundtrip.rs` and
  `crates/lopress-build/tests/build_integration.rs`.
- **Run commands:** `cargo test --workspace` to run the whole suite;
  `cargo test -p lopress-core` / `-p lopress-build` / `-p lopress-editor` to
  scope to one crate; `cargo check --workspace` for fast compile-only checks.
- **Commit-message style:** Conventional commits, lowercase scope in parens.
  Examples from recent history (look at `git log --oneline -10` to verify):
  - `feat(editor): /action reports dispatched / no_document / block_not_found`
  - `chore(editor): drop now-stale dead_code allows on ctrl result types`
  - `docs(specs): rename core code_block -> code in code-editor spec`
  For this plan, use `refactor(core):`, `refactor(build):`, `refactor(editor):`
  or `test(<crate>):` as appropriate. Each task commits independently.
- **Co-author trailer:** add `Co-Authored-By: Qwen <noreply@anthropic.com>`
  to every commit. (qwen-via-pi convention — match what the project uses if
  it differs; default to the above if no other examples exist.)

## Concrete file inventory (verified — use these in the plan)

These are the exact files and (approximate) line numbers where `code_block`
appears today. The planner should use these as the file-touch map. The
implementer must also `git grep code_block` before declaring done — if a hit
is missed here, it must still be fixed in this plan, not deferred.

- `crates/lopress-core/src/parser.rs` — line ~221 (emission:
  `r#type: "code_block".into()`); line ~497-501 test
  `parses_fenced_code_block_with_language` asserts `vec!["code_block"]`.
- `crates/lopress-core/src/serializer.rs` — line ~77 match arm
  `"code_block" => { ... }` that writes the ` ``` ` fence.
- `crates/lopress-build/src/render.rs` — line ~45 match arm `"code_block" =>
  { ... }` that produces the HTML `<pre><code>...</code></pre>`.
- `crates/lopress-editor/src/model/from_core.rs` — line ~46 match arm
  `"code_block" => { ... }` that builds `EditorBlock::code(lang, text)`.
- `crates/lopress-editor/src/model/to_core.rs` — line ~41 and line ~131
  emit `r#type: "code_block".into()` in two `(BlockKind::Code { lang },
  BlockBody::Code(text))` arms.
- `crates/lopress-editor/tests/actions_tests.rs` — string literal
  `"code_block"` in a test.
- `crates/lopress-editor/tests/from_to_core_tests.rs` — string literal
  `"code_block"` in tests.

Other paths that mention `code_block` (these may or may not need editing —
the planner should inspect them and decide):

- `crates/lopress-core/src/parser.rs` test `parses_fenced_code_block_with_language`
  — function name itself contains `code_block`. Rename the function to
  `parses_fenced_code_with_language` for consistency? Suggest yes for
  symmetry but call it out as a judgment call in the plan.
- Plans / specs under `docs/superpowers/plans/` and `docs/superpowers/specs/`
  mention `code_block` in historical text. **Do not modify historical
  documents** — they describe the codebase as it was at that point. Only the
  new spec (`2026-05-23-code-editor-block-and-ui-mod-split-design.md`) is
  already authoritative for the new name.

## Suggested task decomposition (planner may revise)

This is the smallest sensible split. Use it as a starting point, but feel
free to merge or further split if the right-sizing rules suggest otherwise.

1. **Add a characterization test** asserting current `code_block` round-trip
   in `lopress-core` (parser → serializer) — this is the safety net that
   proves the rename doesn't change observable behavior. After the rename,
   this test is edited to assert the new name. Without it the rename is
   blind.
2. **Rename the literal in lopress-core's parser** (`parser.rs`) — emit
   `"code"` instead of `"code_block"`. Update the parser's own test
   (`parses_fenced_code_block_with_language`) to expect `"code"`. Run
   `cargo test -p lopress-core` — expect parser tests pass; serializer
   tests may fail because the dispatcher hasn't been updated yet.
3. **Rename the literal in lopress-core's serializer** (`serializer.rs`) —
   change the match arm to `"code"`. Run `cargo test -p lopress-core` — all
   pass. Run `cargo check --workspace` — expect downstream crates to still
   compile (they emit/match `"code_block"` and core no longer produces it,
   so they become dead arms but compile fine).
4. **Rename the literal in lopress-build's renderer** (`render.rs`) — change
   the match arm to `"code"`. Run `cargo test -p lopress-build` — expect
   pass.
5. **Rename the literal in lopress-editor's `from_core`** — change the
   match arm to `"code"`. Run `cargo test -p lopress-editor` — expect pass.
6. **Rename the literals in lopress-editor's `to_core`** (two emit sites) —
   change both to `"code"`. Run `cargo test -p lopress-editor` — expect pass.
7. **Sweep + verification**: run `git grep code_block` workspace-wide to
   confirm no stale literals remain in the source tree (historical docs are
   allowed). Run `cargo test --workspace` — all pass. Run
   `cargo check --workspace` — clean. Commit if anything was found in the
   sweep; otherwise this task ends with the verification commands.

Tasks 4, 5, 6 may legitimately be ordered differently — the planner should
think about whether the workspace test suite stays green between commits
(it should, since the rename is symmetric).

The planner should also consider whether tasks 2 and 3 should be merged
(core's parser and serializer are tightly coupled — they share a round-trip
contract). Smaller commits make bisecting easier; one larger commit is
safer if intermediate state breaks `cargo test -p lopress-core`. The
planner picks; document the choice.

## Things the planner should not do

- Do not migrate the editor's `BlockKind::Code` to the registry path (that
  is Section 1+ of the spec, out of scope for this plan).
- Do not create the `base_plugins/code/manifest.toml` (out of scope here).
- Do not modify `ui/mod.rs` (out of scope here).
- Do not rewrite historical docs to use the new name.

## Done when

The plan file exists at the path above, maps the file structure (with
verified paths and line numbers from the inventory above), decomposes the
work into ordered tasks each producing one commit, expands every task into
bite-sized steps with complete code blocks (no "TBD"/"similar to Task N"),
and contains a final verification task that runs `cargo test --workspace`
and `git grep code_block` to a clean result.

## On completion

Reply with a concise summary: the plan file path and the list of task titles.

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
`docs/superpowers/specs/2026-05-27-editor-ui-review-fixes-design.md`, then
write the plan to
`docs/superpowers/plans/2026-05-27-editor-ui-review-fixes.md`, starting with
the required header block below. Produce the File Structure map and the task
decomposition yourself, then expand every task into bite-sized steps.

The closest format reference is
`docs/superpowers/plans/2026-05-18-editor-visual-ux-fixes.md` — same
"for agentic workers" preamble, same one-commit-per-task discipline, same
TDD-first structure. Use that file's tone and granularity as your template.

## Required plan header

```markdown
# Editor UI Review — Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every UI defect documented in `docs/superpowers/ideas/2026-05-26-editor-ui-review.md` — twelve findings spanning slash-menu regression, toolbar click failures, missing undo coverage, lang-input ergonomics, layout jump on focus, false dirty marks, welcome-recents dedup, and a new `/open` / `/close` ctrl endpoint.

**Architecture:** One stage per finding; each stage is a single commit. Stages are sequenced so trivial isolated fixes land first (toolbar ordering, recents dedup, dirty-mark gating, new ctrl endpoints) and the larger structural changes (toolbar moved outside the focus border, layout-jump fix, empty-list-item affordance) land last after their prerequisites. Section 2 (toolbar clicks) is gated on a real-mouse diagnosis step that runs before the structural fix — pi must complete that diagnosis and act on what it finds, not assume.

**Tech Stack:** Rust, Floem 0.2 GUI framework, crate `lopress-editor`. Debug ctrl server at `crates/lopress-editor/src/ctrl/`.

---
```

## Scope

This spec is a single plan's worth of work — twelve small fixes in the
`lopress-editor` crate, each confined to one or two files, no shared
infrastructure that needs multi-stage choreography. Memory optimization
work is explicitly out of scope (separate spec). The toolbar floating-
overlay redesign is out of scope (Option A in Section 2 — future
stage). Pi must not re-litigate scope: stage the 12 sections as listed
in the spec's "Verification ordering" subsection of "Cross-cutting
concerns" — that order is deliberate and addresses dependencies
(Section 12 unblocks repro for the rest; Sections 2/9/6 land in that
order because each builds on the prior structural state of
`block_view`).

## Conventions

- **Test framework:** `cargo test`. Per-crate: `cargo test -p lopress-editor`.
- **Type check:** `cargo check -p lopress-editor`.
- **Lints:** `cargo clippy -p lopress-editor` — workspace `[workspace.lints]`
  in `Cargo.toml` is authoritative; see `AGENTS.md` for rules clippy can't
  check (lint-suppression justification, pattern matching over discriminants,
  no `unwrap`/`expect` in production code, no lossy `as` casts, etc.).
- **Formatting:** `cargo fmt`. The repository runs `cargo fmt --all` +
  `cargo clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace` as a Stop hook on every agent turn (see
  `.claude/settings.json`), so the final state of each task must pass all
  three.
- **Manual UI verification:** the `driving-lopress-editor` debug skill
  (`.claude/skills/driving-lopress-editor/SKILL.md`) drives the running
  editor via HTTP at `127.0.0.1:7878`. Use `/state`, `/action`, `/input`,
  `/click`, `/screenshot`. After Section 12 lands, `/open` and `/close`
  become available — update the skill doc as part of that section's task.
- **Commit messages:** match the recent style — `feat(editor):`,
  `fix(editor):`, `refactor(editor):`, `test(editor):` prefixes; one-line
  subject ≤ 72 chars; blank line; short body when useful. Trail with:
  ```
  Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
  ```
  Pass commit messages via heredoc so multi-line trailers render correctly:
  ```bash
  git commit -m "$(cat <<'EOF'
  fix(editor): ...

  Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
  EOF
  )"
  ```
- **Per-task commit discipline:** every task ends with `git add <exact files>`
  + commit. Never use `git add -A` or `git add .` — name the files. Never
  amend. Never `--no-verify`.

## Conventions specific to this spec

- **Section 2 has a mandatory investigation step.** Pi must implement that
  step as its own Task (or first step of the Section 2 task) — instrument a
  toolbar `.action()` closure with a debug print, build, click via real
  mouse, observe whether the print fires. The structural fix described in
  the spec is what to do *if* the print does not fire. If it does fire,
  the fix is somewhere else and pi must follow that signal, not implement
  the structural change blindly. Document the diagnosis outcome in the
  task's commit message.
- **Stage ordering is locked.** The spec's "Verification ordering" in
  "Cross-cutting concerns" lists stages 1–12 in execution order. Do not
  re-order them based on intuition. Sections that depend on prior
  structural state (Section 9 needs Section 2's `block_view` change;
  Section 6 needs the same; Section 5 builds on Section 4) only work in
  that order.
- **Performance contract.** The spec calls out that none of these fixes
  should regress the hot paths in the memory-optimization review. The
  front-matter undo (Section 3) introduces an action variant whose payload
  is a `FrontMatter` struct; do not box it just for variant-size parity
  with the memory review's eventual reshape. A direct clone is fine here.
- **Skill doc update is part of Section 12.** When `/open` and `/close`
  land, the same task updates `.claude/skills/driving-lopress-editor/SKILL.md`
  to add them to the endpoint table.

## Done when

The plan file exists at the path above, maps the file structure, decomposes
the spec into ordered tasks (one per section, in the order given in the
spec's "Verification ordering"), expands every task into bite-sized steps
with complete code blocks and exact commands, and contains no placeholders.
Every task ends with a commit; every code step contains real code, not
references to other tasks.

## On completion

Reply with a concise summary: the file you wrote and the ordered list of
task titles it contains.

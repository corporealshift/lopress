# Editor Cleanup — Named Attr Decls, Error Surfacing, and File Decomposition

**Date:** 2026-06-04
**Author:** Kyle
**Status:** draft — design review output, pending implementation planning
**Related:**
- `docs/superpowers/specs/2026-06-04-block-descriptor-table-design.md` (magic-string elimination overlaps)
- `reference_attr_form_positional_matching` (memory: the bug class this fixes)
- `reference_floem_text_editor_focuslost` (memory: the textarea commit fragility)

---

## 1. Background

The 2026-06-04 architecture review flagged three cleanup items that are independent of
the big plugin/`BlockKind` refactors and each fix a concrete, already-observed problem
class. They are grouped here because each is small; the planner may split them into
separate plans if convenient. In priority order: **(A) named attr decls**, **(B) error
surfacing**, **(C) `inline_editor.rs` decomposition**.

---

## 2. Item A — Named attr decls (fixes a live bug class)

### Problem
`build_attr_form` pairs plugin attribute **declarations** with their **values by array
index** (`crates/lopress-editor/src/ui/blocks/plugin.rs:111-133`). The code's own
comments admit it: *"we infer name from the attrs map order… key-by-position… workable
for the first version."* It relies on `attr_decls` and the attrs map both being in the
same alphabetical order. When they diverge — a block whose attrs map is incomplete, or
declarations whose order doesn't match the value keys — labels attach to the wrong
fields and edits write the wrong key. This has already bitten (callout/pullquote bodies
saved empty; see the two memory references above).

### Root cause
`AttrDecl` in `crates/lopress-plugin/src/manifest.rs` does **not** carry its own field
name. When the editor collects decls into a `Vec` (`from_core.rs:70`,
`Rc::from(decl.attrs.values().cloned().collect())`), the key is dropped. The form is then
forced to re-associate keys with decls positionally.

### Fix
Carry the name on the decl. Two options for the planner:

1. **Add `name: String` to `AttrDecl`** and populate it when parsing the manifest's
   `[blocks.attrs]` table (the key is the name). `attr_decls` stays a `Vec`/`Rc<[_]>` but
   each element now self-identifies.
2. **Change `attr_decls` to carry `(String, AttrDecl)` pairs** (or an ordered map) so the
   name travels alongside without modifying `AttrDecl`.

Option 1 is cleaner — the decl is self-describing everywhere, not just in the editor.
`build_attr_form` then keys each row by `decl.name` directly; the `names.get(i)`
positional inference (`plugin.rs:123-133`) is deleted, along with the dead loop at
`plugin.rs:110-118`.

### Testing
- **Misaligned-order regression test**: a block whose `attr_decls` order differs from its
  attrs-map insertion order renders each label against the *correct* value and
  `EditAttrs` writes the *correct* key. This is the test that would have caught the
  original bug.
- Update `plugin.rs` `label_tests` and the integration `plugin_block_tests.rs` for the
  new `AttrDecl.name` field.
- Manifest-parse test in `lopress-plugin`: `[blocks.attrs]` keys populate `AttrDecl.name`.

---

## 3. Item B — Error surfacing consistency

### Problem
21 `eprintln!` calls are the de-facto error channel. In a GUI binary with no attached
console they vanish. Two categories, two different fixes:

**B1 — user-facing failures that currently fail silently:**
- Image import failure — `crates/lopress-editor/src/ui/mod.rs:275`
  (`Err(e) => eprintln!("image import failed: {e}")`). The user picks a file, nothing
  happens, no feedback.
- Base-plugin load failure — `crates/lopress-editor/src/state.rs:48-49`.

These should reach the user. The editing state already has a `last_error: Option<String>`
field (`state.rs:36`) and the footer renders a save-error signal (`footer.rs`). Route
user-facing failures there: set `last_error` / a transient status the footer or a toast
surfaces. Image-import failure should show "Couldn't import image: <reason>".

**B2 — developer diagnostics:**
- The `[fallback] …` kind/body-mismatch prints (`mod.rs:128`, `plugin.rs:437`,
  `editor_registry.rs:53,87`) and similar `#[cfg(debug_assertions)]` traces.

These are debugging aids, not user errors. Consolidate them behind one tiny logging
helper rather than raw `eprintln!`, so they're greppable and uniformly formatted. A
minimal `fn debug_log(args)` wrapper in one module (or adopting `log`/`tracing` with an
`env_logger`-style backend gated to debug builds) is sufficient — **do not** over-build a
logging framework (YAGNI). The goal is one consistent call shape, not observability
infrastructure.

### Testing
- B1: a unit/integration test that a failed image import sets `last_error` (inject a
  failing `import_image` or point at a nonexistent path) — asserting the user-visible
  channel is populated, not a console print.
- B2: no test (pure diagnostic plumbing); verify the editor still builds in both
  `debug` and `release`.

---

## 4. Item C — `inline_editor.rs` decomposition (lower priority)

### Problem
`crates/lopress-editor/src/ui/blocks/inline_editor.rs` is 741 LOC — the largest UI file,
carrying three `too_many_arguments` suppressions and the core inline-editing logic
(caret, key handling, run commit, focus). It's at the size where "hold the whole file in
context" gets hard, which the writing-plans guidance calls out as the readability
threshold.

### Fix
After the `BlockEnv` threading spec lands (which removes the arg-count pressure), split
`inline_editor.rs` by responsibility — *not* by layer. Candidate seams (planner to
confirm against the actual code):
- caret / selection math
- key-event handling (the Enter/Backspace/formatting dispatch)
- run commit + the live-signal ↔ model sync

Follow the pattern already used for the `ui/editing/` submodule (`action_sink`,
`save_pipeline`, `undo_redo`, … — small focused files), which is the repo's proven
decomposition style.

### Sequencing
Do this **after** the `BlockEnv` threading spec — that removes the suppressions and
clarifies the natural function boundaries, making the split cleaner. This item is
explicitly the lowest priority of the three and may be deferred.

### Testing
Pure refactor — existing `inline_runs_tests.rs` and `style_span_tests.rs` are the safety
net; no new tests, `cargo test --workspace` stays green.

---

## 5. Housekeeping (non-code, no plan needed)

- **`license = "TBD"`** across the workspace (`Cargo.toml:13`). Resolve before the CI
  spec's `cargo-deny` license gate is meaningful. This is a decision, not an
  implementation task — noted here so it isn't forgotten.

---

## 6. Non-goals

- No new attr **types** or UI hints — Item A fixes the *association*, not the widget set.
- No logging/observability framework — Item B standardizes the call shape only.
- No behavior change in any item; all three are correctness/clarity fixes with
  byte-identical output.

---

## 7. Decisions

### Add `name` to `AttrDecl` rather than patch the form's positional logic
The positional matching is a symptom; the missing name on the decl is the cause. Fixing
the cause makes the decl self-describing for every future consumer, not just the form.

### Surface user-facing errors via the existing `last_error`/footer channel
The plumbing already exists (`state.rs:36`, the footer's save-error signal). Reuse it
rather than introduce a new notification system. A toast is optional polish; setting
`last_error` is the floor.

### Standardize diagnostics, don't build observability
One `debug_log` shape (or `log` crate adoption) is the whole ask. A full `tracing`
subscriber with spans is more than a single-binary desktop GUI at MVP needs — YAGNI.

### `inline_editor.rs` split waits for `BlockEnv`
Splitting before the threading refactor means splitting functions that still carry six
hand-threaded args; after, the boundaries are clean. Order matters.

---

## 8. Open questions for the planner

- **`log` vs. hand-rolled `debug_log`**: adopting the `log` crate (already common, tiny)
  vs. a one-function wrapper. Proposal: `log` + a debug-only backend — standard, and the
  `[fallback]` prints become `log::debug!`. Confirm no release-build cost.
- **Toast vs. footer for image-import error**: footer is guaranteed-present and zero new
  UI; a toast is friendlier but new surface. Proposal: footer/`last_error` now, toast as
  a separate UX follow-up.

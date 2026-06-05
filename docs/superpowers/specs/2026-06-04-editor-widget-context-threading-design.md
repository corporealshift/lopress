# Editor Widget Context Threading — One `BlockEnv`, Zero `too_many_arguments`

**Date:** 2026-06-04
**Author:** Kyle
**Status:** draft — design review output, pending implementation planning
**Related:**
- `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (introduced `EditorContext`)
- `docs/superpowers/specs/2026-06-04-everything-is-a-plugin-and-retire-blockkind-design.md` (shares the same widget signatures)

---

## 1. Background

Ten widget functions in the editor thread the same six-to-eight arguments by hand:
`on_action`, `focus_target`, `focus_pub`, `current_doc`, `on_undo`, `on_redo`, plus
per-widget payload. Each suppresses `clippy::too_many_arguments` with a justification
comment:

- `crates/lopress-editor/src/ui/blocks/inline_editor.rs:138, 199, 409`
- `crates/lopress-editor/src/ui/blocks/code_editor.rs:234`
- `crates/lopress-editor/src/ui/blocks/heading.rs:30`
- … and the dispatchers `block_view` (`mod.rs:55`), `render_body` (`plugin.rs:35`),
  `plugin_block_view` (`plugin.rs:35`).

These six values are a single conceptual unit: **the environment a block widget renders
into.** They're stable for the lifetime of the editing view (the signals are created
once in `editing_view`, `ui/mod.rs:221-247`). Passing them individually is noise that
obscures each function's *actual* inputs (the block, the runs, the lang) and makes adding
a seventh shared dependency a 10-function edit.

The 05-17 spec already introduced the right shape — `EditorContext`
(`editor_registry.rs:20-28`) — but it's used **only** inside the registry. `block_view`,
`render_body`, and every per-block widget still take the fields individually, then the
registry re-bundles them into an `EditorContext` for the two registered widgets. The
abstraction exists but isn't adopted.

---

## 2. Goal

A single `BlockEnv` struct carries the stable per-view dependencies. Every block widget
and dispatcher takes `&BlockEnv` (or `BlockEnv` by value — it's cheap to clone, see §4)
plus its own specific payload. The `too_many_arguments` suppressions are deleted because
no function needs them. Adding a shared dependency is a one-line struct change.

This is a pure readability/ergonomics refactor: no behavior change, no test changes
beyond call-site updates.

---

## 3. The struct

Rename and promote the existing `EditorContext`. Proposed `BlockEnv` (the name
`EditorContext` conflates "context" with the per-block `block` field; the env is the
*block-independent* part):

```rust
/// The block-independent environment every editor widget renders into. Created
/// once per editing view; cloned freely (all fields are Copy signals or Rc).
#[derive(Clone)]
pub struct BlockEnv {
    pub on_action: ActionSink,                       // Rc<dyn Fn(BlockAction)>
    pub focus_target: RwSignal<Option<BlockId>>,     // Copy
    pub focus_pub: FocusPublisher,                   // Copy (signals inside)
    pub current_doc: RwSignal<Option<EditorDoc>>,    // Copy
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}
```

`EditorContext` becomes a thin pairing of `&BlockEnv` + `&EditorBlock` at the registry
boundary (or is dropped entirely in favor of passing both):

```rust
pub type EditorWidget = fn(&EditorBlock, &BlockEnv) -> AnyView;
```

---

## 4. Cheap to clone, so pass by value where it reads better

Every field is either a `Copy` floem signal or an `Rc`. Cloning `BlockEnv` is a handful
of atomic refcount bumps — no deep copy. Per the repo's "don't clone to appease the
borrow checker" rule (`AGENTS.md:91`), this clone is *semantically* justified: widgets
genuinely need an owned handle to capture into floem closures (the existing code already
clones `on_action`, `on_undo`, etc. individually for exactly this reason). Bundling them
means one `env.clone()` replaces six individual `.clone()` calls — strictly fewer clones,
not more.

Guideline for the planner: dispatchers take `&BlockEnv` and clone once when handing to a
widget closure; leaf widgets take `BlockEnv` by value.

---

## 5. Migration shape

Mechanical, one widget at a time, each independently testable (the editor still compiles
and renders after each):

1. Define `BlockEnv` in `editor_registry.rs` (or a new `ui/blocks/env.rs`).
2. Construct it once in `editing_view` (`ui/mod.rs`) from the existing signals, replacing
   the six individual values passed into `editor_pane`.
3. Thread `BlockEnv` through `editor_pane` → `block_view` → `wrap_block` /
   `render_body` / `plugin_block_view`.
4. Convert each leaf widget (`paragraph`, `heading`, `code_editor`, `list`, `table`,
   `image`, etc.) to take `(block_or_payload, &BlockEnv)`; delete its
   `#[allow(clippy::too_many_arguments)]`.
5. Delete the now-redundant `EditorContext` re-bundling in the registry.

Each step keeps clippy green (the suppression is removed only when the arg count actually
drops below the threshold).

---

## 6. Interaction with the other specs

- Shares the exact widget signatures the everything-is-a-plugin spec reshapes
  (paragraph/heading → `EditorWidget`). **Do this spec first**, or do it as part of
  Stage A: migrating paragraph/heading to `EditorWidget` is cheaper if `EditorWidget`
  already takes `&BlockEnv`. Recommended: land `BlockEnv` first as a standalone
  refactor, then the plugin migration inherits the clean signature.
- Independent of the descriptor table — touches dispatch *plumbing*, not block
  *identity*.

---

## 7. Testing

This is a refactor with no behavior change, so the existing suite is the safety net:

- `cargo test --workspace` stays green throughout (the model/action tests don't touch
  widget signatures; the integration tests exercise rendering indirectly).
- No new unit tests are warranted — there is no new logic, only re-parameterization.
  Adding tests here would violate YAGNI.
- Manual/e2e: the editor renders and edits identically; verify once via the control
  server after the last widget migrates (open a doc, focus each block type, confirm
  toolbar + editing work).

---

## 8. Non-goals

- No change to what the widgets *do*.
- No new shared dependencies added to `BlockEnv` in this spec — only the existing six are
  bundled. (The struct makes future additions cheap, but YAGNI: add them when needed.)
- Not a file-decomposition pass on `inline_editor.rs` (741 LOC) — that's noted as a
  follow-up in the misc-cleanup spec, separate from this threading change.

---

## 9. Decisions

### Bundle the six stable deps into `BlockEnv`, pass payload separately
The block-dependent input (`block`, `runs`, `lang`) varies per widget and stays an
explicit argument; the block-*independent* environment is the part that's identical
everywhere and belongs in a struct. Splitting on that axis is why `block` is *not* a
field of `BlockEnv` (unlike the old `EditorContext`).

### Rename `EditorContext` → `BlockEnv`
The old name implied it held the block ("context" of what). The promoted struct is
explicitly the block-independent env; the new name says so. `EditorWidget` takes
`(&EditorBlock, &BlockEnv)` so the two axes are visible at the boundary.

### Land before the plugin migration
The everything-is-a-plugin Stage A reshapes paragraph/heading to `EditorWidget`. If
`EditorWidget` already takes `&BlockEnv`, that reshape is trivial; if not, it's reshaped
twice. Order accordingly.

---

## 10. Open questions for the planner

- **Struct location**: `editor_registry.rs` vs. a dedicated `ui/blocks/env.rs`. Proposal:
  dedicated module, since both the registry and `block_view` import it and it's no longer
  registry-specific.
- **By-value vs. by-ref at the dispatcher boundary**: confirm `block_view` takes
  `&BlockEnv` (it fans out to multiple children, each needing a clone) — measure nothing,
  it's refcount-cheap either way; pick the one that reads cleanest.

# Editor Performance & Instrumentation — Design Spec

**Date:** 2026-05-19
**Author:** Kyle
**Status:** Approved — ready for implementation planning
**Related:** `crates/lopress-editor`; `crates/lopress-gui-host/src/session.rs`; `crates/lopress-build`; `crates/lopress-core`

---

## Goal

Make workspace open feel instant, fix the one unambiguous editing-path waste, and leave behind reusable timing instrumentation so future performance regressions are caught by measurement, not vibes.

---

## Opportunity Survey

The user reported two symptoms: *"the editor loads slowly when you open a workspace"* and *"the editor feels slow."* A full sweep of the platform turned up eight opportunities. They are not all in scope — they are listed here so the spec records the survey.

| # | Description | Scope |
|---|-------------|-------|
| **O1** | `Session::open` runs a full static-site build synchronously before the editor window appears. | In scope (Phase 2) |
| **O2** | `Session::open` starts the preview server synchronously on the open path. | In scope (Phase 2) |
| **O3** | `scan_workspace` re-reads and fully parses every `.md` file just for titles/draft flags, redundant with the build that just parsed them. | **Out of scope** — trivially fixable via `frontmatter::split`, but per-doc savings at <20 docs are sub-ms; not worth a plan task. Documented as a known minor inefficiency. |
| **O4** | `on_action` clones the entire `EditorDoc` for the undo snapshot on every action, including each committed text run. `UndoStack::push_before_apply` already takes `&EditorDoc` by reference and `compute_inverse` only clones the affected block — the full-doc clone is pure waste. | In scope (Phase 3) |
| **O5** | Structural edits trigger a full editor-pane rebuild via the `dyn_container` keyed on `pane_key`, tearing down and recreating every block widget. | Conditional (Phase 3) — only attack if measured numbers justify. |
| **O6** | Each debounced save triggers a full `session.rebuild()` (background-threaded, but still kicks off a full site rebuild every 500 ms after edits). | Conditional (Phase 3) — only attack if measured numbers justify. |
| **O7** | The `#[cfg(debug_assertions)]` control-server `create_effect` serializes the entire doc to JSON on every `current_doc` change. | Conditional (debug-only) — zero impact on end users. Phase 3 measures; fix is optional. |
| **O8** | Debug-build codegen overhead. | Not a fix — documented guidance: evaluate editing feel on `cargo run --release`. |

**Context:** The user runs a small workspace (<20 docs) and has been judging slowness on debug builds. Deferring both the build and the serve start (with progress shown in the UI) is acceptable.

---

## Phase 1 — Timing Instrumentation (Foundation)

A small, dependency-free timing facility. New module `perf` in **`lopress-core`** (the one crate that both `lopress-build` and `lopress-editor` already depend on).

### API

A scope guard:

```rust
let _t = lopress_core::perf::span("workspace.open.build");
```

When the guard drops, it records the elapsed time.

### Gating

Off unless the `LOPRESS_TIMING` environment variable is set (any non-empty value). The check is resolved once into a cached `bool` (e.g. via `OnceLock<bool>` or `AtomicBool`). When off, `span()` returns a no-op guard holding no `Instant` — effectively free.

### Output

When enabled, prints one flat line per span on drop:

```
[timing] workspace.open.build: 142ms
```

Dotted span names (`workspace.open.*`, `editor.on_action`, `editor.save.*`) give implicit grouping in the log. No aggregation tree, no nesting machinery — flat lines are sufficient to spot regressions and match the codebase's existing `eprintln!` style.

### Dependencies

No new dependencies — `std::time` only.

### Spans instrumented

These are exactly the spans Phase 3 needs to capture a release baseline:

- `Session::open`: `workspace.open.workspace_load`, `workspace.open.scan`, `workspace.open.initial_build` (recorded in the background thread), `workspace.open.serve_start` (recorded in the background thread).
- `EditingState::open_document`: `editor.open_document.load_parse`, `editor.open_document.from_core`.
- `on_action` total: `editor.on_action`.
- Debounced save: `editor.save.serialize`, `editor.save.write`.

---

## Phase 2 — Deferred Workspace Open

`Session::open` becomes fast. It performs only `Workspace::load` and `scan_workspace`, spawns a single background thread for the rest, and returns. The editor window appears immediately.

### Background thread

Runs, in order:

1. Initial build → update `build_status` (`Building` → `Ok` / `Failed`).
2. Start the preview server → update `serve_status` (`Starting` → `Listening` / `Unavailable`).

### Type changes

In `crates/lopress-gui-host/src/session.rs`:

- `Session::serve_status` becomes `Arc<Mutex<ServeStatus>>` (currently a plain field).
- `ServeStatus` gains a `Starting` variant so the footer can distinguish "still booting" from `Unavailable { reason }`.
- Initial `build_status` flips from `Idle` to `Building` so the footer shows it from the first frame.
- `Session::serve_status()` returns a `ServeStatus` clone (not a reference) since the value now lives behind a mutex.

### UI changes

In `crates/lopress-editor/src/ui/`:

- Footer already polls `build_status` reactively via `start_build_status_poll` — no change needed for the build side.
- Footer's `serve_url_str` is currently computed once at view-build time; it becomes a reactive signal mirrored from `serve_status`, by adding a near-clone of `start_build_status_poll` for `ServeStatus`. The preview link/button is disabled (or shows "Starting preview…") while the status is `Starting`.

---

## Phase 3 — Editing Fixes + Release Baseline

### Fix O4 (concrete, deterministic)

In `crates/lopress-editor/src/ui/mod.rs`, in the `on_action` closure, replace the full-doc clone and outer `if let` with a nested closure:

```rust
current_doc.with_untracked(|maybe| {
    if let Some(d) = maybe {
        undo_stack.update(|s| s.push_before_apply(d, &action));
    }
});
```

This eliminates one `O(doc-size)` allocation and copy per every committed edit, including each text-run commit. Real release-build win.

### Capture the release baseline

With Phase 1 instrumentation in place:

1. Build with `cargo run --release`.
2. Set `LOPRESS_TIMING=1`.
3. Exercise the four interaction categories the user flagged — typing latency, document switching, structural edits (Enter / split / change type / drag-reorder), scrolling and general UI.
4. Capture the numbers.
5. Write findings to `docs/superpowers/plans/2026-05-19-performance-findings.md` (a sibling-doc, not part of this spec).

### Decide on O5, O6, O7 from the numbers

- **O5** (full pane rebuild on structural edits) — only attack if the measured `editor.on_action` for structural actions is perceptibly slow in release. Fixing it means restructuring the pane's `dyn_container` into a stable per-block reactive scheme; sizable work.
- **O6** (every save triggers a full site rebuild) — only attack if the rebuild on a typical save adds perceptible latency in release. Likely fine on a small workspace.
- **O7** (debug-only doc-to-JSON snapshot on every edit) — only attack if it shows up in the debug run; zero impact on end users so optional.

The Phase 3 deliverable is the O4 fix **plus** a written findings doc that either justifies further work or closes out these items.

### O8 — Documented guidance on debug vs release

Editing-feel evaluation should be done on `cargo run --release`. Debug-build sluggishness is mostly unoptimized codegen and the debug-only control snapshot; end users get release builds.

---

## Testing Strategy

### Phase 1

Unit tests for `perf::span` — disabled by default (no allocation when off); enabled path records non-zero durations for a span that sleeps a known interval. No tests on log output text (over-couples to the print format).

### Phase 2

A test confirms `Session::open` returns before the initial build completes — i.e., `build_status` is `Building` immediately after `open` returns and transitions to `Ok` only later. All existing build-and-scan tests in `lopress-gui-host` and `lopress-build` continue to pass unchanged.

### Phase 3 (O4)

Existing undo tests in `crates/lopress-editor` must pass unchanged — the inverse-computation path is untouched. The fix is a no-op for correctness; the regression risk is that the closure-nesting borrows are valid. A code-review-time check is sufficient; no new test needed beyond confirming undo continues to work.

---

## Risks and Non-goals

### Non-goals

- Rewriting the editor's reactive structure (O5) speculatively. Only if measured.
- Changing the build cache, the watcher, or any plugin code.
- Introducing a `tracing` dependency or any other crate.
- Fixing O3 (sub-ms at current scale).

### Risks

- **Deferring the preview server** means the URL is briefly unavailable. Accepted — footer surfaces `Starting preview…` until the server binds.
- **Env-var-gated timing might be silently disabled in CI.** Acceptable — CI does not exercise editor timings; the module docstring should note this.

---

## Resolved Decisions and Tradeoffs

### 1. Approach scope: A (open-path only) vs B (open + measured editing fixes) vs C (full sweep)

**Chosen: B.** Honors "consider every part of the platform" by surveying everything, but spends implementation effort only where release users actually feel it. Adds reusable instrumentation so future regressions are caught by measurement.

- **Rejected A:** ignores real editing-path waste (O4) that's an easy concrete win.
- **Rejected C:** speculatively rebuilds the pane-diffing machinery (O5) for gains that may never be perceived at <20 docs and small documents.

### 2. Initial build on workspace open: defer-to-background with progress vs blocking vs defer-with-progress including serve

**Chosen: defer both the build and the preview server, with progress shown in the footer.** User explicitly asked for this combination.

- **Rejected blocking:** keeps the synchronous slowness this work is meant to fix.
- **Rejected build-only defer (leave serve blocking):** serve start has its own non-trivial cost (socket bind, address-fallback retry); deferring both keeps `Session::open` symmetric and fast.

### 3. Instrumentation home: `lopress-core` vs a new crate vs `tracing`

**Chosen: a tiny `perf` module in `lopress-core`.** Both `lopress-build` and `lopress-editor` already depend on `lopress-core`, so no graph changes. Zero new deps. Trivial to swap for `tracing` later if needs grow.

- **Rejected new crate:** adds workspace member overhead for ~100 lines of code.
- **Rejected `tracing`:** too much setup (subscribers, layers) for a solo project; deferred.

### 4. Instrumentation output: flat lines vs nesting tree vs aggregated histograms

**Chosen: flat lines, dotted names for implicit grouping.** Matches existing `eprintln!` style; sufficient to spot regressions.

- **Rejected nesting tree / histograms:** YAGNI — costs implementation effort and runtime complexity for value not needed at this stage.

### 5. Editing feel on debug vs release

**Chosen: document that editing-feel evaluation is a release-build activity (O8 guidance), and target Phase 3 fixes at things measurable in release.**

- **Rejected:** chase debug-build symptoms without distinguishing. Risks fixing debug-only overhead (O7, codegen) that doesn't affect end users.

### 6. O5 / O6 / O7: pre-decide vs measure-then-decide

**Chosen: measure-then-decide.** The spec defines what gets measured, how, and where findings are written. The conditional fixes are not pre-committed.

- **Rejected pre-decide all fixes:** risks speculative rework. Especially O5, which is a large change to the Floem reactive structure.

### 7. O3 (`scan_dir` double-parse): include vs exclude

**Chosen: exclude.** Trivially fixable but sub-ms at <20 docs; honestly not worth a plan task. Documented in the spec as known minor inefficiency so it isn't forgotten.

- **Rejected include:** plan-task overhead exceeds the savings at current workspace scale.

---

## Open questions for Claude

None identified. All design decisions in this spec are settled.

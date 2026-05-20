# Editor Performance Findings — 2026-05-19

**Spec:** [`docs/superpowers/specs/2026-05-19-editor-perf-and-instrumentation-design.md`](../specs/2026-05-19-editor-perf-and-instrumentation-design.md)
**Plan:** [`docs/superpowers/plans/2026-05-19-editor-perf-and-instrumentation.md`](2026-05-19-editor-perf-and-instrumentation.md)
**Status:** Deferred — implementation tasks landed; the measurement pass and O5/O6/O7 decisions were not pursued. Revisit only if a performance regression is suspected. The protocol and table below remain populated as a ready-to-use template for that future run.

## Implementation status

Tasks 1–7 of the plan are complete and committed on branch `fix-and-measure-performance`:

| Task | Subject | Result |
|------|---------|--------|
| T1 | `perf::span` foundation in `lopress-core` | ✓ landed |
| T2 | Spans in `Session::open` and `Session::save` | ✓ landed |
| T3 | Spans in editor `open_document` and `on_action` | ✓ landed |
| T4 | `ServeStatus::Starting` + `Arc<Mutex<>>` refactor | ✓ landed |
| T5 | Deferred build + serve to background thread | ✓ landed (+ side fixes from review: integration tests now poll, server-lock no longer held across `broadcast_reload`) |
| T6 | Reactive serve-status polling in footer | ✓ landed |
| T7 | O4 fix — full-doc clone in `on_action` eliminated | ✓ landed |

Release build verified: `cargo build --release --workspace --lib` finishes clean (1m 25s cold). Tests/clippy/fmt clean across the workspace.

## How to run the measurement

Build the release binary and launch it against a real workspace with timing enabled.

On Windows PowerShell:

```powershell
$env:LOPRESS_TIMING = "1"
cargo run --release -- <path-to-workspace> 2>timing.log
```

On bash:

```bash
LOPRESS_TIMING=1 cargo run --release -- <path-to-workspace> 2>timing.log
```

Then exercise each interaction below, capturing the `[timing] ...` lines that print to stderr. The four categories the user originally flagged:

1. **Workspace open** — capture `workspace.open.workspace_load`, `workspace.open.scan`, `workspace.open.initial_build`, `workspace.open.serve_start` from startup. The first three appear before the editor window has any content; `initial_build` and `serve_start` now run on a background thread, so the window itself appears before they print.
2. **Document open** — click between two docs in the sidebar; capture `editor.open_document.load_parse` and `editor.open_document.from_core`.
3. **Typing latency** — focus a paragraph block and type ~100 characters. Each committed run prints `editor.on_action`. Subjective: does typing feel rubbery, lag behind keystrokes, or land instantly?
4. **Structural edits** — Enter-to-split, change block type via toolbar, drag-reorder. Each is one `editor.on_action`. Subjective: does the new block / new type land instantly?
5. **Save** — let the debounced save fire after a burst of edits. Captures `editor.save.serialize`, `editor.save.write`. The save also triggers `session.rebuild()` on a background thread — this is the O6 candidate.
6. **Scroll / general UI** — scroll through a medium document; observe by eye. No spans cover scrolling itself; this is a pure feel assessment.

## Observed timings

Fill in once measurement is run. Use `min / typical / max` over ~10 samples per row.

| Span | Min | Typical | Max | Notes |
|------|-----|---------|-----|-------|
| `workspace.open.workspace_load` | | | | |
| `workspace.open.scan` | | | | |
| `workspace.open.initial_build` | | | | bg thread |
| `workspace.open.serve_start` | | | | bg thread |
| `editor.open_document.load_parse` | | | | |
| `editor.open_document.from_core` | | | | |
| `editor.on_action` — inline edit | | | | per text-run commit |
| `editor.on_action` — structural | | | | split / change-type / drag |
| `editor.save.serialize` | | | | |
| `editor.save.write` | | | | |

## Subjective feel

Fill in once exercised on release. One short paragraph each.

- **Typing latency:**
- **Document switching:**
- **Structural edits:**
- **Scrolling / general UI:**

## Recommendations on conditional opportunities

The plan deferred these to a measurement-then-decide call. Fill in once numbers are in hand.

### O5 — Full editor-pane rebuild on structural edits

The pane's `dyn_container` rebuild is keyed on the shape of `current_doc.blocks` (id sequence + kind tag + plugin presence), so any structural edit tears down and recreates every block widget. Cost is `O(doc-size)` per structural action.

**Decision: pursue / defer / close.**

Reasoning from observed `editor.on_action` (structural): _<fill in>_

### O6 — Every save triggers a full site rebuild

The debounced save fires `session.rebuild()`, which kicks off a background `lopress_build::build` walk of the whole workspace. The build is already incremental (per-page hashes in `lopress-build/src/cache.rs`), so a no-change rebuild only pays hash-check cost — but it's still a full walk per save.

**Decision: pursue / defer / close.**

Reasoning from observed save + rebuild cadence: _<fill in>_

### O7 — Debug-only doc-to-JSON snapshot on every edit

`crates/lopress-editor/src/ui/mod.rs` has a `#[cfg(debug_assertions)]` `create_effect` that serializes the entire document to JSON on every `current_doc` change so the debug control server on `127.0.0.1:7878` can read state. Zero impact on end users (release builds disable it), but it inflates `editor.on_action` durations observed in the debug-driven workflow.

**Decision: pursue / defer / close.**

Reasoning: _<fill in. If you only judge the editor through the debug control workflow, fixing this matters; if you do feel-checks on release builds, leave it.>_

## Overall

Implementation tasks T1–T7 landed and judged sufficient at this stage. The visible win — workspace open returning immediately with the footer surfacing build/serve progress — plus the O4 clone fix moved subjective feel far enough that a formal measurement pass wasn't worth the effort right now. The instrumentation (`perf::span` + the run-it protocol above) is in place to revisit quickly if anything regresses.

O5, O6, and O7 are explicitly not closed — just not pursued. Re-open this doc and run the measurement protocol if needed.

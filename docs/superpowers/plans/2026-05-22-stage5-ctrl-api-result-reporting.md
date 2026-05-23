# Stage 5 — Ctrl API Test Coverage + `/action` Result Reporting

> **For the implementer (qwen):** execute this plan task-by-task in order. You have full git and the cargo toolchain — commit per task, run the verification suite before each commit, and report back when all five are done. Treat me as a senior reviewer on call: if a *characterization* test fails or a snippet here doesn't match the file you find, stop and report rather than improvising the production code.

**Goal:** Pin the existing `CtrlAction → BlockAction` translation with characterization tests, and change `POST /action` so it reports whether the action actually reached a real block instead of always answering `ok`.

**Architecture:** The debug control server (`crates/lopress-editor/src/ctrl/mod.rs`, `#![cfg(debug_assertions)]`) receives JSON `CtrlAction` requests on `POST /action`, forwards them over a crossbeam channel to the Floem UI thread, which translates each to a `BlockAction` and runs it through the `on_action` chokepoint. Today the HTTP handler answers `ok` 200 immediately and never learns the outcome. We change the channel payload from `CtrlAction` to `(CtrlAction, Sender<CtrlActionResult>)` so the UI thread can ship the real outcome back to a blocked HTTP handler — `dispatched` / `no_document` / `block_not_found`. The handler then maps that to a status code + JSON body.

**Tech stack:** Rust, `crossbeam-channel`, `tiny_http`, `serde_json`, Floem reactive signals.

---

## Background — read before starting

You are working in the `lopress` workspace; the relevant crate is `lopress-editor`. The branch is `feat/edit-block-body`, which is wrapping up a multi-stage editor refactor. Stages 1–4 are committed; this is the final stage.

### The translation layer is correct — don't modify it

`CtrlAction::into_block_action` (around `ctrl/mod.rs:69`) already translates all eight verb-shaped requests, including `EditInline → EditBlockBody{Inline}` and `EditCode → EditBlockBody{Code}`. Stage 3 reworked it. **Don't touch its body in this plan.** Task 1 only writes tests against it. If a characterization test you write *fails*, that is a real translation bug — stop and report it rather than editing `into_block_action` to make the test green.

Its only failure mode is an unknown block id (every arm uses `find(doc, block_id)?`), so `None` from it always means "block not found".

### Useful facts you'll rely on

- `BlockId::new()` mints a fresh monotonic id; `BlockId::raw()` returns the `u64`.
- `EditorBlock::paragraph(runs)` / `heading(level, runs)` / `code(lang, text)` / `list(ordered, items)` are constructors at `crates/lopress-editor/src/model/types.rs:122` onward; each mints a fresh `BlockId` you read via `block.id`.
- `EditorDoc { blocks, front_matter }` — `front_matter` is `lopress_core::FrontMatter::default()`.
- `InlineRun::plain("text")` builds a plain run. `InlineRun` is `Debug + Clone + PartialEq`.
- `BlockAction` is `#[derive(Debug, Clone)]` only — **no `PartialEq`**. Pattern-match it with `match`; don't `assert_eq!` it.
- `BlockKind` *is* `PartialEq` — you may `assert_eq!` it.
- `BlockAction`, `BlockBody`, `BlockKind`, `InlineRun`, `EditorDoc` are all already imported at the top of `ctrl/mod.rs`, so `use super::*;` inside the test module brings them into scope. `EditorBlock` needs an explicit import.
- The file starts with `#![cfg(debug_assertions)]`, so the whole module — and any `#[cfg(test)] mod tests` inside it — compiles only in debug. `cargo test` builds debug, so the test module runs normally.

### The new `/action` HTTP contract (the deliverable)

| Outcome                                       | Status | Body                                                          |
|-----------------------------------------------|--------|---------------------------------------------------------------|
| Translated + routed to `on_action`            | 200    | `{"status":"dispatched"}`                                     |
| No document is open                           | 409    | `{"status":"no_document","detail":"no document is open"}`     |
| Block id not present in the open doc          | 422    | `{"status":"block_not_found","block_id":N}`                   |
| Reply channel timeout (>2 s)                  | 504    | `editor did not respond` (plain text, existing helper)        |
| Editor receiver closed                        | 503    | `editor channel closed` (plain text, existing helper)         |
| JSON parse error (unchanged)                  | 400    | `parse error: …` (plain text, existing helper)                |

"Dispatched" means **routed to a real block** — it does *not* guarantee the document changed. A no-op like `Move` to the current index still counts. Reporting actual mutation would require threading `apply`'s return up through `ActionSink` and is out of scope.

### Per-task verification

Run all four from the workspace root (`C:\Users\corpo\Documents\projects\lopress`) and confirm clean before committing:

```
cargo build -p lopress-editor
cargo test -p lopress-editor
cargo clippy -p lopress-editor --all-targets -- -D warnings
cargo fmt --all -- --check
```

On Windows PowerShell, cargo's stderr gets wrapped in a cosmetic `NativeCommandError` — ignore that; only the actual `error[...]` / `warning:` lines and the final `test result:` / `Finished` lines matter. Task 4 additionally requires a full-workspace `cargo build` (no `-p` filter) to catch any `lib.rs` type-inference breakage.

### Commit style

The branch's commits are short, conventional, lowercase, scoped. Examples from the recent log:

```
fix(editor): correct undo for list edits, drop EditBlockBody coalescing
refactor(editor): delete EditListItem / SplitListItem / MergeListItemWithPrev
feat(editor): add EditBlockBody action variant for shape-agnostic body swaps
```

One commit per task; suggested messages are at the bottom of each task.

---

## Task 1: Characterization tests for `CtrlAction::into_block_action`

Append a `#[cfg(test)] mod tests` block at the very end of `crates/lopress-editor/src/ctrl/mod.rs` (the file currently ends at the `#[cfg(not(target_os = "windows"))] fn screenshot()` stub). Cover every variant and arm. These tests must pass on first run — they pin existing correct behavior.

### Module header

Tests assert with `panic!` / `unwrap` in places, which the workspace lints would flag — silence them on this module:

```rust
// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::types::EditorBlock;

    // helper + tests here
}
```

### Fixture

```rust
fn doc_one_paragraph() -> (EditorDoc, u64) {
    let block = EditorBlock::paragraph(vec![InlineRun::plain("text")]);
    let raw = block.id.raw();
    let doc = EditorDoc {
        blocks: vec![block],
        front_matter: lopress_core::FrontMatter::default(),
    };
    (doc, raw)
}
```

### Coverage required (one test each unless noted)

1. `EditInline` → `BlockAction::EditBlockBody { new_body: BlockBody::Inline(runs) }`; assert runs preserved.
2. `EditCode` → `BlockAction::EditBlockBody { new_body: BlockBody::Code(text) }`; assert text preserved.
3. `Split { byte_offset }` → `BlockAction::Split { byte_offset, new_block_id: None }`. The `new_block_id` must be `None` — ctrl must never pre-mint ids.
4. `MergeWithPrev` → `BlockAction::MergeWithPrev`.
5. `Delete` → `BlockAction::Delete`.
6. `Move { to_index }` → `BlockAction::Move { to_index }`; assert to_index preserved.
7. `EditAttrs { new_attrs }` → `BlockAction::EditAttrs`; assert the `serde_json::Map` round-trips equal.
8. `ChangeType` — one test exercising each `CtrlBlockKind` variant in a loop and asserting the mapped `BlockKind`: `Paragraph → Paragraph`, `Heading { level: 2 } → Heading(2)`, `Code { lang: "rust" } → Code { lang: "rust" }`, `List { ordered: true } → List { ordered: true }`.
9. `ChangeType { Heading { level } }` clamping — separate test: level 9 clamps to 6, level 0 clamps to 1. (See the existing `level.clamp(1, 6)` in `into_block_action`.)
10. Unknown id (`u64::MAX`, which `BlockId::new()` never mints) → `into_block_action` returns `None`.

### Worked example (use as a pattern for the rest)

```rust
#[test]
fn edit_inline_translates_to_edit_block_body_inline() {
    let (doc, raw) = doc_one_paragraph();
    let ctrl = CtrlAction::EditInline {
        block_id: raw,
        new_runs: vec![InlineRun::plain("new")],
    };
    match ctrl.into_block_action(&doc).expect("known id translates") {
        BlockAction::EditBlockBody {
            block_id,
            new_body: BlockBody::Inline(runs),
        } => {
            assert_eq!(block_id.raw(), raw);
            assert_eq!(runs, vec![InlineRun::plain("new")]);
        }
        other => panic!("expected EditBlockBody/Inline, got {other:?}"),
    }
}
```

**Done when:** all ten tests pass on first `cargo test -p lopress-editor`; the four-command suite is clean; one commit.

Suggested message: `test(editor): characterize CtrlAction::into_block_action translations`

---

## Task 2: `CtrlAction::block_id()` accessor

Task 4 needs to report *which* block id a translation failed on; every `CtrlAction` variant carries one, so a uniform accessor is the cleanest path.

In `ctrl/mod.rs`, inside `impl CtrlAction { ... }` immediately after `into_block_action`'s closing brace, add:

```rust
/// The raw `u64` block id this action targets. Every variant carries
/// one. Used to report which block was missing when translation fails.
pub(crate) fn block_id(&self) -> u64 {
    match self {
        CtrlAction::Split { block_id, .. }
        | CtrlAction::MergeWithPrev { block_id }
        | CtrlAction::Delete { block_id }
        | CtrlAction::Move { block_id, .. }
        | CtrlAction::ChangeType { block_id, .. }
        | CtrlAction::EditInline { block_id, .. }
        | CtrlAction::EditCode { block_id, .. }
        | CtrlAction::EditAttrs { block_id, .. } => *block_id,
    }
}
```

Add one test in `mod tests` that constructs each of the eight variants with a *distinct* `block_id` value (1..=8 is fine) and asserts `.block_id()` returns each. Distinct values catch a slipped match arm; uniform values wouldn't.

**Done when:** test passes; four-command suite clean; one commit.

Suggested message: `feat(editor): add CtrlAction::block_id accessor`

---

## Task 3: `CtrlActionResult` + HTTP response mapping

A small enum describing the outcome of routing an action, plus a pure method that maps it to `(u16, String)`. Standalone and fully unit-testable — no channel wiring yet.

Insert this **immediately after** the closing brace of `impl CtrlAction { ... }` and **before** the `// ── Doc state serialization ──` divider:

```rust
// ── Action result (HTTP API) ──────────────────────────────────────────────────

/// Outcome of routing a `CtrlAction`, reported back to the blocked HTTP
/// handler so the caller learns whether the action reached a real block.
///
/// `Dispatched` means the action named an existing block and was routed to
/// the editor's `on_action` chokepoint. It does **not** guarantee the
/// document changed — a no-op action (e.g. `Move` to the same position)
/// still counts as dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtrlActionResult {
    Dispatched,
    NoDocument,
    BlockNotFound { block_id: u64 },
}

impl CtrlActionResult {
    /// HTTP status code and JSON body to return for this outcome.
    pub(crate) fn http_response_parts(&self) -> (u16, String) {
        match self {
            CtrlActionResult::Dispatched => {
                (200, serde_json::json!({ "status": "dispatched" }).to_string())
            }
            CtrlActionResult::NoDocument => (
                409,
                serde_json::json!({
                    "status": "no_document",
                    "detail": "no document is open",
                })
                .to_string(),
            ),
            CtrlActionResult::BlockNotFound { block_id } => (
                422,
                serde_json::json!({
                    "status": "block_not_found",
                    "block_id": *block_id,
                })
                .to_string(),
            ),
        }
    }
}
```

Add three tests in `mod tests`, one per variant. For each: call `http_response_parts()`, assert the status code, and assert the body contains the expected status string (and, for `BlockNotFound`, the id rendered as a number).

**Done when:** tests pass; four-command suite clean; one commit.

Suggested message: `feat(editor): add CtrlActionResult with HTTP response mapping`

---

## Task 4: Wire the reply channel end-to-end

The biggest task. Changes the type carried by the `/action` channel from `CtrlAction` to `(CtrlAction, Sender<CtrlActionResult>)`, so the workspace will not compile until **both** producer (`ctrl/mod.rs`) and consumer (`ui/mod.rs`) are updated. Apply all edits in `ctrl/mod.rs` first, then all edits in `ui/mod.rs`, then run `cargo build` once.

### Producer side: `crates/lopress-editor/src/ctrl/mod.rs`

**1. Add the envelope alias** — immediately after the `impl CtrlActionResult { ... }` block from Task 3:

```rust
/// What travels the `/action` channel: the parsed action plus a one-shot
/// reply sender the UI thread uses to report the outcome back to the
/// blocked HTTP handler.
pub(crate) type CtrlActionEnvelope = (CtrlAction, Sender<CtrlActionResult>);
```

`Sender` is already imported via `use crossbeam_channel::Sender;`.

**2. Thread `CtrlActionEnvelope` through the channel API.** Wherever the channel's element type appears as `CtrlAction`, change to `CtrlActionEnvelope`:

- `start()`'s return type: `crossbeam_channel::Receiver<CtrlActionEnvelope>`
- the turbofish in `crossbeam_channel::unbounded::<CtrlActionEnvelope>()`
- `serve`'s `action_tx: Sender<CtrlActionEnvelope>` parameter
- `handle_request`'s `action_tx: &Sender<CtrlActionEnvelope>` parameter

**3. Rewrite the `POST /action` arm.** Find the current arm (today around `ctrl/mod.rs:253–265`, body is `let _ = action_tx.send(action); text_response("ok", 200)`) and replace its whole match arm with:

```rust
("POST", "/action") => {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return text_response("read error", 400);
    }
    match serde_json::from_str::<CtrlAction>(&body) {
        Ok(action) => {
            // Round-trip: hand the action to the UI thread with a
            // one-shot reply channel, then block until it reports
            // the outcome (or 2 s elapse). The serve loop is
            // single-threaded, so other endpoints wait during this
            // window — acceptable for a debug tool, and the UI
            // normally answers within a frame.
            let (reply_tx, reply_rx) =
                crossbeam_channel::bounded::<CtrlActionResult>(1);
            if action_tx.send((action, reply_tx)).is_err() {
                return text_response("editor channel closed", 503);
            }
            match reply_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(result) => {
                    let (code, json) = result.http_response_parts();
                    tiny_http::Response::from_string(json)
                        .with_header(json_header())
                        .with_status_code(code)
                }
                Err(_) => text_response("editor did not respond", 504),
            }
        }
        Err(e) => text_response(&format!("parse error: {e}"), 400),
    }
}
```

### Consumer side: `crates/lopress-editor/src/ui/mod.rs`

**4. Update the three receiver type annotations.** Search for `crossbeam_channel::Receiver<crate::ctrl::CtrlAction>` — there are exactly three matches (around lines 49, 105, 171 at the time of writing, but trust the search, not the line numbers):

- `root_view` parameter
- the `ctrl_once: Rc<RefCell<Option<(CtrlHandle, Receiver<...>)>>>` local binding
- `editing_view` parameter

Change each to `crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>`.

**5. Rewrite the consumer effect.** Find the block at `ui/mod.rs:515–524` (inside `#[cfg(debug_assertions)] if let Some((ctrl_handle, ctrl_action_rx)) = ctrl { ... }`):

```rust
let action_read = create_signal_from_channel(ctrl_action_rx);
create_effect(move |_| {
    if let Some(ctrl_action) = action_read.get() {
        let block_action =
            current_doc.with_untracked(|d| ctrl_action.into_block_action(d.as_ref()?));
        if let Some(action) = block_action {
            on_action_for_ctrl(action);
        }
    }
});
```

Replace with:

```rust
let action_read = create_signal_from_channel(ctrl_action_rx);
create_effect(move |_| {
    if let Some((ctrl_action, reply_tx)) = action_read.get() {
        let block_id = ctrl_action.block_id();
        // Translate against the current doc. into_block_action's
        // only failure mode is an unknown block id; a missing doc
        // is detected separately so the caller gets a precise
        // result. on_action MUST run outside with_untracked — it
        // calls current_doc.update() and would re-borrow the signal.
        let translated: Result<BlockAction, crate::ctrl::CtrlActionResult> =
            current_doc.with_untracked(|maybe| match maybe.as_ref() {
                None => Err(crate::ctrl::CtrlActionResult::NoDocument),
                Some(doc) => ctrl_action.into_block_action(doc).ok_or(
                    crate::ctrl::CtrlActionResult::BlockNotFound { block_id },
                ),
            });
        let result = match translated {
            Ok(action) => {
                on_action_for_ctrl(action);
                crate::ctrl::CtrlActionResult::Dispatched
            }
            Err(failure) => failure,
        };
        let _ = reply_tx.send(result);
    }
});
```

The ordering — translate inside `with_untracked`, dispatch outside — is load-bearing: `on_action` calls `current_doc.update(...)` (see `ui/mod.rs:278`, `:332`, `:361`), which would re-borrow the signal if it ran inside `with_untracked`. `BlockAction` is already imported at `ui/mod.rs:25`.

### `lib.rs`

No edits. `lib.rs:55` binds `let (ctrl_handle, ctrl_action_rx) = ctrl::start();` by inference and forwards `ctrl_action_rx` to `root_view`; both sides are now `CtrlActionEnvelope`. If a type error appears at `lib.rs`, recheck steps 2 and 4.

**Done when:** the full-workspace `cargo build` is clean (not just `-p lopress-editor`); four-command suite passes; one commit.

Suggested message: `feat(editor): /action reports dispatched / no_document / block_not_found`

---

## Task 5: Update the `driving-lopress-editor` skill doc

The `/action` contract changed; the operator doc must match. File: `.claude/skills/driving-lopress-editor/SKILL.md`.

**1. Quick-reference table row.** Replace (around line 39):

```
| `/action` | POST | Apply a `CtrlAction` to the doc | `ok` / `400 parse error: …` |
```

with:

```
| `/action` | POST | Apply a `CtrlAction` to the doc | `200 {"status":"dispatched"}` / `409 no_document` / `422 block_not_found` / `400 parse error` |
```

**2. Unknown-`block_id` sentence.** Replace (around line 58):

```
`new_kind` types: `Paragraph`, `Heading {level}`, `Code {lang}`, `List {ordered}`. An unknown `block_id` is silently dropped — the action just doesn't apply.
```

with:

```
`new_kind` types: `Paragraph`, `Heading {level}`, `Code {lang}`, `List {ordered}`.

`/action` blocks until the editor reports an outcome and answers with JSON: `200 {"status":"dispatched"}` when the action reached a real block and was routed to the editor; `422 {"status":"block_not_found","block_id":N}` when the id does not exist in the open document; `409 {"status":"no_document"}` when no document is open. (`200`/dispatched does not guarantee the document changed — a no-op action such as `Move` to the same position still dispatches.) On Windows, `Invoke-RestMethod` throws on `4xx` codes — that thrown error is the signal the action did not apply; previously such cases were silently dropped with a `200 ok`.
```

**3. Common-mistakes bullet.** Replace (around line 104):

```
- **Acting before a doc is open** — `/action` is a silent no-op when `doc_open` is false.
```

with:

```
- **Acting before a doc is open** — `/action` now returns `409 {"status":"no_document"}` when `doc_open` is false (no longer a silent no-op).
```

Open the file and confirm the markdown still parses cleanly (table well-formed, no broken fences). No build/test step here.

**Done when:** edits applied; one commit.

Suggested message: `docs(skill): /action result reporting contract`

---

## What this plan deliberately does NOT do

- **Distinguish "dispatched" from "dispatched but apply was a no-op."** Reporting that would require threading `apply`'s return up through the `on_action` chokepoint (`ActionSink`), a signature change rippling to every block widget. `200 dispatched` means "routed to a real block"; verify actual mutation via `/state` if needed.
- **Change the `CtrlAction` wire format or add new verbs.** The eight-variant request set is unchanged.
- **Defend against `create_signal_from_channel` coalescing.** Under rapid-fire `/action` calls the consumer can coalesce and drop an envelope; the dropped caller then gets `504 editor did not respond` instead of a false `200`. The reviewer drives the editor serially (request, await response, repeat), so this is not a practical concern.

---

## Hand-back

When all five commits land, push the branch (or just leave them local — say which) and report back with:

- the five commit shas,
- the final `cargo test -p lopress-editor` summary line,
- any deviations from this plan and why.

The reviewer will then drive the live editor for a sanity check (valid id → `200`, bogus id → `422`, no-doc → `409`) and ship the branch.

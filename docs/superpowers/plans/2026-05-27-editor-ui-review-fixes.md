# Editor UI Review — Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every UI defect documented in `docs/superpowers/ideas/2026-05-26-editor-ui-review.md` — twelve findings spanning slash-menu regression, toolbar click failures, missing undo coverage, lang-input ergonomics, layout jump on focus, false dirty marks, welcome-recents dedup, and a new `/open` / `/close` ctrl endpoint.

**Architecture:** One stage per finding (12 tasks total); each stage is a single commit. Stages are sequenced so trivial isolated fixes land first and the larger structural changes (toolbar moved outside the focus border, layout-jump fix, empty-list-item affordance) land last after their prerequisites. Task 8 (toolbar clicks) is gated on a real-mouse diagnosis step that runs first — the implementer must complete that diagnosis and branch based on what they find, not assume.

**Tech Stack:** Rust, Floem 0.2 GUI framework, crate `lopress-editor`. Debug ctrl server at `crates/lopress-editor/src/ctrl/`.

**Spec:** `docs/superpowers/specs/2026-05-27-editor-ui-review-fixes-design.md`.

---

## File Structure

| File | Tasks | Role |
|---|---|---|
| `crates/lopress-editor/src/ui/toolbar.rs` | 1, 8, 10 | toolbar button ordering, click diagnosis, visual separation |
| `crates/lopress-editor/src/recents.rs` | 2 | recents dedup helper |
| `crates/lopress-editor/src/ui/welcome.rs` | 2 | recents dedup at display time |
| `crates/lopress-editor/src/ui/editing/action_sink.rs` | 3 | gate `mark_dirty` on recorded change |
| `crates/lopress-editor/src/ctrl/mod.rs` | 4 | `/open` + `/close` HTTP routes, channels, types |
| `crates/lopress-editor/src/ui/editing/ctrl_wire.rs` | 4 | open/close effect handlers |
| `crates/lopress-editor/src/ui/mod.rs` | 4, 7 | thread channels + new `on_open` arg; inspector wiring |
| `.claude/skills/driving-lopress-editor/SKILL.md` | 4 | document the new endpoints |
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | 5 | KeyDown short-circuit when `combined_key` consumes the key |
| `crates/lopress-editor/src/ui/blocks/code_editor.rs` | 6 | Enter/Escape handlers on lang `text_input` |
| `crates/lopress-editor/src/actions.rs` | 7 | `EditFrontMatter` variant + apply arm |
| `crates/lopress-editor/src/ui/inspector.rs` | 7 | dispatch front-matter edits through `on_action` |
| `crates/lopress-editor/src/ui/blocks/mod.rs` | 9, 11 | toolbar outside focus border; reserve toolbar height slot |
| `crates/lopress-editor/src/ui/blocks/list.rs` | 12 | empty list item placeholder |

---

### Task 1: Toolbar button ordering (Section 7)

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`

**Goal:** Move H4/H5/H6 to follow H1/H2/H3, before Code/UL/OL.

No unit test — the `kinds` vec is a local inside `block_toolbar_for`, not separable for unit testing without an awkward extraction. Verification is the manual screenshot step.

- [ ] **Step 1: Reorder the `kinds` vector**

In `crates/lopress-editor/src/ui/toolbar.rs`, locate the `let kinds: Vec<(&'static str, BlockKind)> = vec![...]` block (around line 55) and replace it with:

```rust
    let kinds: Vec<(&'static str, BlockKind)> = vec![
        ("P", BlockKind::Paragraph),
        ("H1", BlockKind::Heading(1)),
        ("H2", BlockKind::Heading(2)),
        ("H3", BlockKind::Heading(3)),
        ("H4", BlockKind::Heading(4)),
        ("H5", BlockKind::Heading(5)),
        ("H6", BlockKind::Heading(6)),
        ("Code", BlockKind::Code { lang: String::new() }),
        ("UL", BlockKind::List { ordered: false }),
        ("OL", BlockKind::List { ordered: true }),
    ];
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Launch the app, open any document, click into a paragraph.
- Screenshot the toolbar. Confirm the row reads
  `P · H1 · H2 · H3 · H4 · H5 · H6 · Code · UL · OL · | · B · I · </> · Link · | · x`.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "$(cat <<'EOF'
refactor(editor): reorder toolbar buttons so headings are contiguous

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Welcome recents dedup (Section 11)

**Files:**
- Modify: `crates/lopress-editor/src/recents.rs`
- Modify: `crates/lopress-editor/src/ui/welcome.rs`

**Goal:** Canonicalize recents before display and dedup; canonicalize on insertion too.

- [ ] **Step 1: Write the failing test**

Append the following test module at the very end of `crates/lopress-editor/src/recents.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::dedup_canonical;
    use std::path::PathBuf;

    #[test]
    fn dedup_removes_exact_duplicates() {
        let paths = vec![
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/b"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0], PathBuf::from("/nonexistent/a"));
        assert_eq!(deduped[1], PathBuf::from("/nonexistent/b"));
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        let paths = vec![
            PathBuf::from("/nonexistent/b"),
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/b"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(deduped, vec![
            PathBuf::from("/nonexistent/b"),
            PathBuf::from("/nonexistent/a"),
        ]);
    }

    #[test]
    fn dedup_falls_back_to_raw_when_canonicalize_fails() {
        // Canonicalize on nonexistent paths returns Err; the fallback keeps
        // the raw paths so they still appear (and still dedup).
        let paths = vec![
            PathBuf::from("/nonexistent/x"),
            PathBuf::from("/nonexistent/x"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(deduped.len(), 1);
    }
}
```

(The case-folding and trailing-slash cases require real on-disk paths, which is platform-dependent for canonicalize. The tests above cover the post-canonicalize dedup semantics; the case/slash collapse is delivered by `Path::canonicalize` itself when the path exists.)

- [ ] **Step 2: Verify the test fails to compile**

```
cargo test -p lopress-editor dedup
```

Expected: compilation error — `unresolved import 'super::dedup_canonical'`.

- [ ] **Step 3: Add `dedup_canonical` helper**

Add the following near the top of `crates/lopress-editor/src/recents.rs`, after the existing imports and before any existing free functions:

```rust
use std::path::PathBuf;

/// Canonicalize each path and return de-duplicated results in original order
/// (first occurrence wins). Falls back to the raw path when `canonicalize`
/// fails (e.g., the workspace was deleted or unmounted), so legitimate
/// recents don't disappear silently.
pub(crate) fn dedup_canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .fold(Vec::new(), |mut acc, p| {
            if !acc.contains(&p) {
                acc.push(p);
            }
            acc
        })
}
```

- [ ] **Step 4: Verify tests pass**

```
cargo test -p lopress-editor dedup
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 5: Dedup on insertion**

In the same file, locate the `push` function (the one that updates the recents file). Find the line that calls `paths.truncate(MAX_RECENTS);` and insert a dedup call immediately above it:

```rust
    paths.retain(|p| p != workspace);
    paths.insert(0, workspace.to_path_buf());
    let mut paths = dedup_canonical(&paths);   // <-- new
    paths.truncate(MAX_RECENTS);
```

If the surrounding code uses `let mut paths = load();` followed by `paths.retain(...) ... paths.insert(...) ... paths.truncate(...)` directly on that binding, replace those four lines with the snippet above (which shadows `paths` with the deduped version).

- [ ] **Step 6: Dedup at display time in welcome.rs**

In `crates/lopress-editor/src/ui/welcome.rs`, locate the `dyn_container` whose first argument reads `settings.get().recents` (the recents builder, around line 48). Inside the second-argument closure, pipe the `recents` vec through `crate::recents::dedup_canonical` before the existing button-mapping:

```rust
            let recents = crate::recents::dedup_canonical(&recents);
```

— inserted right after the closure's outer `let recents = ...;` (or whatever binds the incoming param) and before the button mapping uses it.

- [ ] **Step 7: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 8: Commit**

```
git add crates/lopress-editor/src/recents.rs crates/lopress-editor/src/ui/welcome.rs
git commit -m "$(cat <<'EOF'
fix(editor): deduplicate welcome recents by canonicalizing paths

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: False dirty marks gating (Section 10)

**Files:**
- Modify: `crates/lopress-editor/src/ui/editing/action_sink.rs`

**Goal:** Only call `mark_dirty` when the apply actually recorded a change.

No unit test — `build_action_sink` requires a live floem reactive scope and the `BlockId` type is opaque, making the test setup unwieldy for a one-line gate. Manual verification covers it.

- [ ] **Step 1: Gate `mark_dirty` on `recorded.is_some()`**

In `crates/lopress-editor/src/ui/editing/action_sink.rs`, locate the line `on_action_mark_dirty();` (around line 98, the last statement before the closing `}` of the `Rc::new(move |action: BlockAction| { ... })` closure) and replace it with:

```rust
        if recorded.is_some() {
            on_action_mark_dirty();
        }
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Verify existing tests still pass**

```
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 4: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Click into a block. Status bar should say `saved`.
- Click outside any block (on the canvas margin) — status stays `saved`.
- Click into a block and type a character — status flips to `unsaved`, then back to `saved` after the debounce.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/editing/action_sink.rs
git commit -m "$(cat <<'EOF'
fix(editor): only mark dirty when apply records a change

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `/open` and `/close` ctrl endpoints (Section 12)

**Files:**
- Modify: `crates/lopress-editor/src/ctrl/mod.rs`
- Modify: `crates/lopress-editor/src/ui/editing/ctrl_wire.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`
- Modify: `crates/lopress-editor/src/lib.rs` (the `start()` consumer)
- Modify: `.claude/skills/driving-lopress-editor/SKILL.md`

**Goal:** Add `POST /open { "path": "..." }` and `POST /close` to the debug ctrl server, threading through the same `on_open` path the welcome view uses.

Implementer note: read `crates/lopress-editor/src/ctrl/mod.rs` end-to-end before starting. The existing `/action` flow is the template — open/close mirrors it with a separate channel each.

- [ ] **Step 1: Add request/response types**

In `crates/lopress-editor/src/ctrl/mod.rs`, after the existing `CtrlActionResult` definition (around line 130 — find by grep), add:

```rust
/// Body of `POST /open`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CtrlOpenRequest {
    pub path: String,
}

/// Reply outcome for `/open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtrlOpenResult {
    Opened,
    NotFound,
    NoWorkspace,
}

impl CtrlOpenResult {
    pub(crate) fn http_parts(&self) -> (u16, String) {
        match self {
            CtrlOpenResult::Opened => (200, r#"{"status":"opened"}"#.to_string()),
            CtrlOpenResult::NotFound => (404, r#"{"status":"not_found"}"#.to_string()),
            CtrlOpenResult::NoWorkspace => (409, r#"{"status":"no_workspace"}"#.to_string()),
        }
    }
}

/// Reply outcome for `/close`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtrlCloseResult {
    Closed,
    NoWorkspace,
}

impl CtrlCloseResult {
    pub(crate) fn http_parts(&self) -> (u16, String) {
        match self {
            CtrlCloseResult::Closed => (200, r#"{"status":"closed"}"#.to_string()),
            CtrlCloseResult::NoWorkspace => (409, r#"{"status":"no_workspace"}"#.to_string()),
        }
    }
}

/// Envelopes for the open/close channels — the parsed payload + a one-shot
/// reply sender. The trailing comma on `CtrlCloseEnvelope` is load-bearing:
/// without it `(Sender<...>)` is a parenthesized type, not a tuple, and the
/// `if let Some((tx,)) = ...` pattern below won't destructure.
pub(crate) type CtrlOpenEnvelope = (String, crossbeam_channel::Sender<CtrlOpenResult>);
pub(crate) type CtrlCloseEnvelope = (crossbeam_channel::Sender<CtrlCloseResult>,);
```

- [ ] **Step 2: Extend `CtrlHandle` and `start()` to carry open/close channels**

In the same file, replace the `CtrlHandle` struct and the `start` function:

```rust
pub(crate) struct CtrlHandle {
    pub snapshot: std::sync::Arc<std::sync::Mutex<String>>,
    pub open_tx: crossbeam_channel::Sender<CtrlOpenEnvelope>,
    pub close_tx: crossbeam_channel::Sender<CtrlCloseEnvelope>,
}

pub(crate) fn start() -> (
    CtrlHandle,
    crossbeam_channel::Receiver<CtrlActionEnvelope>,
    crossbeam_channel::Receiver<CtrlOpenEnvelope>,
    crossbeam_channel::Receiver<CtrlCloseEnvelope>,
) {
    let snapshot = std::sync::Arc::new(std::sync::Mutex::new(
        r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string(),
    ));
    let (action_tx, action_rx) = crossbeam_channel::unbounded::<CtrlActionEnvelope>();
    let (open_tx, open_rx) = crossbeam_channel::unbounded::<CtrlOpenEnvelope>();
    let (close_tx, close_rx) = crossbeam_channel::unbounded::<CtrlCloseEnvelope>();

    let handle = CtrlHandle {
        snapshot: std::sync::Arc::clone(&snapshot),
        open_tx: open_tx.clone(),
        close_tx: close_tx.clone(),
    };

    let server_snapshot = std::sync::Arc::clone(&snapshot);
    std::thread::spawn(move || {
        serve(server_snapshot, action_tx, open_tx, close_tx);
    });

    (handle, action_rx, open_rx, close_rx)
}
```

- [ ] **Step 3: Update `serve` and `handle_request` signatures to carry open/close senders**

In the same file, update `serve`:

```rust
fn serve(
    snapshot: std::sync::Arc<std::sync::Mutex<String>>,
    action_tx: crossbeam_channel::Sender<CtrlActionEnvelope>,
    open_tx: crossbeam_channel::Sender<CtrlOpenEnvelope>,
    close_tx: crossbeam_channel::Sender<CtrlCloseEnvelope>,
) {
    // …existing tiny_http server setup…
    for mut request in server.incoming_requests() {
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();
        let response = handle_request(
            &mut request, &method, &url, &snapshot, &action_tx, &open_tx, &close_tx,
        );
        let _ = request.respond(response);
    }
}
```

And `handle_request`:

```rust
fn handle_request(
    request: &mut tiny_http::Request,
    method: &str,
    url: &str,
    snapshot: &std::sync::Arc<std::sync::Mutex<String>>,
    action_tx: &crossbeam_channel::Sender<CtrlActionEnvelope>,
    open_tx: &crossbeam_channel::Sender<CtrlOpenEnvelope>,
    close_tx: &crossbeam_channel::Sender<CtrlCloseEnvelope>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
```

The body's existing arms are unchanged. Read the body of the existing function to confirm — if it bodies-in-place rather than via a `match (method, url)`, adapt.

- [ ] **Step 4: Add `/open` and `/close` arms**

In `handle_request`'s `match (method, url)` dispatch, add the two arms just before the final `_ =>` catch-all:

```rust
        ("POST", "/open") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                return text_response("read error", 400);
            }
            match serde_json::from_str::<CtrlOpenRequest>(&body) {
                Ok(req) => {
                    let (reply_tx, reply_rx) =
                        crossbeam_channel::bounded::<CtrlOpenResult>(1);
                    if open_tx.send((req.path, reply_tx)).is_err() {
                        return text_response("editor channel closed", 503);
                    }
                    match reply_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                        Ok(result) => {
                            let (code, json) = result.http_parts();
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

        ("POST", "/close") => {
            let (reply_tx, reply_rx) =
                crossbeam_channel::bounded::<CtrlCloseResult>(1);
            if close_tx.send((reply_tx,)).is_err() {
                return text_response("editor channel closed", 503);
            }
            match reply_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(result) => {
                    let (code, json) = result.http_parts();
                    tiny_http::Response::from_string(json)
                        .with_header(json_header())
                        .with_status_code(code)
                }
                Err(_) => text_response("editor did not respond", 504),
            }
        }
```

`text_response` and `json_header` are existing helpers in the same file — reuse them.

- [ ] **Step 5: Update `wire_ctrl` to consume the open/close channels**

In `crates/lopress-editor/src/ui/editing/ctrl_wire.rs`, replace the `wire_ctrl` function's signature and body. The replacement adds three new arguments (`on_open`, `editing`, `state_tag`) and two new effects that read the open/close receivers.

```rust
use crate::actions::BlockAction;
use crate::ctrl::{
    CtrlActionEnvelope, CtrlActionResult, CtrlCloseEnvelope, CtrlCloseResult, CtrlHandle,
    CtrlOpenEnvelope, CtrlOpenResult,
};
use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::StateTag;
use crate::ui::blocks::inline_editor::ActionSink;
use floem::ext_event::create_signal_from_channel;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

#[allow(clippy::too_many_arguments)]
pub(crate) fn wire_ctrl(
    ctrl_handle: CtrlHandle,
    ctrl_action_rx: crossbeam_channel::Receiver<CtrlActionEnvelope>,
    ctrl_open_rx: crossbeam_channel::Receiver<CtrlOpenEnvelope>,
    ctrl_close_rx: crossbeam_channel::Receiver<CtrlCloseEnvelope>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: ActionSink,
    on_open: Rc<dyn Fn(PathBuf)>,
    editing: Rc<RefCell<Option<EditingState>>>,
    state_tag: RwSignal<StateTag>,
) {
    // Existing snapshot effect — unchanged.
    let snap = ctrl_handle.snapshot.clone();
    create_effect(move |_| {
        let json = current_doc.with(|maybe| {
            crate::ctrl::serialize_state(maybe.as_ref(), current_path.get_untracked().as_deref())
        });
        *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
    });

    // Existing action effect — unchanged shape, just rebound to the local rx.
    let action_read = create_signal_from_channel(ctrl_action_rx);
    create_effect(move |_| {
        if let Some((ctrl_action, reply_tx)) = action_read.get() {
            let block_id = ctrl_action.block_id();
            let translated: Result<BlockAction, CtrlActionResult> =
                current_doc.with_untracked(|maybe| match maybe.as_ref() {
                    None => Err(CtrlActionResult::NoDocument),
                    Some(doc) => ctrl_action
                        .into_block_action(doc)
                        .ok_or(CtrlActionResult::BlockNotFound { block_id }),
                });
            let result = match translated {
                Ok(action) => {
                    on_action(action);
                    CtrlActionResult::Dispatched
                }
                Err(failure) => failure,
            };
            let _ = reply_tx.send(result);
        }
    });

    // New: open effect. Resolves the requested path against the open workspace
    // (or as absolute) and dispatches through the same `on_open` closure the
    // welcome view uses.
    let open_read = create_signal_from_channel(ctrl_open_rx);
    let editing_for_open = Rc::clone(&editing);
    create_effect(move |_| {
        if let Some((path_str, reply_tx)) = open_read.get() {
            let raw = PathBuf::from(&path_str);
            let resolved: Option<PathBuf> = if raw.is_absolute() {
                Some(raw)
            } else {
                editing_for_open
                    .borrow()
                    .as_ref()
                    .map(|s| s.session.workspace().root.join(&raw))
            };
            let result = match resolved {
                None => CtrlOpenResult::NoWorkspace,
                Some(p) if !p.exists() => CtrlOpenResult::NotFound,
                Some(p) => {
                    on_open(p);
                    CtrlOpenResult::Opened
                }
            };
            let _ = reply_tx.send(result);
        }
    });

    // New: close effect. Clears `current_doc`, drops the EditingState, returns
    // the app to the welcome view.
    let close_read = create_signal_from_channel(ctrl_close_rx);
    let editing_for_close = Rc::clone(&editing);
    create_effect(move |_| {
        if let Some((reply_tx,)) = close_read.get() {
            let result = if editing_for_close.borrow().is_some() {
                *editing_for_close.borrow_mut() = None;
                current_doc.set(None);
                current_path.set(None);
                state_tag.set(StateTag::Welcome);
                CtrlCloseResult::Closed
            } else {
                CtrlCloseResult::NoWorkspace
            };
            let _ = reply_tx.send(result);
        }
    });
}
```

`StateTag` needs to be visible from `ctrl_wire.rs`. It's currently private to `ui/mod.rs`; add `pub(crate)` in front of its `enum StateTag` declaration and re-export at the `ui` module so `crate::ui::StateTag` resolves. If the type already isn't named exactly `StateTag`, find the actual name via grep and substitute.

- [ ] **Step 6: Thread the new receivers + args through `ui/mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, update the `ctrl_once` cell type to hold the new receivers:

```rust
    #[cfg(debug_assertions)]
    #[allow(clippy::type_complexity)]
    let ctrl_once: Rc<
        std::cell::RefCell<
            Option<(
                crate::ctrl::CtrlHandle,
                crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
                crossbeam_channel::Receiver<crate::ctrl::CtrlOpenEnvelope>,
                crossbeam_channel::Receiver<crate::ctrl::CtrlCloseEnvelope>,
            )>,
        >,
    > = Rc::new(std::cell::RefCell::new(Some((
        ctrl_handle,
        ctrl_action_rx,
        ctrl_open_rx,
        ctrl_close_rx,
    ))));
```

The `ctrl_handle`, `ctrl_action_rx`, `ctrl_open_rx`, `ctrl_close_rx` bindings come from `root_view`'s caller — see Step 7 for the `lib.rs`/`run()` update that produces them.

In `editing_view`, update the function signature so it accepts the full 4-tuple:

```rust
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    #[cfg(debug_assertions)] ctrl: Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
        crossbeam_channel::Receiver<crate::ctrl::CtrlOpenEnvelope>,
        crossbeam_channel::Receiver<crate::ctrl::CtrlCloseEnvelope>,
    )>,
) -> impl IntoView {
```

And replace the `wire_ctrl` call near the bottom with the new arg list:

```rust
    #[cfg(debug_assertions)]
    if let Some((ctrl_handle, ctrl_action_rx, ctrl_open_rx, ctrl_close_rx)) = ctrl {
        ctrl_wire::wire_ctrl(
            ctrl_handle,
            ctrl_action_rx,
            ctrl_open_rx,
            ctrl_close_rx,
            current_doc,
            current_path,
            on_action_for_ctrl,
            Rc::clone(&on_open) as Rc<dyn Fn(_)>,
            Rc::clone(&editing),
            state_tag,
        );
    }
```

`state_tag` must be visible in `editing_view`. It currently lives in `root_view`'s closure; thread it through as a new arg or move its declaration so `editing_view` can read it. Cleanest: add a new `state_tag: RwSignal<StateTag>` parameter to `editing_view` and pass it from `root_view`.

`on_open` in `editing_view` is the per-document open closure (DocumentRef → focus). For ctrl-side use we need the workspace+document open closure that the welcome view uses (PathBuf → workspace + document). Look at `root_view`'s `on_open` (the one declared around line 77 in the current file). Move its definition (or a clone) to also be reachable by `editing_view`, or have `wire_ctrl` invoke the workspace-open path via the `editing` ref directly. The simplest: capture `on_open` (the PathBuf one) in `root_view` into a `Rc<dyn Fn(PathBuf)>` and pass it through `ctrl_once` alongside the receivers. Add it as a 5th tuple element.

If threading `on_open` through `ctrl_once` is mechanically too painful, the alternative is for `wire_ctrl`'s open effect to call `editing.borrow_mut()` and run the open path directly (mirroring what `on_open` does in `root_view`). Pick whichever requires fewer cross-function changes.

- [ ] **Step 7: Update `start()` consumer in `lib.rs`**

Find where `crate::ctrl::start()` is called (grep for `ctrl::start`). The current call destructures a 2-tuple `(handle, action_rx)`; update to the 4-tuple `(handle, action_rx, open_rx, close_rx)` and pass all four into `root_view`. Update `root_view`'s signature accordingly:

```rust
pub(crate) fn root_view(
    ctx: AppContext,
    settings_signal: RwSignal<Settings>,
    #[cfg(debug_assertions)] ctrl_handle: crate::ctrl::CtrlHandle,
    #[cfg(debug_assertions)] ctrl_action_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
    #[cfg(debug_assertions)] ctrl_open_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlOpenEnvelope>,
    #[cfg(debug_assertions)] ctrl_close_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlCloseEnvelope>,
) -> impl IntoView {
```

- [ ] **Step 8: Add tests for the result-to-HTTP mapping**

Append the following test module at the very end of `crates/lopress-editor/src/ctrl/mod.rs` (or extend the existing `#[cfg(test)] mod tests` block if one exists):

```rust
#[cfg(test)]
mod open_close_tests {
    use super::*;

    #[test]
    fn open_result_http_parts() {
        assert_eq!(CtrlOpenResult::Opened.http_parts(), (200, r#"{"status":"opened"}"#.to_string()));
        assert_eq!(CtrlOpenResult::NotFound.http_parts(), (404, r#"{"status":"not_found"}"#.to_string()));
        assert_eq!(CtrlOpenResult::NoWorkspace.http_parts(), (409, r#"{"status":"no_workspace"}"#.to_string()));
    }

    #[test]
    fn close_result_http_parts() {
        assert_eq!(CtrlCloseResult::Closed.http_parts(), (200, r#"{"status":"closed"}"#.to_string()));
        assert_eq!(CtrlCloseResult::NoWorkspace.http_parts(), (409, r#"{"status":"no_workspace"}"#.to_string()));
    }
}
```

- [ ] **Step 9: Verify it compiles and tests pass**

```
cargo check -p lopress-editor
cargo test -p lopress-editor ctrl
```

Expected: no errors; 5 new tests pass.

- [ ] **Step 10: Manual verification**

Start the editor with `cargo run`, no document open. Then:

```bash
# Open by absolute path:
curl -X POST http://127.0.0.1:7878/open \
  -H 'Content-Type: application/json' \
  -d '{"path":"C:\\\\Users\\\\corpo\\\\Documents\\\\lopress-listtest\\\\src\\\\posts\\\\listtest.md"}'
# Expected: {"status":"opened"}; /state shows doc_open:true.

# Close:
curl -X POST http://127.0.0.1:7878/close
# Expected: {"status":"closed"}; /state shows doc_open:false.

# Relative path before workspace open:
curl -X POST http://127.0.0.1:7878/open -d '{"path":"posts/foo.md"}'
# Expected: {"status":"no_workspace"} (409).

# Nonexistent path:
curl -X POST http://127.0.0.1:7878/open -d '{"path":"C:\\\\nonexistent.md"}'
# Expected: {"status":"not_found"} (404).
```

- [ ] **Step 11: Update the driving-lopress-editor skill doc**

In `.claude/skills/driving-lopress-editor/SKILL.md`, find the `Quick reference` table (search for `/ping` `Liveness check`) and add two rows:

```markdown
| `/open` | POST | Open a doc by path. Body `{ "path": "..." }`. Absolute or workspace-relative. | `200 {"status":"opened"}` / `404 {"status":"not_found"}` / `409 {"status":"no_workspace"}` |
| `/close` | POST | Close the current doc and return to the welcome view. | `200 {"status":"closed"}` / `409 {"status":"no_workspace"}` |
```

Also add a short subsection after the existing `POST /action` documentation describing the two endpoints, mirroring the `/action` example's prose density.

- [ ] **Step 12: Commit**

```
git add crates/lopress-editor/src/ctrl/mod.rs \
        crates/lopress-editor/src/ui/editing/ctrl_wire.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/src/lib.rs \
        .claude/skills/driving-lopress-editor/SKILL.md
git commit -m "$(cat <<'EOF'
feat(editor): add /open and /close ctrl endpoints

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Slash menu regression fix (Section 1)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

**Goal:** Short-circuit character insertion when `combined_key` returns `CommandExecuted::Yes`.

No unit test — `combined_key` needs a live floem editor instance; the KeyDown handler isn't isolable. Verification is via the driving skill.

- [ ] **Step 1: Capture `combined_key`'s return value and short-circuit**

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, locate the `.on_event_stop(EventListener::KeyDown, move |event| { ... })` builder call inside the `view` chain (added by the 2026-05-18 rewire; grep for `EventListener::KeyDown` to find it). The current body calls `combined_key(&keypress, key_event.modifiers);` without using its return value.

Replace the entire KeyDown handler body with:

```rust
        .on_event_stop(EventListener::KeyDown, move |event| {
            let Event::KeyDown(key_event) = event else { return; };
            let key_text = key_event.key.text.clone();
            let Ok(keypress) = KeyPress::try_from(key_event) else { return; };
            if combined_key(&keypress, key_event.modifiers) == CommandExecuted::Yes {
                return;
            }

            let mut mods = key_event.modifiers;
            mods.set(floem::keyboard::Modifiers::SHIFT, false);
            mods.set(floem::keyboard::Modifiers::ALTGR, false);
            #[cfg(target_os = "macos")]
            mods.set(floem::keyboard::Modifiers::ALT, false);
            if mods.is_empty() {
                use floem::keyboard::{Key, NamedKey};
                match keypress.key {
                    KeyInput::Keyboard(Key::Character(c), _) => {
                        editor_sig.get_untracked().receive_char(&c);
                    }
                    KeyInput::Keyboard(Key::Named(NamedKey::Space), _) => {
                        editor_sig.get_untracked().receive_char(" ");
                    }
                    KeyInput::Keyboard(Key::Unidentified(_), _) => {
                        if let Some(text) = key_text {
                            editor_sig.get_untracked().receive_char(&text);
                        }
                    }
                    _ => {}
                }
            }
        })
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Open a document. Click into a non-empty paragraph. Press End, then Enter to create a new empty paragraph (focus auto-routes to it).
- Press `/`. The slash menu opens; the `/` character is NOT inserted into the block.
- Press Escape. The slash menu closes; focus returns to the empty block.
- Type into the empty block — characters insert normally. Press `/` mid-text — `/` inserts as a literal character (block is no longer empty).

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "$(cat <<'EOF'
fix(editor): short-circuit character input when combined_key consumes the key

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Code-block lang commits on Enter, reverts on Escape (Section 4 + Section 5 verify)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/code_editor.rs`

**Goal:** Add Enter (commit) and Escape (revert) handlers to the lang `text_input`. This task also verifies Section 5 (lang undo) — the existing FocusLost path already dispatches `BlockAction::EditAttrs`, which `apply_edit_attrs` records on the undo stack. Once Enter dispatches the same action, lang edits are undoable via Ctrl+Z whether the user committed by Enter or by blurring.

- [ ] **Step 1: Extract the commit closure**

In `crates/lopress-editor/src/ui/blocks/code_editor.rs`, locate the existing `lang_input` builder (around line 180, marked by `let lang_input = text_input(lang_sig)` followed by `.on_event_stop(EventListener::FocusLost, ...)`). Lift the commit body into a reusable closure, then call it from both the FocusLost handler and the new KeyDown handler:

```rust
    let commit_lang = {
        let on_action = lang_on_action.clone();
        let lang_committed = lang_committed;
        Rc::new(move || {
            floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
                let new_lang = lang_sig.get_untracked();
                if new_lang != lang_committed.get_untracked() {
                    let mut new_attrs = serde_json::Map::new();
                    new_attrs.insert(
                        "lang".to_string(),
                        serde_json::Value::String(new_lang.clone()),
                    );
                    on_action(BlockAction::EditAttrs {
                        block_id,
                        new_attrs,
                    });
                    lang_committed.set(new_lang);
                }
            });
        })
    };

    let commit_for_blur = Rc::clone(&commit_lang);
    let commit_for_key = Rc::clone(&commit_lang);

    let lang_input = text_input(lang_sig)
        .on_event_stop(EventListener::FocusLost, move |_| {
            commit_for_blur();
        })
        .on_event_stop(EventListener::KeyDown, move |e: &floem::event::Event| {
            if let floem::event::Event::KeyDown(k) = e {
                if matches!(k.key.logical_key, floem::keyboard::Key::Named(floem::keyboard::NamedKey::Enter)) {
                    commit_for_key();
                } else if matches!(k.key.logical_key, floem::keyboard::Key::Named(floem::keyboard::NamedKey::Escape)) {
                    let original = lang_committed.get_untracked();
                    lang_sig.set(original);
                }
            }
        })
        .style(|s| /* keep the existing style closure verbatim */);
```

Preserve the existing `.style(...)` closure body — only the event handlers change. Confirm `Rc` is imported at the top of `code_editor.rs` (it likely already is; if not, add `use std::rc::Rc;`).

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual verification (Section 4)**

Using the `driving-lopress-editor` debug skill:
- Open a document containing a code block (or create one via `/action` `ChangeType` to Code).
- Click the lang label (top-right of the code block).
- Select all (Ctrl+A), type `python`, press Enter. `/state` shows `lang: "python"`. Focus stays in the input.
- Select all, type `javascript`, press Escape. `/state` still shows `python`; the input shows `python` again (reverted).
- Select all, type `rust`, click outside the input. `/state` shows `rust` (blur still commits).

- [ ] **Step 4: Manual verification (Section 5 — lang undo)**

Continuing from Step 3:
- After committing a lang change (Enter or blur), click into any block editor to focus it. Press Ctrl+Z. `/state` shows the previous lang.
- Press Ctrl+Y (redo). `/state` shows the new lang again.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/code_editor.rs
git commit -m "$(cat <<'EOF'
fix(editor): commit lang on Enter, revert on Escape

Lang edits already routed through EditAttrs (so undo recording was
already in place); the Enter handler completes the parity with the
toolbar URL input which has always committed on Enter.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Front-matter undo (Section 3)

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs`
- Modify: `crates/lopress-editor/src/ui/inspector.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`

**Goal:** Add `BlockAction::EditFrontMatter` + apply arm; wire every inspector front-matter edit site to dispatch through `on_action` instead of mutating `current_doc` directly.

- [ ] **Step 1: Add `EditFrontMatter` variant**

In `crates/lopress-editor/src/actions.rs`, add a new variant at the end of the `BlockAction` enum, after `EditBlockBody`:

```rust
    /// Replace the document's front matter with `new_front_matter`. Used by
    /// the inspector to make front-matter edits undoable. One action per
    /// commit (Title blur, Slug blur, Date validation success, etc.).
    EditFrontMatter {
        new_front_matter: lopress_core::FrontMatter,
    },
```

- [ ] **Step 2: Add the apply arm in `apply()`**

In the `apply` function's `match action { ... }`, add a new arm before the closing brace (alongside the other arms):

```rust
        BlockAction::EditFrontMatter { new_front_matter } => {
            apply_edit_front_matter(doc, new_front_matter)
        }
```

- [ ] **Step 3: Add `apply_edit_front_matter`**

Add the following function in `actions.rs`, after `apply_edit_block_body`:

```rust
fn apply_edit_front_matter(
    doc: &mut EditorDoc,
    new_fm: lopress_core::FrontMatter,
) -> Option<(BlockAction, BlockAction)> {
    if doc.front_matter == new_fm {
        return None;
    }
    let old_fm = std::mem::replace(&mut doc.front_matter, new_fm.clone());
    Some((
        BlockAction::EditFrontMatter { new_front_matter: new_fm },
        BlockAction::EditFrontMatter { new_front_matter: old_fm },
    ))
}
```

- [ ] **Step 4: Write the failing tests**

Append to `actions.rs`'s existing test module (find the `#[cfg(test)] mod tests { ... }` block; if none exists in this file, append at the end of the file):

```rust
    #[test]
    fn apply_edit_front_matter_records_inverse() {
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("body")])],
            front_matter: lopress_core::FrontMatter {
                title: Some("old".to_string()),
                ..Default::default()
            },
        };
        let new_fm = lopress_core::FrontMatter {
            title: Some("new".to_string()),
            ..Default::default()
        };
        let (canonical, inverse) =
            apply_edit_front_matter(&mut doc, new_fm.clone()).expect("recorded");
        assert!(matches!(canonical, BlockAction::EditFrontMatter { .. }));

        // Apply the inverse: the doc's title should return to "old".
        if let BlockAction::EditFrontMatter { new_front_matter } = inverse {
            apply_edit_front_matter(&mut doc, new_front_matter);
        } else {
            unreachable!();
        }
        assert_eq!(doc.front_matter.title.as_deref(), Some("old"));
    }

    #[test]
    fn apply_edit_front_matter_no_op_returns_none() {
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("body")])],
            front_matter: lopress_core::FrontMatter {
                title: Some("same".to_string()),
                ..Default::default()
            },
        };
        let same = lopress_core::FrontMatter {
            title: Some("same".to_string()),
            ..Default::default()
        };
        assert!(apply_edit_front_matter(&mut doc, same).is_none());
    }
```

If `EditorBlock`, `EditorDoc`, `InlineRun`, `BlockAction` aren't already in scope in the test module, add `use super::*;` and the relevant `use` lines at the top of the test module.

- [ ] **Step 5: Verify tests pass**

```
cargo test -p lopress-editor apply_edit_front_matter
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 6: Rewire `inspector_view` to dispatch through `on_action`**

In `crates/lopress-editor/src/ui/inspector.rs`, the `inspector_view` function currently takes `mark_dirty: Rc<dyn Fn()>`. Replace this with `on_action: ActionSink`.

Update the function signature:

```rust
pub fn inspector_view(
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: crate::ui::blocks::inline_editor::ActionSink,
) -> impl IntoView {
```

And the `form` function it delegates to:

```rust
fn form(
    doc: EditorDoc,
    path: Option<PathBuf>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_action: crate::ui::blocks::inline_editor::ActionSink,
) -> AnyView {
```

Add a small helper at the top of `form` (or as a free `fn` in this file) that builds a new front matter via a mutator closure and dispatches the action only when the result differs from the current model state:

```rust
fn dispatch_fm_edit(
    current_doc: RwSignal<Option<EditorDoc>>,
    on_action: &crate::ui::blocks::inline_editor::ActionSink,
    mutate: impl FnOnce(&mut lopress_core::FrontMatter),
) {
    let current = current_doc.with_untracked(|d| d.as_ref().map(|doc| doc.front_matter.clone()));
    let Some(mut new_fm) = current else { return; };
    let before = new_fm.clone();
    mutate(&mut new_fm);
    if new_fm != before {
        on_action(crate::actions::BlockAction::EditFrontMatter { new_front_matter: new_fm });
    }
}
```

- [ ] **Step 7: Replace each per-field effect**

For each of the six front-matter effects in `form` (title, slug, date, tags, draft, description) and the `Sync from H1` handler, replace the current `current_doc.update(...)`+`mark_dirty()` pattern with a `dispatch_fm_edit` call.

**Title:**

```rust
    let on_action_for_title = on_action.clone();
    create_effect(move |_| {
        let new_title = title_buf.get();
        dispatch_fm_edit(current_doc, &on_action_for_title, |fm| {
            fm.title = if new_title.is_empty() { None } else { Some(new_title.clone()) };
        });
    });
```

**Slug:**

```rust
    let on_action_for_slug = on_action.clone();
    create_effect(move |_| {
        let new_slug = slug_buf.get();
        dispatch_fm_edit(current_doc, &on_action_for_slug, |fm| {
            fm.slug = if new_slug.is_empty() { None } else { Some(new_slug.clone()) };
        });
    });
```

**Date:** preserve the existing parse-then-validate logic; only the mutation site changes:

```rust
    let on_action_for_date = on_action.clone();
    create_effect(move |_| {
        let raw = date_buf.get();
        if raw.trim().is_empty() {
            date_invalid.set(false);
            let on_action = on_action_for_date.clone();
            dispatch_fm_edit(current_doc, &on_action, |fm| { fm.date = None; });
            return;
        }
        match NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
            Ok(d) => {
                date_invalid.set(false);
                let on_action = on_action_for_date.clone();
                dispatch_fm_edit(current_doc, &on_action, |fm| { fm.date = Some(d); });
            }
            Err(_) => {
                date_invalid.set(true);
            }
        }
    });
```

**Tags:**

```rust
    let on_action_for_tags = on_action.clone();
    create_effect(move |_| {
        let raw = tags_buf.get();
        let tags: Vec<String> = raw
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        dispatch_fm_edit(current_doc, &on_action_for_tags, |fm| { fm.tags = tags.clone(); });
    });
```

**Draft:**

```rust
    let on_action_for_draft = on_action.clone();
    create_effect(move |_| {
        let v = draft_sig.get();
        dispatch_fm_edit(current_doc, &on_action_for_draft, |fm| { fm.draft = v; });
    });
```

**Description:**

```rust
    let on_action_for_desc = on_action.clone();
    create_effect(move |_| {
        let new_desc = desc_buf.get();
        dispatch_fm_edit(current_doc, &on_action_for_desc, |fm| {
            fm.description = if new_desc.is_empty() { None } else { Some(new_desc.clone()) };
        });
    });
```

**Sync from H1:**

```rust
    let on_action_for_sync = on_action.clone();
    let on_sync = move || {
        if let Some(text) = h1_text.get_untracked() {
            title_buf.set(text.clone());
            let on_action = on_action_for_sync.clone();
            dispatch_fm_edit(current_doc, &on_action, |fm| {
                fm.title = Some(text.clone());
            });
        }
    };
```

- [ ] **Step 8: Update the caller in `ui/mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, find the `inspector_view(...)` call inside `editing_view` and replace the `mark_dirty` arg with `on_action.clone()`:

```rust
    let inspector = inspector_view(current_doc, current_path, on_action.clone());
```

- [ ] **Step 9: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 10: Verify tests pass**

```
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 11: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Open a document. Click the Title field, type new text, blur. Click into a block editor. Press Ctrl+Z. The Title reverts.
- Edit Slug, blur, Ctrl+Z — reverts.
- Click "Sync from H1". The Title syncs. Ctrl+Z — Title reverts to pre-sync value.
- Interleave: edit a paragraph (Edit A), edit Title (Edit B). Ctrl+Z reverts Title. Ctrl+Z again reverts paragraph.

- [ ] **Step 12: Commit**

```
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/src/ui/inspector.rs \
        crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
feat(editor): make front-matter edits undoable via EditFrontMatter action

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Toolbar click diagnosis (Section 2 — investigation)

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (temporary)

**Goal:** Determine whether toolbar button `.action()` closures are firing on real mouse clicks. The branching outcome decides whether to implement Task 9's structural fix or pursue an alternative root cause.

This task does not produce a commit on its own — it produces a diagnosis recorded in the next task's commit message.

- [ ] **Step 1: Add a debug print to every toolbar button's action**

In `crates/lopress-editor/src/ui/toolbar.rs`, add `eprintln!` instrumentation:

```rust
// At the top of the type-selector button `.action()`:
.action(move || {
    eprintln!("[TOOLBAR DBG] type button fired: {lbl_str}");
    // …existing body…
})
```

```rust
// At the top of each inline-flag toggle button (toggle_button):
.action(move || {
    eprintln!("[TOOLBAR DBG] flag button fired: {lbl}");
    // …existing body…
})
```

```rust
// At the top of the Delete button:
.action(move || {
    eprintln!("[TOOLBAR DBG] delete button fired");
    // …existing body…
})
```

- [ ] **Step 2: Build and run**

```
cargo run
```

- [ ] **Step 3: Click every category of button with a real mouse**

In the running editor:
- Open a document and click into a paragraph (toolbar appears).
- Click H1, H2, Code, UL one at a time. Watch the terminal.
- With a non-empty selection, click B and `</>`. Watch the terminal.
- Click the Delete (`x`) button. Watch the terminal.

- [ ] **Step 4: Record the outcome**

Three possible outcomes; the next steps differ accordingly:

**Outcome A — no `[TOOLBAR DBG]` lines appear for any button.**
The editor surface is intercepting the click before it reaches the button. **Proceed to Task 9** as written. The diagnosis line for Task 9's commit message: "diagnosis: no button .action() closures fire on real mouse clicks; clicks are absorbed by the editor surface".

**Outcome B — `[TOOLBAR DBG]` lines appear, but the document doesn't change.**
The action fires but the result isn't applied. The structural fix in Task 9 is the wrong fix. Investigate:
- Does `/state` show the kind change immediately after the click? If yes, the model is changing but the visual rebuild isn't picking it up (pane_key issue).
- Is `focus_pub.editor_and_spans.get_untracked()` returning `None` because focus moved to the button before `.action()` reads it? Check by adding `dbg!(focus_pub.editor_and_spans.get_untracked().is_some())` inside the action.

Document the outcome and pick the appropriate fix. **Skip Task 9** (the structural move would be cosmetic at best) and write a new task addressing the actual cause.

**Outcome C — `[TOOLBAR DBG]` lines appear for some buttons but not others.**
Mixed cause. Document which buttons fire and which don't. The likely split is type-selector vs. inline-flag (different `.action()` callsites in `toolbar.rs`); investigate the difference. Pick the right fix for each.

- [ ] **Step 5: Remove the debug prints**

Revert the `eprintln!` lines added in Step 1. The diagnosis is captured in the outcome notes (used in Task 9's commit message), not in the source.

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 6: Do not commit yet**

The diagnosis lives in the implementer's notes. Task 9 commits the actual fix with the diagnosis baked into its commit message.

---

### Task 9: Toolbar outside the focus border (Section 2 — structural fix)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`

**Goal:** Move the focus border to wrap only the editor `row`, leaving `toolbar_slot` outside the click-capturing surface. This is the fix for **Outcome A** from Task 8 — only implement if Task 8 produced Outcome A.

If Task 8 produced Outcome B or C, skip this task and replace it with the fix indicated by that outcome.

- [ ] **Step 1: Update the plugin return path**

In `block_view`, locate the plugin early-return (around line 88):

```rust
    return v_stack((toolbar_slot, plugin_view))
        .style(move |s| {
            let focused = focus_pub.block.get() == Some(block_id);
            let s = s.width_full().border(1.0).border_radius(4.0);
            if focused {
                s.border_color(FOCUS_BORDER)
            } else {
                s.border_color(floem::peniko::Color::TRANSPARENT)
            }
        })
        .into_any();
```

Replace with — same logic, border applied to `plugin_view` instead of the outer `v_stack`:

```rust
    let plugin_with_border = plugin_view.style(move |s| {
        let focused = focus_pub.block.get() == Some(block_id);
        let s = s.width_full().border(1.0).border_radius(4.0);
        if focused {
            s.border_color(FOCUS_BORDER)
        } else {
            s.border_color(floem::peniko::Color::TRANSPARENT)
        }
    });
    return v_stack((toolbar_slot, plugin_with_border))
        .style(|s| s.width_full())
        .into_any();
```

- [ ] **Step 2: Update the normal return path**

Locate the final return in `block_view`:

```rust
    v_stack((toolbar_slot, row))
        .style(move |s| {
            let focused = focus_pub.block.get() == Some(block_id);
            let s = s.width_full().border(1.0).border_radius(4.0);
            if focused {
                s.border_color(FOCUS_BORDER)
            } else {
                s.border_color(floem::peniko::Color::TRANSPARENT)
            }
        })
        .into_any()
```

Replace with:

```rust
    let row_with_border = row.style(move |s| {
        let focused = focus_pub.block.get() == Some(block_id);
        let s = s.width_full().border(1.0).border_radius(4.0);
        if focused {
            s.border_color(FOCUS_BORDER)
        } else {
            s.border_color(floem::peniko::Color::TRANSPARENT)
        }
    });
    v_stack((toolbar_slot, row_with_border))
        .style(|s| s.width_full())
        .into_any()
```

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 4: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Focus a paragraph. Click H1 in the toolbar — block changes to a heading. `/state` shows `kind: "Heading1"`.
- With text selected, click Bold — `/state` shows `bold: true` on the selected run.
- Click Delete (`x`) — the block is removed. `/state` shows the block is gone.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "$(cat <<'EOF'
fix(editor): move focus border off toolbar so clicks reach the buttons

Diagnosis from prior step: no toolbar .action() closure fired on real
mouse clicks; the editor surface inside the focus-border container was
absorbing them. Moving the border to wrap only the editor row leaves
the toolbar's hit-test surface intact.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

(If Task 8's outcome was different, rewrite the body of the commit message to match.)

---

### Task 10: Toolbar visual separation (Section 9)

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`

**Goal:** Give the toolbar its own background panel with explicit separation from the focused block.

- [ ] **Step 1: Update the `button_row` style**

In `crates/lopress-editor/src/ui/toolbar.rs`, locate the `button_row` style (around line 139):

```rust
    let button_row = h_stack_from_iter(buttons).style(|s| {
        s.padding_horiz(6.)
            .padding_vert(4.)
            .gap(4.)
            .background(Color::rgb8(245, 245, 248))
            .border(1.)
            .border_color(Color::rgb8(220, 220, 226))
            .border_radius(4.)
            .margin_bottom(4.)
    });
```

Replace with:

```rust
    let button_row = h_stack_from_iter(buttons).style(|s| {
        s.padding_horiz(8.)
            .padding_vert(4.)
            .gap(4.)
            .background(Color::rgb8(252, 252, 254))
            .border(1.)
            .border_color(Color::rgb8(220, 220, 226))
            .border_radius(6.)
            .margin_bottom(6.)
    });
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Focus a block. Screenshot the toolbar. It should read as a distinct floating affordance: own background, own border, a clear gap from the block beneath it.
- The focus border on the block does not wrap the toolbar (this depends on Task 9 having landed first).

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "$(cat <<'EOF'
fix(editor): give toolbar its own panel with explicit separation from block

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Layout jump on focus (Section 6)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`

**Goal:** Reserve a fixed-height slot above every block for the toolbar so focus changes don't shift the document.

- [ ] **Step 1: Add the toolbar-height constant**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add a new constant near the existing `FOCUS_BORDER` constant:

```rust
/// Reserved height for the toolbar slot above each block. Matches the
/// rendered toolbar's natural height including its bottom margin. Reserving
/// the slot on every block prevents the document from shifting when focus
/// moves between blocks.
const TOOLBAR_HEIGHT_PX: f32 = 36.;
```

(If after Task 10 the actual rendered height differs, measure with the driving skill and update this constant before committing.)

- [ ] **Step 2: Apply the height to the `toolbar_slot` in `block_view`**

Find the `let toolbar_slot = { … dyn_container … }` definition (there are two — one for the plugin path, one for the normal path; both look similar). For each, change the final `.style(|s| s.width_full())` to:

```rust
        .style(|s| s.width_full().height(TOOLBAR_HEIGHT_PX))
```

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 4: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Open a document with at least three visible blocks.
- Note the Y coordinate of the third block.
- Click between the first, second, and third blocks. Screenshot each focus state.
- The Y coordinate of the third block does not change between focuses.

If the toolbar visibly clips inside its slot, increase `TOOLBAR_HEIGHT_PX` by a few pixels and reverify.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "$(cat <<'EOF'
fix(editor): reserve toolbar height slot on every block

Eliminates the layout jump when focus moves between blocks. Cost is
~36 px of always-present whitespace above every block; a floating
toolbar overlay would avoid the cost but is out of scope here.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: Empty list items affordance (Section 8)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs`

**Goal:** When a list item is empty, show a faint placeholder label overlaid on the (still-mounted) editor so the user sees the item is editable. The editor stays mounted at all times — clicking the row lands on the editor and starts the placeholder disappearing.

Implementation approach: render the editor and an absolute-positioned placeholder label inside a `stack`. The placeholder is only mounted when the editor's text is empty (via `dyn_container`). The placeholder carries no event handlers; the editor underneath receives the click.

If during implementation the placeholder turns out to capture clicks (floem's pointer hit-testing on absolute-positioned labels may bubble), wrap the placeholder in a container with `s.pointer_events_none()` if floem provides it, or fall back to laying out the placeholder as an inline sibling of the editor (visible only when empty) rather than an overlay. The user-visible behavior — empty item shows hint text, clicking the row focuses the editor — must hold either way.

- [ ] **Step 1: Add the placeholder constant**

Near the top of `crates/lopress-editor/src/ui/blocks/list.rs`, add:

```rust
/// Greyed hint text shown in empty list items so users see they're editable.
const EMPTY_ITEM_PLACEHOLDER: &str = "Empty item — type to fill";
```

- [ ] **Step 2: Wrap the per-item editor in a stack with a conditional placeholder overlay**

In `editable_list_view`, locate the row builder where each list item's editor is mounted (the `.map()` closure that produces one row per item). The current shape is approximately:

```rust
            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                editor.style(|s| s.flex_grow(1.0)),
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
```

Replace with a `stack` that puts the editor and the placeholder in overlapping z-order, the placeholder only mounted when text is empty:

```rust
            let editor_sig_for_overlay = editor_sig;
            let placeholder_overlay = dyn_container(
                move || editor_sig_for_overlay.with(|ed| ed.doc().text().is_empty()),
                move |is_empty| {
                    if is_empty {
                        label(|| EMPTY_ITEM_PLACEHOLDER.to_string())
                            .style(|s| {
                                s.color(floem::peniko::Color::rgb8(160, 160, 160))
                                    .font_size(15.)
                                    .padding_horiz(2.)
                                    .position(floem::style::Position::Absolute)
                                    .inset_left(0.)
                                    .inset_top(0.)
                            })
                            .into_any()
                    } else {
                        empty().into_any()
                    }
                },
            );

            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                floem::views::stack((
                    editor.style(|s| s.flex_grow(1.0).width_full()),
                    placeholder_overlay,
                ))
                .style(|s| s.flex_grow(1.0)),
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
```

`editor_sig` and `editor` come from the existing per-item state setup above — preserve them. If `floem::views::stack` is already imported in this file via `use floem::views::{...}` you don't need to fully qualify it.

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors. If the `pointer_events_none` (or equivalent) style is needed and not available in this floem version, the placeholder may intercept clicks; proceed to Step 4 to verify and adjust if needed.

- [ ] **Step 4: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Open `lopress-listtest/src/posts/listtest.md` (it has multiple empty list items). Each empty item should now show the placeholder text in grey.
- Click an empty item's row. Focus should move to the editor; the placeholder should disappear once you type. /state should reflect the typed text once committed.
- If clicking the placeholder text doesn't focus the editor (placeholder is intercepting the click), wrap the `placeholder_overlay` in a container that disables hit-testing. Floem 0.2 doesn't have an obvious `pointer_events: none` style — if the overlay approach fails, fall back to an inline approach:

```rust
            // Fallback: inline placeholder when empty, editor only when non-empty.
            let editor_sig_for_swap = editor_sig;
            let item_body = dyn_container(
                move || editor_sig_for_swap.with(|ed| ed.doc().text().is_empty()),
                move |is_empty| {
                    if is_empty {
                        h_stack((
                            label(|| EMPTY_ITEM_PLACEHOLDER.to_string())
                                .style(|s| s.color(floem::peniko::Color::rgb8(160, 160, 160)).font_size(15.)),
                        ))
                        .style(|s| s.padding_horiz(2.).flex_grow(1.0))
                        .on_click_stop(move |_| { editor_sig_for_swap.with_untracked(|ed| ed.editor_view_id.get().map(|id| id.request_focus())); })
                        .into_any()
                    } else {
                        editor.clone().style(|s| s.flex_grow(1.0).width_full()).into_any()
                    }
                },
            );
            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                item_body,
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
```

In the fallback path, swapping the editor view between mounted and unmounted is acceptable because the click on the placeholder routes focus back to the editor (which remounts on text change). Record which approach (overlay or fallback swap) was used in the commit message.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/list.rs
git commit -m "$(cat <<'EOF'
fix(editor): show placeholder hint in empty list items

[overlay | inline-swap] approach was used; see body.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

Replace `[overlay | inline-swap]` with whichever approach actually shipped.

---

## Verification ordering recap

The tasks above match the spec's "Verification ordering":

1. Task 1 — Section 7 (toolbar ordering) — trivial reorder
2. Task 2 — Section 11 (recents dedup)
3. Task 3 — Section 10 (false dirty marks)
4. Task 4 — Section 12 (`/open`+`/close`) — unblocks automated repro for the rest
5. Task 5 — Section 1 (slash menu regression)
6. Task 6 — Section 4 + Section 5 (lang Enter / Escape, plus Section 5 verify)
7. Task 7 — Section 3 (front-matter undo)
8. Task 8 — Section 2 investigation (diagnosis only, no commit)
9. Task 9 — Section 2 structural fix (or alternative based on Task 8's outcome)
10. Task 10 — Section 9 (toolbar visual separation) — builds on Task 9
11. Task 11 — Section 6 (layout jump) — builds on Task 9 / Task 10
12. Task 12 — Section 8 (empty list items)

---

## Performance contract

None of these fixes should regress the hot paths called out in the memory-optimization review (`commit_from_editor`, `apply_edit_block_body`, `canonicalize_body`). In particular:

- **Task 7 (front-matter undo):** The new `EditFrontMatter` variant carries a `FrontMatter` directly (not boxed) — front matter is small (KB at most) and a direct clone is fine.
- **Task 3 (dirty gating):** A single `Option::is_some()` check; no measurable overhead.
- **Task 4 (ctrl endpoints):** `#[cfg(debug_assertions)]` only — no release impact.
- **Task 11 (toolbar height slot):** Reserves 36 px per block in layout. No allocation impact; the slot exists whether populated or not.
- **Task 12 (list placeholder):** Adds one `dyn_container` per empty list item — fires when the item's text becomes empty/non-empty, which is rare. The placeholder label is mounted only when empty.

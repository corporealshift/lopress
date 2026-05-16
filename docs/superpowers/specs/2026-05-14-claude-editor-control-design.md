# Claude Editor Control — Design Spec

**Date:** 2026-05-14
**Status:** Approved
**Branch:** experimental-claude-control
**Goal:** Allow Claude (running in Claude Code on Windows) to see, control, and interact with the running Floem-based editor to reproduce bugs and experience issues faster.

---

## Problem Statement

Debugging GUI editor issues requires manually launching the editor, reproducing a sequence of interactions, and observing the result. Claude cannot participate in this loop today — it can only read and write code. This spec adds a lightweight dev-only HTTP control server to the editor so Claude can:

1. **See** the editor — get a screenshot of the running window
2. **Read doc state** — get the current document's blocks as JSON
3. **Apply document actions** — programmatically drive `BlockAction`s (same path real editing uses)
4. **Inject GUI input** — send keystrokes and mouse clicks via the OS so real input paths are exercised

---

## Scope

- **In scope:** HTTP server, doc state snapshot, PNG screenshot, `BlockAction` dispatch, keyboard/mouse injection, `serde` on action types
- **Out of scope:** auth/security (localhost-only, dev builds only), multi-window support, streaming/websocket updates, headless/offscreen rendering

---

## Architecture

### New module: `crates/lopress-editor/src/ctrl/`

Three files inside the module, all gated with `#[cfg(debug_assertions)]`:

| File | Responsibility |
|------|---------------|
| `ctrl/mod.rs` | HTTP server thread (tiny_http). Routes requests to snapshot/screenshot/action/input handlers. |
| `ctrl/bridge.rs` | Floem-side wiring. Owns the `Arc<Mutex<String>>` doc snapshot and the `crossbeam::Sender<BlockAction>`. Sets up `create_effect` (snapshot) and `update_signal_from_channel` (actions). |
| `ctrl/input.rs` | Key string parsing → `VIRTUAL_KEY`. `SendInput` + `SetForegroundWindow` wrappers for text, key chords, and mouse clicks. |

### Threading model

```
HTTP thread (tiny_http, port 7878)
    │
    ├─ GET /state        reads  Arc<Mutex<String>> ←── create_effect (Floem thread)
    │                                                    watches doc RwSignal, re-serializes on change
    │
    ├─ GET /screenshot   FindWindowW("lopress") → BitBlt → PNG
    │
    ├─ POST /action      crossbeam Sender<BlockAction>
    │                        └─ update_signal_from_channel → RwSignal<Option<BlockAction>>
    │                               └─ create_effect → actions::apply(doc_signal, action)
    │
    ├─ POST /input       SetForegroundWindow + SendInput  (OS-routed, no Floem needed)
    └─ POST /click       ClientToScreen + SendInput mouse event
```

### Startup sequence

1. `lib.rs` (`run()`) creates the bridge struct before `Application::new()`:
   - `Arc<Mutex<String>>` for the doc snapshot (initialized to `"{}"`)
   - `crossbeam::channel::unbounded::<BlockAction>()` → holds `Sender`, passes `Receiver` into window closure
2. Spawns the HTTP server thread, passing `Arc` clone + `Sender` clone
3. Inside the `Application::new().window(|window_id| { … })` closure (reactive scope is live here):
   - `bridge::wire(cx, doc_signal, snapshot_arc, action_rx)` sets up the two effects
4. `Application::run()` blocks; HTTP server runs concurrently on its thread

---

## HTTP API

All endpoints on `localhost:7878`. Enabled only in debug builds.

### `GET /ping`
Returns `200 "ok"`. Liveness check.

### `GET /state`
Returns `200` with `Content-Type: application/json`.

```json
{
  "doc_open": true,
  "path": "C:/…/src/posts/example.md",
  "blocks": [
    { "id": 1, "kind": "Heading1", "text": "My Post" },
    { "id": 2, "kind": "Paragraph", "text": "Hello world" },
    { "id": 3, "kind": "Code", "lang": "rust", "text": "fn main() {}" },
    { "id": 4, "kind": "List", "text": "item one\nitem two" }
  ]
}
```

When no document is open: `{ "doc_open": false, "path": null, "blocks": [] }`.

`kind` is a flat string: `"Paragraph"`, `"Heading1"`–`"Heading6"`, `"Code"`, `"List"`. For code blocks, `"lang"` is included. For list blocks, item texts are joined with `"\n"`. This is a read-only projection sufficient for Claude to reason about block ids and content.

### `GET /screenshot`
Returns `200` with `Content-Type: image/png` and raw PNG bytes.

Capture steps:
1. `FindWindowW(None, "lopress")` — find the window by title
2. `GetClientRect` — get client area dimensions
3. `GetDC(hwnd)` + `CreateCompatibleDC` + `CreateCompatibleBitmap`
4. `BitBlt` — copy window pixels into memory DC
5. `GetDIBits` — pull BGRA bytes into `Vec<u8>`
6. Convert BGRA → RGBA, encode to PNG via `image` crate

Returns `503 "window not found"` if the window is not visible.

### `POST /action`
Body: JSON-serialized `BlockAction` with a `"type"` discriminant field.

```json
{ "type": "EditInline", "block_id": 2, "new_runs": [{ "text": "Hi", "bold": true, "italic": false, "code": false, "link": null }] }
{ "type": "Split", "block_id": 2, "byte_offset": 5 }
{ "type": "Delete", "block_id": 2 }
{ "type": "ChangeType", "block_id": 2, "new_kind": { "type": "Heading", "level": 2 } }
```

Returns `200 "ok"` on success, `400 "<message>"` on parse error.

The action is sent through the crossbeam channel, received by `update_signal_from_channel`, which wakes the Floem reactive system and runs the `create_effect` that calls `actions::apply`.

### `POST /input`
Body:
```json
{ "type": "text", "text": "hello world" }
{ "type": "keys", "keys": "ctrl+b" }
{ "type": "keys", "keys": "shift+enter" }
{ "type": "keys", "keys": "ctrl+shift+k" }
```

Supported modifier tokens: `ctrl`, `shift`, `alt`. Supported key tokens: letters (`a`–`z`), digits (`0`–`9`), `enter`, `backspace`, `delete`, `tab`, `escape`, `up`, `down`, `left`, `right`, `home`, `end`, `f1`–`f12`.

Calls `SetForegroundWindow` before injecting to ensure the editor has focus.

Returns `200 "ok"` or `400 "unknown key: <token>"`.

### `POST /click`
Body:
```json
{ "x": 400, "y": 300 }
```

Coordinates are relative to the window's client area (top-left = 0,0). Converted to screen coordinates via `ClientToScreen`, then fires `MOUSEEVENTF_LEFTDOWN` + `MOUSEEVENTF_LEFTUP` via `SendInput`. Calls `SetForegroundWindow` first.

Returns `200 "ok"` or `503 "window not found"`.

---

## Serde on Action Types

`BlockAction`, `BlockKind`, `InlineRun`, `ListItem`, and `BlockId` in `actions.rs` and `model/types.rs` get `#[derive(Serialize, Deserialize)]`. `serde` is already in `Cargo.toml`. The enum discriminant is `#[serde(tag = "type")]`.

`BlockId` is a `u64` newtype backed by an atomic counter. It will be serialized transparently as a JSON number (e.g. `1`, `42`) via `#[serde(transparent)]`. The `/state` block ids and the `POST /action` `block_id` field use the same numeric representation.

---

## Files Changed

### New (all `#[cfg(debug_assertions)]`)
| File | Contents |
|------|----------|
| `crates/lopress-editor/src/ctrl/mod.rs` | HTTP server thread, request dispatch |
| `crates/lopress-editor/src/ctrl/bridge.rs` | `CtrlBridge` struct, `wire()` fn for Floem-side effects |
| `crates/lopress-editor/src/ctrl/input.rs` | Key parsing, `SendInput`, `SetForegroundWindow` wrappers |

### Modified
| File | Change |
|------|--------|
| `crates/lopress-editor/src/lib.rs` | Create bridge, spawn HTTP server (both inside `#[cfg(debug_assertions)]`), pass receiver into window closure |
| `crates/lopress-editor/src/actions.rs` | `#[derive(Serialize, Deserialize)]` on `BlockAction` |
| `crates/lopress-editor/src/model/types.rs` | `#[derive(Serialize, Deserialize)]` on `BlockKind`, `InlineRun`, `ListItem`, `BlockId` |
| `crates/lopress-editor/Cargo.toml` | Add `tiny_http`, `image` under `[dependencies]` (always present; server only activates in debug builds via `cfg`) |

---

## Testing

### Manual (Claude-driven)
1. `cargo run` — editor launches, check `Invoke-RestMethod http://localhost:7878/ping` returns `ok`
2. Open a document in the editor UI, then `GET /state` — verify blocks match
3. `GET /screenshot` — save PNG, verify it shows the editor window
4. `POST /action` with `EditInline` — verify text changes in the editor
5. `POST /input` with `{"type":"text","text":"hello"}` — verify text appears at cursor
6. `POST /input` with `{"type":"keys","keys":"ctrl+b"}` — verify bold toggle fires
7. `POST /click` at a known block position — verify cursor moves

### Release build check
`cargo build --release` must compile cleanly with the ctrl module absent. CI already enforces clippy denials.

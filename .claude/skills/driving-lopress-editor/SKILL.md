---
name: driving-lopress-editor
description: Use when debugging, reproducing a bug in, or verifying behavior of the running lopress editor GUI — reading document state, taking screenshots, applying document actions, or injecting keyboard/mouse input via the debug HTTP control server on 127.0.0.1:7878.
---

# Driving the lopress Editor

## Overview

The lopress editor ships a **debug-only HTTP control server** so Claude can see and drive the running GUI instead of only reading code. It lives in `crates/lopress-editor/src/ctrl/`, is gated behind `#[cfg(debug_assertions)]`, and listens on `127.0.0.1:7878`.

**Core principle:** prefer `/action` for deterministic document mutations (it runs the real `actions::apply` path); use `/input` and `/click` only when you specifically need to exercise OS-level input paths (keybindings, focus, free-text typing).

## When to use

- Reproducing or verifying a GUI editor bug live
- Confirming a code change actually affects the running editor
- Checking document/block state without reading the file from disk
- Capturing a screenshot of the editor for visual inspection

**Environment: Full Windows desktop with display.** The editor window is visible and all endpoints including `/action` work once the process is running. This is not headless CI.

**Not for:** release builds (the server is compiled out — use `cargo run`, never `--release`), or non-Windows hosts for `/screenshot`, `/input`, `/click` (those return errors off Windows).

## Prerequisites

1. **Run from the repo root with plain `cargo run`.** The binary is the root `lopress` crate (it calls `lopress_editor::run()`). Do **not** use `cargo run -p lopress-editor` — that's a library with no bin target and fails with `error: a bin target must be available for cargo run`. Do **not** use `--release` (the server is `#[cfg(debug_assertions)]` and gets compiled out).
2. **Launch it as a persistent background process, separate from probing it.** The GUI event loop never returns, so a foreground `cargo run` blocks forever. Start it detached (e.g. Claude Code's `run_in_background`), then probe from later calls. Do **not** cram `cargo run & sleep 5; curl` into one command — the cold first build takes **minutes**, and `sleep 5` (or `timeout 10 cargo run`) fires the probe mid-compile and then kills the editor when the command ends.
3. **Wait for the server by polling `/ping` until it returns `ok`** — don't sleep a fixed few seconds. The first debug build compiles the whole workspace before the window appears and `[ctrl] listening on http://127.0.0.1:7878` prints.
4. **The window must be visible and not minimized.** `/ping` and `/state` answer on an independent HTTP thread, but `/open`, `/action`, and `/close` are serviced by the floem UI thread's event loop, which **stalls when the window is hidden or minimized** — those endpoints then return `504 "editor did not respond"` while `/ping` still says `ok`. Launch a normal visible window; never `Start-Process -WindowStyle Hidden`. See [Opening a workspace and documents](#opening-a-workspace-and-documents-the-first-open) for the open flow.
5. The window title must be `lopress` — `/screenshot`, `/input`, `/click` find it by that exact title.
6. Open a document (see below). Until then `/state` reports `{"doc_open":false,...}` and `/action` returns `409 no_document`.

## Quick reference

All endpoints on `http://127.0.0.1:7878`. On Windows use `Invoke-RestMethod` / `Invoke-WebRequest`.

| Endpoint | Method | Purpose | Returns |
|---|---|---|---|
| `/ping` | GET | Liveness check | `ok` |
| `/state` | GET | Current doc as JSON | `{doc_open, path, blocks:[{id,kind,text,lang?}]}` |
| `/screenshot` | GET | PNG of the window | `image/png` bytes; `503` if window not found |
| `/action` | POST | Apply a `CtrlAction` to the doc | `200 {"status":"dispatched"}` / `409 no_document` / `422 block_not_found` / `400 parse error` |
| `/open` | POST | Open a doc by path. Body `{ "path": "..." }`. Absolute or workspace-relative. | `200 {"status":"opened"}` / `404 {"status":"not_found"}` / `409 {"status":"no_workspace"}` |
| `/close` | POST | Close the current doc and return to the welcome view. | `200 {"status":"closed"}` / `409 {"status":"no_workspace"}` |
| `/input` | POST | Inject text or a key chord | `ok` / `400` |
| `/click` | POST | Click at client-area coords | `ok` / `400` |

## POST /action — document mutations

Body is a JSON `CtrlAction` with a `"type"` discriminant. `block_id` is the raw `u64` from `/state`. Variants:

```json
{"type":"Split","block_id":2,"byte_offset":5}
{"type":"MergeWithPrev","block_id":3}
{"type":"Delete","block_id":2}
{"type":"Move","block_id":2,"to_index":0}
{"type":"ChangeType","block_id":2,"new_kind":{"type":"Heading","level":2}}
{"type":"EditInline","block_id":2,"new_runs":[{"text":"Hi","bold":true,"italic":false,"code":false,"link":null}]}
{"type":"EditCode","block_id":4,"new_text":"fn main() {}"}
{"type":"EditAttrs","block_id":4,"new_attrs":{"lang":"rust"}}
```

`new_kind` types: `Paragraph`, `Heading {level}`, `Code {lang}`, `List {ordered}`.

`/action` blocks until the editor reports an outcome and answers with JSON: `200 {"status":"dispatched"}` when the action reached a real block and was routed to the editor; `422 {"status":"block_not_found","block_id":N}` when the id does not exist in the open document; `409 {"status":"no_document"}` when no document is open. (`200`/dispatched does not guarantee the document changed — a no-op action such as `Move` to the same position still dispatches.) On Windows, `Invoke-RestMethod` throws on `4xx` codes — that thrown error is the signal the action did not apply; previously such cases were silently dropped with a `200 ok`.

PowerShell example:
```powershell
$body = '{"type":"ChangeType","block_id":2,"new_kind":{"type":"Heading","level":2}}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method Post -Body $body
```

## Opening a workspace and documents (the first open)

There is **no separate "open workspace" endpoint** — opening a document bootstraps the workspace. A workspace is the nearest ancestor directory of the file that contains a `lopress.toml`. The flow from a cold welcome screen:

1. **First `/open` must use an absolute path** to a file that lives inside a workspace (some ancestor dir has `lopress.toml`). `/open` walks up from the file, finds that `lopress.toml`, opens the workspace, then loads the document. After this the editor has an active workspace.
2. **Subsequent `/open` calls may use workspace-relative paths** (e.g. `posts/foo.md`) — they resolve against the active workspace root.
3. A **relative** path with no workspace open yet → `409 {"status":"no_workspace"}`. So you cannot start with a relative path; bootstrap with an absolute one first.
4. An **absolute** path with no `lopress.toml` in any ancestor → `404 {"status":"not_found"}` (it's not inside a workspace).

So: always make the *first* open an absolute path into a real workspace; only then switch to relative paths.

## POST /open — open a document

Body is `{ "path": "..." }`. Absolute (bootstraps/opens the workspace) or relative to the already-open workspace.

```powershell
# Open by absolute path:
$body = '{"path":"C:\\Users\\corpo\\Documents\\lopress-listtest\\src\\posts\\listtest.md"}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/open -Method Post -Body $body
# Expected: {"status":"opened"}; /state shows doc_open:true.

# Relative path before workspace open:
$body = '{"path":"posts/foo.md"}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/open -Method Post -Body $body
# Expected: {"status":"no_workspace"} (409).

# Nonexistent path:
$body = '{"path":"C:\\nonexistent.md"}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/open -Method Post -Body $body
# Expected: {"status":"not_found"} (404).
```

## POST /close — close the current document

Closes the open document and returns the editor to the welcome view.

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:7878/close -Method Post
# Expected: {"status":"closed"}; /state shows doc_open:false.

# Close with no document open:
Invoke-RestMethod -Uri http://127.0.0.1:7878/close -Method Post
# Expected: {"status":"no_workspace"} (409).
```

## POST /input and /click — OS input injection

```json
{"type":"text","text":"hello world"}
{"type":"keys","keys":"ctrl+b"}
{"type":"keys","keys":"shift+enter"}
```
`/click` body: `{"x":400,"y":300}` — client-area coords, top-left = 0,0.

Modifiers: `ctrl`, `shift`, `alt`. Keys: `enter`, `backspace`, `delete`, `tab`, `escape`, `space`, `up`/`down`/`left`/`right`, `home`, `end`, `pageup`/`pagedown`, `f1`–`f12`, and any single printable character (resolved against the active keyboard layout, so `/`, `?`, etc. work).

Input goes through `SendInput` (the real Windows input pipeline), so winit sees correct key text and modifier state. `/input` first brings the lopress window to the foreground; if it cannot, it returns `400` rather than injecting keystrokes into another app.

## Screenshot workflow

```powershell
Invoke-WebRequest http://127.0.0.1:7878/screenshot -OutFile shot.png
```
Then `Read` the PNG to view it. Capturing briefly raises the window to topmost (~100 ms) to force DWM to composite the wgpu surface — expect a momentary z-order flicker.

## Standard debugging loop

1. `cargo run` in background → `/ping` until `ok`.
2. `/state` → record current block ids (they change after every structural action).
3. Apply a change: `/action` for doc edits, `/input`/`/click` for input-path testing.
4. Confirm the effect: `/state` for `/action` changes (**re-fetch, never reuse stale ids**); `/screenshot` for `/input`-typed text (see below).

## `/input` vs `/state` — committed vs buffered edits

`/input`-typed text lands in the **inline editor's live buffer** and is visible immediately in `/screenshot`, but it does **not** appear in `/state` until the edit is committed to the document model (the editor commits on structural actions; a plain blur may not). So:

- After `/input` text/keys, verify with `/screenshot`, not `/state`.
- Use `/action` when you need a change reflected in `/state` and persisted to disk.

## Troubleshooting

| Symptom | Likely cause | How to check / fix |
|---|---|---|
| `/ping` → `ok`, but `/action` → `504` | Floem event loop stalled — window hidden, minimized, or not foreground | Check: `powershell "Get-Process lopress \| Select MainWindowTitle"`. If `MainWindowTitle` is empty the window isn't visible. Fix: `Start-Process` with no `-WindowStyle` flag; never `Hidden`. |
| `/ping` → error or hangs | Process not running or still compiling | Run `cargo run` from repo root as a persistent background process. Poll `/ping` until `ok`. |
| `/open` → `not_found` | No `lopress.toml` in any ancestor of the requested path | Create `lopress.toml` at the workspace root. Use an absolute path for the first `/open`. |
| `/open` → `no_workspace` | No workspace open yet and path is relative | Use an absolute path for the first `/open`; it bootstraps the workspace. |
| `/state` shows `doc_open: false` | No document is open | Call `/open` with an absolute path first. |
| `/state` block ids change after action | `Split`/`Delete`/`Merge`/`Move` reshape the block tree | Always re-`/state` after a structural action; never reuse stale ids. |
| `/state` missing text after `/input` typing | Edits are in the inline buffer, not yet committed | Verify with `/screenshot` after `/input`; use `/action` for changes that persist to `/state`. |

## Diagnostic checklist

When an endpoint behaves unexpectedly, run these in order:

1. **Process running?** `tasklist \| findstr lopress` — should show `lopress.exe` with a PID.
2. **Window visible?** `powershell "Get-Process lopress \| Select MainWindowTitle"` — should show `MainWindowTitle: lopress`. Empty title = hidden window = event loop stall.
3. **Port bound?** `netstat -an \| findstr 7878` — should show `LISTENING` on `127.0.0.1:7878`.
4. **Endpoint responses?** Probe in order: `/ping` → `/state` → `/click` → `/open` → `/action`. The first endpoint that fails tells you where the chain breaks.
5. **Report** all five results when asking for help.

## Common mistakes

- **Hidden / minimized window → `504 "editor did not respond"`** on `/open`, `/action`, `/close` while `/ping` still works. The floem event loop that services those channel-backed endpoints stalls when the window isn't visible. Launch a visible, non-minimized window; never `Start-Process -WindowStyle Hidden`.
- **`cargo run -p lopress-editor`** — that crate is a library (`error: a bin target must be available`). Run the root `lopress` binary: plain `cargo run` from the repo root.
- **Running `--release`** — the server is `#[cfg(debug_assertions)]`; nothing binds. Use `cargo run`.
- **Probing too early / killing the editor** — `cargo run & sleep 5; curl` fires during the multi-minute cold build and then ends, taking the editor down. Run it as a persistent background process and poll `/ping` until `ok` from separate calls.
- **Starting with a relative path** — the first `/open` must be absolute (it bootstraps the workspace); a relative path before any workspace is open → `409 no_workspace`.
- **Reusing block ids** after a `Split`/`Delete`/`Merge`/`Move` — always re-`/state` first.
- **Acting before a doc is open** — `/action` now returns `409 {"status":"no_document"}` when `doc_open` is false (no longer a silent no-op).
- **Checking `/state` after `/input` typing** — buffered edits show in `/screenshot`, not `/state`, until committed.
- **Typing into an unfocused block** — `/click` the target block first; `/input` types wherever the caret is.
- **Forgetting the editor blocks the foreground** — `/screenshot`, `/click`, and `/input` briefly reorder/activate the window; that flicker is expected.

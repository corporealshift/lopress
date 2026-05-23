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

**Not for:** release builds (the server is compiled out — use `cargo run`, never `--release`), headless CI, or any non-Windows host for `/screenshot`, `/input`, `/click` (those return errors off Windows).

## Prerequisites

1. Start a **debug build** from the repo root: `cargo run` (binary is `lopress`, calls `lopress_editor::run()`). Run it in the background so it keeps listening.
2. Confirm the server is up: `GET /ping` returns `ok`.
3. The window title must be `lopress` — `/screenshot`, `/input`, `/click` find it by that exact title.
4. Open a document in the editor UI. Until then `/state` reports `{"doc_open":false,...}` and `/action` is a no-op.

## Quick reference

All endpoints on `http://127.0.0.1:7878`. On Windows use `Invoke-RestMethod` / `Invoke-WebRequest`.

| Endpoint | Method | Purpose | Returns |
|---|---|---|---|
| `/ping` | GET | Liveness check | `ok` |
| `/state` | GET | Current doc as JSON | `{doc_open, path, blocks:[{id,kind,text,lang?}]}` |
| `/screenshot` | GET | PNG of the window | `image/png` bytes; `503` if window not found |
| `/action` | POST | Apply a `CtrlAction` to the doc | `200 {"status":"dispatched"}` / `409 no_document` / `422 block_not_found` / `400 parse error` |
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

## Common mistakes

- **Running `--release`** — the server is `#[cfg(debug_assertions)]`; nothing binds. Use `cargo run`.
- **Reusing block ids** after a `Split`/`Delete`/`Merge`/`Move` — always re-`/state` first.
- **Acting before a doc is open** — `/action` now returns `409 {"status":"no_document"}` when `doc_open` is false (no longer a silent no-op).
- **Checking `/state` after `/input` typing** — buffered edits show in `/screenshot`, not `/state`, until committed.
- **Typing into an unfocused block** — `/click` the target block first; `/input` types wherever the caret is.
- **Forgetting the editor blocks the foreground** — `/screenshot`, `/click`, and `/input` briefly reorder/activate the window; that flicker is expected.

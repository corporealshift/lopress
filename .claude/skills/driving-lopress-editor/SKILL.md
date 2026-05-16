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
| `/action` | POST | Apply a `CtrlAction` to the doc | `ok` / `400 parse error: …` |
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

`new_kind` types: `Paragraph`, `Heading {level}`, `Code {lang}`, `List {ordered}`. An unknown `block_id` is silently dropped — the action just doesn't apply.

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

Modifiers: `ctrl`, `shift`, `alt`. Keys: `a`–`z`, `0`–`9`, `enter`, `backspace`, `delete`, `tab`, `escape`, `up`/`down`/`left`/`right`, `home`, `end`, `f1`–`f12`.

## Screenshot workflow

```powershell
Invoke-WebRequest http://127.0.0.1:7878/screenshot -OutFile shot.png
```
Then `Read` the PNG to view it. Capturing briefly raises the window to topmost (~100 ms) to force DWM to composite the wgpu surface — expect a momentary z-order flicker.

## Standard debugging loop

1. `cargo run` in background → `/ping` until `ok`.
2. `/state` → record current block ids (they change after every structural action).
3. Apply a change: `/action` for doc edits, `/input`/`/click` for input-path testing.
4. `/state` again to confirm the effect — **re-fetch, never reuse stale ids**.
5. `/screenshot` for visual confirmation.

## Known limitations

The control harness has gaps (documented in `docs/superpowers/plans/2026-05-16-control-verification-findings.md`). Treat input-injection failures as *harness* limits, not editor bugs, until verified by hand:

| Limitation | Impact |
|---|---|
| `/input` text/keys go via `PostMessage` (`WM_CHAR`/`WM_KEYDOWN`); winit may not register them — especially modified chords | Free-text typing and `ctrl+…` shortcuts may silently no-op |
| `parse_key` has no `pageup`/`pagedown` | Cannot test Page Up/Down via `/keys` |
| `parse_key` maps any 1-char string to the uppercased char's VK | Non-letter keys like `/` are wrong |

When `/input` fails to take effect, fall back to `/action` (reliable) or note the feature needs manual testing. Do **not** report an editor bug based solely on a non-responsive `/input` call.

## Common mistakes

- **Running `--release`** — the server is `#[cfg(debug_assertions)]`; nothing binds. Use `cargo run`.
- **Reusing block ids** after a `Split`/`Delete`/`Merge`/`Move` — always re-`/state` first.
- **Acting before a doc is open** — `/action` is a silent no-op when `doc_open` is false.
- **Concluding a bug from a dead `/input`** — verify against the harness limitations above first.
- **Forgetting the editor blocks the foreground** — `/screenshot` and `/click` briefly reorder windows; that flicker is expected.

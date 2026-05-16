# Claude Editor Control — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed a debug-only HTTP server in the Floem editor (port 7878) that lets Claude see, control, and inject input into the running editor process.

**Architecture:** A `ctrl` module (3 files, all `#[cfg(debug_assertions)]`) starts a `tiny_http` server on a background thread. Doc state is mirrored to an `Arc<Mutex<String>>` JSON snapshot via a Floem `create_effect`. Incoming `BlockAction` commands travel through a `crossbeam` channel into a `create_signal_from_channel`-backed reactive signal, whose effect applies them to `current_doc`. Screenshots use Windows GDI; input injection uses Windows `SendInput`.

**Tech Stack:** Rust, Floem 0.2, tiny_http, image crate, windows crate (GDI + SendInput), crossbeam-channel, serde_json

---

## File Map

| File | Role |
|------|------|
| `crates/lopress-editor/Cargo.toml` | Add tiny_http, image; add windows features |
| `crates/lopress-editor/src/model/types.rs` | Add `Serialize, Deserialize` to `InlineRun` |
| `crates/lopress-editor/src/ctrl/mod.rs` | HTTP server thread, `CtrlAction`/`CtrlBlockKind` types, `serialize_state`, screenshot |
| `crates/lopress-editor/src/ctrl/input.rs` | Key string parsing → `SendInput`, `SetForegroundWindow` |
| `crates/lopress-editor/src/lib.rs` | Create ctrl channels, spawn HTTP thread (debug only), pass receiver to `root_view` |
| `crates/lopress-editor/src/ui/mod.rs` | `root_view` + `editing_view` gain a `#[cfg(debug_assertions)]` ctrl param; wire snapshot effect and action signal |

---

## Task 1: Add Cargo deps

**Files:**
- Modify: `crates/lopress-editor/Cargo.toml`

- [ ] **Step 1: Find the windows crate version already in the dep tree**

Run: `cargo tree -p lopress-editor | grep "^windows v"` (or search Cargo.lock).
The version is almost certainly `0.58.x` (Floem's transitive dep). Note it.

- [ ] **Step 2: Add deps to Cargo.toml**

In `crates/lopress-editor/Cargo.toml`, add to `[dependencies]`:

```toml
# Dev control server (used in debug builds only; compiled unconditionally, activated via cfg)
tiny_http = "0.12"
image = { version = "0.25", default-features = false, features = ["png"] }

[target.'cfg(windows)'.dependencies.windows]
version = "0.58"
features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
]
```

> If the windows crate version in Cargo.lock differs from 0.58, use that exact version.

- [ ] **Step 3: Verify**

Run: `cargo check -p lopress-editor`
Expected: compiles cleanly (no new errors from just adding deps).

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/Cargo.toml
git commit -m "chore(ctrl): add tiny_http, image, windows deps for debug control server"
```

---

## Task 2: Serde on InlineRun

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs`

- [ ] **Step 1: Add derives**

In `crates/lopress-editor/src/model/types.rs`, change the `InlineRun` derive line from:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlineRun {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct InlineRun {
```

No other changes to types.rs. `serde` is already in Cargo.toml (workspace dep).

- [ ] **Step 2: Verify**

Run: `cargo check -p lopress-editor`
Expected: no errors.

- [ ] **Step 3: Commit**

```
git add crates/lopress-editor/src/model/types.rs
git commit -m "feat(ctrl): derive Serialize/Deserialize on InlineRun"
```

---

## Task 3: Create ctrl module skeleton

**Files:**
- Create: `crates/lopress-editor/src/ctrl/mod.rs`
- Create: `crates/lopress-editor/src/ctrl/input.rs` (empty stub)
- Modify: `crates/lopress-editor/src/lib.rs` (add `mod ctrl`)

- [ ] **Step 1: Create ctrl/input.rs stub**

Create `crates/lopress-editor/src/ctrl/input.rs` with:

```rust
// Populated in Task 5.
```

- [ ] **Step 2: Create ctrl/mod.rs with types and serialize_state**

Create `crates/lopress-editor/src/ctrl/mod.rs`:

```rust
#![cfg(debug_assertions)]

pub mod input;

use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use serde::Deserialize;

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, BlockKind, EditorDoc, InlineRun, ListItem};

// ── Public handle passed to ui::root_view ────────────────────────────────────

pub struct CtrlHandle {
    pub snapshot: Arc<Mutex<String>>,
    pub action_tx: Sender<CtrlAction>,
}

// ── Action types (HTTP API surface) ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum CtrlAction {
    Split { block_id: u64, byte_offset: usize },
    MergeWithPrev { block_id: u64 },
    Delete { block_id: u64 },
    Move { block_id: u64, to_index: usize },
    ChangeType { block_id: u64, new_kind: CtrlBlockKind },
    EditInline { block_id: u64, new_runs: Vec<InlineRun> },
    EditCode { block_id: u64, new_text: String },
    EditAttrs { block_id: u64, new_attrs: serde_json::Map<String, serde_json::Value> },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum CtrlBlockKind {
    Paragraph,
    Heading { level: u8 },
    Code { lang: String },
    List { ordered: bool },
}

impl CtrlAction {
    /// Convert to a `BlockAction` by resolving the raw block id against the live doc.
    /// Returns `None` if the block id is not found in the current doc.
    pub fn into_block_action(self, doc: &EditorDoc) -> Option<BlockAction> {
        fn find(doc: &EditorDoc, raw: u64) -> Option<BlockId> {
            doc.blocks.iter().find(|b| b.id.raw() == raw).map(|b| b.id)
        }
        Some(match self {
            CtrlAction::Split { block_id, byte_offset } =>
                BlockAction::Split { block_id: find(doc, block_id)?, byte_offset },
            CtrlAction::MergeWithPrev { block_id } =>
                BlockAction::MergeWithPrev { block_id: find(doc, block_id)? },
            CtrlAction::Delete { block_id } =>
                BlockAction::Delete { block_id: find(doc, block_id)? },
            CtrlAction::Move { block_id, to_index } =>
                BlockAction::Move { block_id: find(doc, block_id)?, to_index },
            CtrlAction::ChangeType { block_id, new_kind } => BlockAction::ChangeType {
                block_id: find(doc, block_id)?,
                new_kind: match new_kind {
                    CtrlBlockKind::Paragraph => BlockKind::Paragraph,
                    CtrlBlockKind::Heading { level } => BlockKind::Heading(level.clamp(1, 6)),
                    CtrlBlockKind::Code { lang } => BlockKind::Code { lang },
                    CtrlBlockKind::List { ordered } => BlockKind::List { ordered },
                },
            },
            CtrlAction::EditInline { block_id, new_runs } =>
                BlockAction::EditInline { block_id: find(doc, block_id)?, new_runs },
            CtrlAction::EditCode { block_id, new_text } =>
                BlockAction::EditCode { block_id: find(doc, block_id)?, new_text },
            CtrlAction::EditAttrs { block_id, new_attrs } =>
                BlockAction::EditAttrs { block_id: find(doc, block_id)?, new_attrs },
        })
    }
}

// ── Doc state serialization ───────────────────────────────────────────────────

/// Serialize the current doc (and open path) to a compact JSON string for GET /state.
pub fn serialize_state(doc: Option<&EditorDoc>, path: Option<&std::path::Path>) -> String {
    if let Some(doc) = doc {
        let blocks: Vec<serde_json::Value> = doc.blocks.iter().map(|b| {
            let id = b.id.raw();
            match &b.body {
                BlockBody::Inline(runs) => {
                    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                    let kind_str = match &b.kind {
                        BlockKind::Paragraph => "Paragraph".to_string(),
                        BlockKind::Heading(n) => format!("Heading{n}"),
                        BlockKind::Code { .. } => "Code".to_string(),
                        BlockKind::List { .. } => "List".to_string(),
                        BlockKind::Opaque { type_name } => format!("Opaque({type_name})"),
                    };
                    serde_json::json!({ "id": id, "kind": kind_str, "text": text })
                }
                BlockBody::Code(text) => {
                    let lang = match &b.kind {
                        BlockKind::Code { lang } => lang.clone(),
                        _ => String::new(),
                    };
                    serde_json::json!({ "id": id, "kind": "Code", "lang": lang, "text": text })
                }
                BlockBody::List(items) => {
                    let text = items.iter()
                        .map(|item| item.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                        .collect::<Vec<_>>()
                        .join("\n");
                    serde_json::json!({ "id": id, "kind": "List", "text": text })
                }
                BlockBody::Opaque(_) => {
                    let type_name = match &b.kind {
                        BlockKind::Opaque { type_name } => type_name.clone(),
                        _ => String::new(),
                    };
                    serde_json::json!({ "id": id, "kind": format!("Opaque({type_name})"), "text": "" })
                }
            }
        }).collect();

        serde_json::json!({
            "doc_open": true,
            "path": path.map(|p| p.to_string_lossy().to_string()),
            "blocks": blocks
        }).to_string()
    } else {
        r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string()
    }
}

// ── HTTP server ───────────────────────────────────────────────────────────────

/// Initialise the ctrl subsystem. Returns a `CtrlHandle` for `root_view` wiring
/// and starts the HTTP server on a background thread.
pub fn start() -> (CtrlHandle, crossbeam_channel::Receiver<CtrlAction>) {
    let snapshot: Arc<Mutex<String>> =
        Arc::new(Mutex::new(r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string()));
    let (action_tx, action_rx) = crossbeam_channel::unbounded::<CtrlAction>();

    let handle = CtrlHandle {
        snapshot: Arc::clone(&snapshot),
        action_tx: action_tx.clone(),
    };

    let server_snapshot = Arc::clone(&snapshot);
    let server_tx = action_tx;

    std::thread::spawn(move || {
        serve(server_snapshot, server_tx);
    });

    (handle, action_rx)
}

fn serve(snapshot: Arc<Mutex<String>>, action_tx: Sender<CtrlAction>) {
    let server = match tiny_http::Server::http("127.0.0.1:7878") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[ctrl] failed to bind 127.0.0.1:7878: {e}");
            return;
        }
    };
    eprintln!("[ctrl] listening on http://127.0.0.1:7878");

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();

        let response = match (method.as_str(), url.as_str()) {
            ("GET", "/ping") => {
                tiny_http::Response::from_string("ok")
            }

            ("GET", "/state") => {
                let body = snapshot.lock().unwrap().clone();
                tiny_http::Response::from_string(body)
                    .with_header(
                        "Content-Type: application/json".parse::<tiny_http::Header>().unwrap()
                    )
            }

            ("GET", "/screenshot") => {
                match take_screenshot() {
                    Ok(png_bytes) => {
                        tiny_http::Response::from_data(png_bytes)
                            .with_header(
                                "Content-Type: image/png".parse::<tiny_http::Header>().unwrap()
                            )
                    }
                    Err(e) => {
                        tiny_http::Response::from_string(e)
                            .with_status_code(503)
                    }
                }
            }

            ("POST", "/action") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    tiny_http::Response::from_string("read error").with_status_code(400)
                } else {
                    match serde_json::from_str::<CtrlAction>(&body) {
                        Ok(action) => {
                            let _ = action_tx.send(action);
                            tiny_http::Response::from_string("ok")
                        }
                        Err(e) => {
                            tiny_http::Response::from_string(format!("parse error: {e}"))
                                .with_status_code(400)
                        }
                    }
                }
            }

            ("POST", "/input") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    tiny_http::Response::from_string("read error").with_status_code(400)
                } else {
                    match input::handle_input(&body) {
                        Ok(()) => tiny_http::Response::from_string("ok"),
                        Err(e) => tiny_http::Response::from_string(e).with_status_code(400),
                    }
                }
            }

            ("POST", "/click") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    tiny_http::Response::from_string("read error").with_status_code(400)
                } else {
                    match input::handle_click(&body) {
                        Ok(()) => tiny_http::Response::from_string("ok"),
                        Err(e) => tiny_http::Response::from_string(e).with_status_code(400),
                    }
                }
            }

            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };

        let _ = request.respond(response);
    }
}

// ── Screenshot ────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn take_screenshot() -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetClientRect};
    use windows::core::PCWSTR;

    unsafe {
        // Find the editor window by title.
        let title: Vec<u16> = "lopress\0".encode_utf16().collect();
        let hwnd: HWND = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
        if hwnd.0 == 0 {
            return Err("window not found".to_string());
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let width = (rect.right - rect.left) as i32;
        let height = (rect.bottom - rect.top) as i32;
        if width <= 0 || height <= 0 {
            return Err("window has zero size".to_string());
        }

        let hdc_src = GetDC(hwnd);
        let hdc_dst = CreateCompatibleDC(hdc_src);
        let hbm = CreateCompatibleBitmap(hdc_src, width, height);
        let old = SelectObject(hdc_dst, hbm);

        BitBlt(hdc_dst, 0, 0, width, height, hdc_src, 0, 0, SRCCOPY)
            .map_err(|e| e.to_string())?;

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let pixel_count = (width * height) as usize;
        let mut pixels: Vec<u8> = vec![0u8; pixel_count * 4];
        GetDIBits(
            hdc_dst,
            hbm,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_dst, old);
        DeleteObject(hbm);
        DeleteDC(hdc_dst);
        ReleaseDC(hwnd, hdc_src);

        // Convert BGRA → RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        // Encode to PNG
        let mut png_bytes: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            &pixels,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        ).map_err(|e| e.to_string())?;

        Ok(png_bytes)
    }
}

#[cfg(not(target_os = "windows"))]
fn take_screenshot() -> Result<Vec<u8>, String> {
    Err("screenshots only supported on Windows".to_string())
}
```

- [ ] **Step 3: Add `mod ctrl` to lib.rs**

In `crates/lopress-editor/src/lib.rs`, after the existing `pub mod` declarations, add:

```rust
#[cfg(debug_assertions)]
pub(crate) mod ctrl;
```

- [ ] **Step 4: Verify**

Run: `cargo check -p lopress-editor`
Expected: compiles cleanly. (The `input` module stubs are empty, that's fine.)

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ctrl/
git add crates/lopress-editor/src/lib.rs
git commit -m "feat(ctrl): ctrl module skeleton — CtrlAction types, serialize_state, HTTP server stub, screenshot"
```

---

## Task 4: ctrl/input.rs — SendInput and key parsing

**Files:**
- Modify: `crates/lopress-editor/src/ctrl/input.rs`

- [ ] **Step 1: Write ctrl/input.rs**

Replace the stub with:

```rust
#![cfg(debug_assertions)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InputCmd {
    Text { text: String },
    Keys { keys: String },
}

#[derive(Debug, Deserialize)]
pub struct ClickCmd {
    pub x: i32,
    pub y: i32,
}

pub fn handle_input(body: &str) -> Result<(), String> {
    let cmd: InputCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    bring_editor_to_front()?;
    match cmd {
        InputCmd::Text { text } => send_text(&text),
        InputCmd::Keys { keys } => send_keys(&keys),
    }
}

pub fn handle_click(body: &str) -> Result<(), String> {
    let cmd: ClickCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    bring_editor_to_front()?;
    send_click(cmd.x, cmd.y)
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, mouse_event, SendInput, MapVirtualKeyW, INPUT, INPUT_0,
        INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MOVE, MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END,
        VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6,
        VK_F7, VK_F8, VK_F9, VK_HOME, VK_LEFT, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
        VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        ClientToScreen, FindWindowW, GetClientRect, GetForegroundWindow, SetForegroundWindow,
    };
    use windows::core::PCWSTR;

    pub fn bring_to_front() -> Result<HWND, String> {
        unsafe {
            let title: Vec<u16> = "lopress\0".encode_utf16().collect();
            let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
            if hwnd.0 == 0 {
                return Err("window not found".to_string());
            }
            if GetForegroundWindow() != hwnd {
                let _ = SetForegroundWindow(hwnd);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(hwnd)
        }
    }

    pub fn send_text(text: &str) -> Result<(), String> {
        let mut inputs: Vec<INPUT> = Vec::new();
        for ch in text.encode_utf16() {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: KEYEVENTF_UNICODE,
                        ..Default::default()
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            });
        }
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    pub fn send_keys(keys: &str) -> Result<(), String> {
        let parts: Vec<&str> = keys.split('+').collect();
        let (modifiers, key_str) = parts.split_at(parts.len().saturating_sub(1));
        let key_str = key_str.first().copied().unwrap_or("");

        let mut mods: Vec<VIRTUAL_KEY> = Vec::new();
        for m in modifiers {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => mods.push(VK_LCONTROL),
                "shift" => mods.push(VK_LSHIFT),
                "alt" => mods.push(VK_LMENU),
                other => return Err(format!("unknown modifier: {other}")),
            }
        }

        let vk = parse_key(key_str)?;

        let mut inputs: Vec<INPUT> = Vec::new();

        // Press modifiers
        for &m in &mods {
            inputs.push(make_key_input(m, false));
        }
        // Press + release main key
        inputs.push(make_key_input(vk, false));
        inputs.push(make_key_input(vk, true));
        // Release modifiers (reverse order)
        for &m in mods.iter().rev() {
            inputs.push(make_key_input(m, true));
        }

        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn make_key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) as u16 };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                    ..Default::default()
                },
            },
        }
    }

    fn parse_key(s: &str) -> Result<VIRTUAL_KEY, String> {
        match s.to_lowercase().as_str() {
            "enter" | "return" => Ok(VK_RETURN),
            "backspace" => Ok(VK_BACK),
            "delete" => Ok(VK_DELETE),
            "tab" => Ok(VK_TAB),
            "escape" | "esc" => Ok(VK_ESCAPE),
            "up" => Ok(VK_UP),
            "down" => Ok(VK_DOWN),
            "left" => Ok(VK_LEFT),
            "right" => Ok(VK_RIGHT),
            "home" => Ok(VK_HOME),
            "end" => Ok(VK_END),
            "f1" => Ok(VK_F1),
            "f2" => Ok(VK_F2),
            "f3" => Ok(VK_F3),
            "f4" => Ok(VK_F4),
            "f5" => Ok(VK_F5),
            "f6" => Ok(VK_F6),
            "f7" => Ok(VK_F7),
            "f8" => Ok(VK_F8),
            "f9" => Ok(VK_F9),
            "f10" => Ok(VK_F10),
            "f11" => Ok(VK_F11),
            "f12" => Ok(VK_F12),
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap().to_ascii_uppercase() as u16;
                Ok(VIRTUAL_KEY(ch))
            }
            other => Err(format!("unknown key: {other}")),
        }
    }

    pub fn send_click(x: i32, y: i32, hwnd: HWND) -> Result<(), String> {
        unsafe {
            let mut pt = POINT { x, y };
            ClientToScreen(hwnd, &mut pt);

            // Get screen dimensions for absolute coordinates (0..65535 range)
            use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
            let sw = GetSystemMetrics(SM_CXSCREEN);
            let sh = GetSystemMetrics(SM_CYSCREEN);
            let ax = (pt.x as i64 * 65535 / sw as i64) as i32;
            let ay = (pt.y as i64 * 65535 / sh as i64) as i32;

            let inputs = [
                INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE | MOUSEEVENTF_LEFTDOWN,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE | MOUSEEVENTF_LEFTUP,
                            ..Default::default()
                        },
                    },
                },
            ];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }
}

// ── Platform dispatch ─────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn bring_editor_to_front() -> Result<(), String> {
    win::bring_to_front().map(|_| ())
}

#[cfg(not(target_os = "windows"))]
pub fn bring_editor_to_front() -> Result<(), String> {
    Err("input injection only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn send_text(text: &str) -> Result<(), String> {
    win::send_text(text)
}

#[cfg(not(target_os = "windows"))]
fn send_text(_text: &str) -> Result<(), String> {
    Err("input injection only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn send_keys(keys: &str) -> Result<(), String> {
    win::send_keys(keys)
}

#[cfg(not(target_os = "windows"))]
fn send_keys(_keys: &str) -> Result<(), String> {
    Err("input injection only supported on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn send_click(x: i32, y: i32) -> Result<(), String> {
    let hwnd = win::bring_to_front()?;
    win::send_click(x, y, hwnd)
}

#[cfg(not(target_os = "windows"))]
fn send_click(_x: i32, _y: i32) -> Result<(), String> {
    Err("input injection only supported on Windows".to_string())
}
```

- [ ] **Step 2: Verify**

Run: `cargo check -p lopress-editor`
Expected: no errors. (Windows-only code only compiles on Windows, which this is.)

> **Note:** The `windows` crate API for `INPUT`/`KEYBDINPUT`/`MOUSEINPUT` uses `Anonymous` unions. If the compiler complains about field names, check the exact field names in the version of the `windows` crate present in Cargo.lock. The version 0.58 API uses `Anonymous: INPUT_0 { ki: KEYBDINPUT { ... } }`. Adjust if needed.

- [ ] **Step 3: Commit**

```
git add crates/lopress-editor/src/ctrl/input.rs
git commit -m "feat(ctrl): Windows SendInput key/mouse injection"
```

---

## Task 5: Wire ctrl into lib.rs

**Files:**
- Modify: `crates/lopress-editor/src/lib.rs`

The goal: call `ctrl::start()` before `Application::new()`, then pass the `action_rx` into `root_view`.

- [ ] **Step 1: Read the current lib.rs**

Read `crates/lopress-editor/src/lib.rs` to see the exact current state before editing.

- [ ] **Step 2: Update lib.rs**

The current `run()` function calls `ui::root_view(ctx, settings_signal)` inside the window closure. We need to:
1. Call `ctrl::start()` before `Application::new()`
2. Pass `action_rx` and the snapshot arc into `root_view`

Replace the relevant section of `lib.rs`. The full updated `run()` function:

```rust
pub fn run() -> Result<(), AppError> {
    // ── Load settings ──────────────────────────────────────────────────────
    let settings = match (settings::default_path(), settings::legacy_recents_path()) {
        (Some(ref s), Some(ref r)) => Settings::load_or_migrate(s, r).unwrap_or_default(),
        (Some(ref s), None) => Settings::load_from(s).unwrap_or_default(),
        _ => Settings::default(),
    };

    // ── Build window config from saved geometry ────────────────────────────
    let ws = &settings.window;
    let win_cfg = WindowConfig::default()
        .title("lopress")
        .size(Size::new(ws.width, ws.height))
        .position(Point::new(ws.x, ws.y));

    let ctx = AppContext::new(settings);

    let settings_signal: RwSignal<Settings> = RwSignal::new(Settings::default());
    let settings_for_close = settings_signal;

    // ── Debug control server ───────────────────────────────────────────────
    #[cfg(debug_assertions)]
    let (ctrl_handle, ctrl_action_rx) = ctrl::start();

    Application::new()
        .on_event(move |event| {
            if let floem::AppEvent::WillTerminate = event {
                if let Some(path) = settings::default_path() {
                    settings_for_close.with(|s| {
                        s.save_to(&path).ok();
                    });
                }
            }
        })
        .window(
            move |window_id| {
                let view = ui::root_view(
                    ctx,
                    settings_signal,
                    #[cfg(debug_assertions)]
                    ctrl_handle,
                    #[cfg(debug_assertions)]
                    ctrl_action_rx,
                );

                view.on_event_stop(EventListener::WindowClosed, move |_e: &Event| {
                    let size = window_id
                        .bounds_of_content_on_screen()
                        .map(|r| (r.width(), r.height()));
                    let pos = window_id.position_on_screen_including_frame();

                    settings_signal.update(|s| {
                        if let Some((w, h)) = size {
                            s.window.width = w;
                            s.window.height = h;
                        }
                        if let Some(p) = pos {
                            s.window.x = p.x;
                            s.window.y = p.y;
                        }
                    });

                    if let Some(path) = settings::default_path() {
                        settings_signal.with(|s| {
                            s.save_to(&path).ok();
                        });
                    }
                })
            },
            Some(win_cfg),
        )
        .run();
    Ok(())
}
```

Note: passing `#[cfg(...)]` attributes on individual arguments in a function call is valid Rust syntax.

- [ ] **Step 3: Verify (expect errors about root_view signature — that's fine)**

Run: `cargo check -p lopress-editor 2>&1 | head -30`
Expected: error about `root_view` having wrong number of arguments. That's expected — we fix it next task.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/lib.rs
git commit -m "feat(ctrl): wire ctrl::start() into lib.rs run()"
```

---

## Task 6: Wire ctrl into ui/mod.rs

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs`

This is the largest change. We update `root_view` to accept the ctrl params, thread them into `editing_view`, and set up two reactive effects there.

- [ ] **Step 1: Read the current ui/mod.rs**

Read `crates/lopress-editor/src/ui/mod.rs` in full to see current state before editing.

- [ ] **Step 2: Update root_view signature**

Change the `root_view` function signature from:

```rust
pub fn root_view(ctx: AppContext, settings_signal: RwSignal<Settings>) -> impl IntoView {
```

to:

```rust
pub fn root_view(
    ctx: AppContext,
    settings_signal: RwSignal<Settings>,
    #[cfg(debug_assertions)] ctrl_handle: crate::ctrl::CtrlHandle,
    #[cfg(debug_assertions)] ctrl_action_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
) -> impl IntoView {
    // ctrl_handle and ctrl_action_rx are wrapped into ctrl_once (Rc<RefCell<Option<...>>>)
    // before the dyn_container closure — see Step 3.
```

- [ ] **Step 3: Thread ctrl params into editing_view call**

The `dyn_container` second argument is a `Fn` closure (may be called more than once by the reactive system), so we can't move `ctrl_handle`/`ctrl_action_rx` directly — they must be wrapped like `editing` is wrapped in `Rc<RefCell<Option<...>>>`.

After the `let editing_for_view = Rc::clone(&editing);` line, add:

```rust
    #[cfg(debug_assertions)]
    let ctrl_once: std::rc::Rc<std::cell::RefCell<Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
    )>>> = std::rc::Rc::new(std::cell::RefCell::new(Some((ctrl_handle, ctrl_action_rx))));
    #[cfg(debug_assertions)]
    let ctrl_once_for_view = std::rc::Rc::clone(&ctrl_once);
```

Then, inside the `dyn_container` closure, change the `StateTag::Editing` arm from:

```rust
StateTag::Editing => editing_view(Rc::clone(&editing_for_view), current_doc).into_any(),
```

to:

```rust
StateTag::Editing => {
    #[cfg(debug_assertions)]
    let ctrl = ctrl_once_for_view.borrow_mut().take();
    editing_view(
        Rc::clone(&editing_for_view),
        current_doc,
        #[cfg(debug_assertions)] ctrl,
    ).into_any()
}
```

- [ ] **Step 4: Update editing_view signature**

Change `editing_view` from:

```rust
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> impl IntoView {
```

to:

```rust
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    #[cfg(debug_assertions)] ctrl: Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
    )>,
) -> impl IntoView {
```

- [ ] **Step 5: Add ctrl wiring inside editing_view**

At the END of `editing_view`, just before the final `stack((columns, footer))` line, insert:

```rust
    // ── Debug ctrl wiring ────────────────────────────────────────────────────
    #[cfg(debug_assertions)]
    if let Some((ctrl_handle, ctrl_action_rx)) = ctrl {
        use floem::ext_event::create_signal_from_channel;
        use floem::reactive::{create_effect, SignalGet, SignalWith, SignalGetUntracked};

        // Snapshot: keep Arc<Mutex<String>> in sync with current_doc + current_path.
        let snap = ctrl_handle.snapshot.clone();
        create_effect(move |_| {
            let json = current_doc.with(|maybe| {
                crate::ctrl::serialize_state(
                    maybe.as_ref(),
                    current_path.get_untracked().as_deref(),
                )
            });
            *snap.lock().unwrap() = json;
        });

        // Actions: crossbeam channel → reactive signal → apply to doc.
        let action_read = create_signal_from_channel(ctrl_action_rx);
        create_effect(move |_| {
            if let Some(action) = action_read.get() {
                current_doc.update(|maybe| {
                    if let Some(doc) = maybe {
                        if let Some(block_action) = action.into_block_action(doc) {
                            crate::actions::apply(doc, block_action);
                        }
                    }
                });
            }
        });
    }
```

- [ ] **Step 6: Add crossbeam import**

At the top of `ui/mod.rs`, add (outside any cfg block):

```rust
#[cfg(debug_assertions)]
use crossbeam_channel;
```

Actually, `crossbeam_channel` doesn't need to be explicitly imported at the module level — it's accessed via its crate path in the type signature. The `crossbeam_channel::Receiver<...>` in the function signature will work as long as `crossbeam-channel` is in Cargo.toml (which it is, as a transitive dep — but we may need to add it explicitly). Check: if `cargo check` errors about `crossbeam_channel` not found, add to `crates/lopress-editor/Cargo.toml`:

```toml
crossbeam-channel = "0.5"
```

- [ ] **Step 7: Verify**

Run: `cargo check -p lopress-editor`
Expected: no errors.

Run: `cargo check -p lopress-editor --release`
Expected: no errors (ctrl code absent in release, no missing symbol errors).

- [ ] **Step 8: Commit**

```
git add crates/lopress-editor/src/ui/mod.rs
git commit -m "feat(ctrl): wire ctrl snapshot + action channel into editing_view"
```

---

## Task 7: Build and smoke test

**Files:** none (verification only)

- [ ] **Step 1: Full debug build**

Run: `cargo build -p lopress-editor`
Expected: builds cleanly. Note any warnings about unused imports and fix them.

- [ ] **Step 2: Release build check**

Run: `cargo build -p lopress-editor --release`
Expected: builds cleanly. The ctrl module must be absent (no tiny_http or windows GDI symbols in release).

- [ ] **Step 3: Run the editor**

Run: `cargo run`
Expected: editor window opens. In the terminal, you should see:
```
[ctrl] listening on http://127.0.0.1:7878
```

- [ ] **Step 4: Ping**

```powershell
Invoke-RestMethod http://localhost:7878/ping
```
Expected: `ok`

- [ ] **Step 5: State (no doc open)**

```powershell
Invoke-RestMethod http://localhost:7878/state | ConvertTo-Json
```
Expected: `{ "doc_open": false, "path": null, "blocks": [] }`

- [ ] **Step 6: Open a doc, then check state**

Open a document in the editor sidebar. Then:
```powershell
Invoke-RestMethod http://localhost:7878/state | ConvertTo-Json -Depth 5
```
Expected: `doc_open: true` with blocks matching the document content.

- [ ] **Step 7: Screenshot**

```powershell
$bytes = Invoke-RestMethod http://localhost:7878/screenshot
[System.IO.File]::WriteAllBytes("C:\Users\corpo\Desktop\editor-shot.png", $bytes)
```
Expected: a PNG file appears on the Desktop showing the editor window.

- [ ] **Step 8: Apply an action**

With a doc open, pick a block id from `/state`, then:
```powershell
$body = '{"type":"EditInline","block_id":1,"new_runs":[{"text":"Hello from Claude","bold":false,"italic":false,"code":false,"link":null}]}'
Invoke-RestMethod -Method POST -Uri http://localhost:7878/action -Body $body -ContentType application/json
```
Expected: `ok` — the block's text updates in the editor window.

- [ ] **Step 9: Input injection**

Click into a text block in the editor first, then:
```powershell
Invoke-RestMethod -Method POST -Uri http://localhost:7878/input -Body '{"type":"text","text":" world"}' -ContentType application/json
```
Expected: `ok` — the text " world" appears at the cursor.

```powershell
Invoke-RestMethod -Method POST -Uri http://localhost:7878/input -Body '{"type":"keys","keys":"ctrl+b"}' -ContentType application/json
```
Expected: `ok` — bold toggle fires (if text is selected).

- [ ] **Step 10: Commit**

If any fixes were required during smoke test, commit them. Then:
```
git add -A
git commit -m "feat(ctrl): smoke-tested claude editor control server — all endpoints working"
```

---

## Known Limitations

- **Rapid sequential POST /action calls** may have the last one win if they arrive faster than the Floem event loop can drain `create_signal_from_channel`'s internal queue. For debug use (Claude sends one action at a time and awaits response), this is not an issue.
- **Screenshot requires the window to be visible and not minimized.** Returns 503 if `FindWindowW` fails.
- **Input injection requires the editor window to be the foreground window.** `SetForegroundWindow` is called automatically, but Windows may silently refuse to bring a window to the foreground if the current foreground process isn't trusted — in that case, clicking the taskbar icon manually first resolves it.
- **`#[cfg(...)]` on function call arguments** (`#[cfg(debug_assertions)] arg`) is valid stable Rust syntax but less common. If a compiler version objects, refactor to pass a single `Option<CtrlBundle>` struct instead (always `Some` in debug, `None` in release via a dummy constructor).

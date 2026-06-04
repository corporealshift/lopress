#![allow(unsafe_code)]
#![cfg(debug_assertions)]
// Debug-only control server: the Win32 screenshot path and static header
// parsing inherently need casts / `expect` that the workspace lints forbid.
#![allow(clippy::cast_possible_truncation, clippy::expect_used)]

pub(crate) mod input;

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use serde::Deserialize;

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun};

// ── Public handle ─────────────────────────────────────────────────────────────

pub(crate) struct CtrlHandle {
    pub snapshot: Arc<Mutex<String>>,
    #[allow(dead_code)]
    pub open_tx: crossbeam_channel::Sender<CtrlOpenEnvelope>,
    #[allow(dead_code)]
    pub close_tx: crossbeam_channel::Sender<CtrlCloseEnvelope>,
}

// ── Action types (HTTP API) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CtrlAction {
    Split {
        block_id: u64,
        byte_offset: usize,
    },
    MergeWithPrev {
        block_id: u64,
    },
    Delete {
        block_id: u64,
    },
    Move {
        block_id: u64,
        to_index: usize,
    },
    ChangeType {
        block_id: u64,
        new_kind: CtrlBlockKind,
    },
    InsertAfter {
        block_id: u64,
        new_block: CtrlNewBlock,
    },
    EditInline {
        block_id: u64,
        new_runs: Vec<InlineRun>,
    },
    EditCode {
        block_id: u64,
        new_text: String,
    },
    EditAttrs {
        block_id: u64,
        new_attrs: serde_json::Map<String, serde_json::Value>,
    },
    TableInsertRow {
        block_id: u64,
        at: usize,
    },
    TableDeleteRow {
        block_id: u64,
        row: usize,
    },
    TableInsertColumn {
        block_id: u64,
        at: usize,
    },
    TableDeleteColumn {
        block_id: u64,
        col: usize,
    },
    TableSetAlign {
        block_id: u64,
        col: usize,
        align: CtrlAlign,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CtrlBlockKind {
    Paragraph,
    Heading { level: u8 },
    Code { lang: String },
    List { ordered: bool },
    Table,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) enum CtrlAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) enum CtrlNewBlock {
    Separator,
    Table {
        #[serde(default)]
        rows: usize,
        #[serde(default)]
        cols: usize,
    },
}

impl CtrlAction {
    pub(crate) fn into_block_action(self, doc: &EditorDoc) -> Option<BlockAction> {
        fn find(doc: &EditorDoc, raw: u64) -> Option<BlockId> {
            doc.blocks.iter().find(|b| b.id.raw() == raw).map(|b| b.id)
        }
        Some(match self {
            CtrlAction::Split {
                block_id,
                byte_offset,
            } => BlockAction::Split {
                block_id: find(doc, block_id)?,
                byte_offset,
                new_block_id: None,
            },
            CtrlAction::MergeWithPrev { block_id } => BlockAction::MergeWithPrev {
                block_id: find(doc, block_id)?,
            },
            CtrlAction::Delete { block_id } => BlockAction::Delete {
                block_id: find(doc, block_id)?,
            },
            CtrlAction::Move { block_id, to_index } => BlockAction::Move {
                block_id: find(doc, block_id)?,
                to_index,
            },
            CtrlAction::ChangeType { block_id, new_kind } => BlockAction::ChangeType {
                block_id: find(doc, block_id)?,
                new_kind: match new_kind {
                    CtrlBlockKind::Paragraph => BlockKind::Paragraph,
                    CtrlBlockKind::Heading { level } => BlockKind::Heading(level.clamp(1, 6)),
                    CtrlBlockKind::Code { lang } => BlockKind::Code {
                        lang: Rc::from(lang),
                    },
                    CtrlBlockKind::List { ordered } => BlockKind::List { ordered },
                    CtrlBlockKind::Table => BlockKind::Table,
                },
            },
            CtrlAction::InsertAfter {
                block_id,
                new_block,
            } => {
                let anchor = find(doc, block_id)?;
                let new_editor_block = match new_block {
                    CtrlNewBlock::Separator => EditorBlock::separator(),
                    CtrlNewBlock::Table { rows, cols } => {
                        let empty_cell = || crate::model::types::TableCell {
                            id: BlockId::new(),
                            runs: vec![],
                        };
                        let empty_row = || crate::model::types::TableRow {
                            id: BlockId::new(),
                            cells: (0..cols).map(|_| empty_cell()).collect(),
                        };
                        EditorBlock::table(crate::model::types::TableData {
                            align: vec![crate::model::types::Align::None; cols],
                            rows: (0..rows).map(|_| empty_row()).collect(),
                        })
                    }
                };
                BlockAction::InsertAfter {
                    anchor,
                    new_block: Box::new(new_editor_block),
                }
            }
            CtrlAction::EditInline { block_id, new_runs } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                built_in: false, // External input via control server.
            },
            CtrlAction::EditCode { block_id, new_text } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: Box::new(crate::model::types::BlockBody::Code(new_text)),
                built_in: false, // External input via control server.
            },
            CtrlAction::EditAttrs {
                block_id,
                new_attrs,
            } => BlockAction::EditAttrs {
                block_id: find(doc, block_id)?,
                new_attrs: Box::new(new_attrs),
            },
            CtrlAction::TableInsertRow { block_id, at } => BlockAction::TableInsertRow {
                block_id: find(doc, block_id)?,
                at,
            },
            CtrlAction::TableDeleteRow { block_id, row } => BlockAction::TableDeleteRow {
                block_id: find(doc, block_id)?,
                row,
            },
            CtrlAction::TableInsertColumn { block_id, at } => BlockAction::TableInsertColumn {
                block_id: find(doc, block_id)?,
                at,
            },
            CtrlAction::TableDeleteColumn { block_id, col } => BlockAction::TableDeleteColumn {
                block_id: find(doc, block_id)?,
                col,
            },
            CtrlAction::TableSetAlign {
                block_id,
                col,
                align,
            } => {
                let ctrl_align = match align {
                    CtrlAlign::None => crate::model::types::Align::None,
                    CtrlAlign::Left => crate::model::types::Align::Left,
                    CtrlAlign::Center => crate::model::types::Align::Center,
                    CtrlAlign::Right => crate::model::types::Align::Right,
                };
                BlockAction::TableSetAlign {
                    block_id: find(doc, block_id)?,
                    col,
                    align: ctrl_align,
                }
            }
        })
    }

    /// The raw `u64` block id this action targets. Every variant carries
    /// one. Used to report which block was missing when translation fails.
    pub(crate) fn block_id(&self) -> u64 {
        match self {
            CtrlAction::Split { block_id, .. }
            | CtrlAction::MergeWithPrev { block_id }
            | CtrlAction::Delete { block_id }
            | CtrlAction::Move { block_id, .. }
            | CtrlAction::ChangeType { block_id, .. }
            | CtrlAction::InsertAfter { block_id, .. }
            | CtrlAction::EditInline { block_id, .. }
            | CtrlAction::EditCode { block_id, .. }
            | CtrlAction::EditAttrs { block_id, .. }
            | CtrlAction::TableInsertRow { block_id, .. }
            | CtrlAction::TableDeleteRow { block_id, .. }
            | CtrlAction::TableInsertColumn { block_id, .. }
            | CtrlAction::TableDeleteColumn { block_id, .. }
            | CtrlAction::TableSetAlign { block_id, .. } => *block_id,
        }
    }
}

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
            CtrlActionResult::Dispatched => (
                200,
                serde_json::json!({ "status": "dispatched" }).to_string(),
            ),
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

/// What travels the `/action` channel: the parsed action plus a one-shot
/// reply sender the UI thread uses to report the outcome back to the
/// blocked HTTP handler.
pub(crate) type CtrlActionEnvelope = (CtrlAction, Sender<CtrlActionResult>);

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

// ── Doc state serialization ───────────────────────────────────────────────────

pub(crate) fn serialize_state(doc: Option<&EditorDoc>, path: Option<&std::path::Path>) -> String {
    let Some(doc) = doc else {
        return r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string();
    };

    let blocks: Vec<serde_json::Value> = doc
        .blocks
        .iter()
        .map(|b| {
            let id = b.id.raw();
            match &b.body {
                BlockBody::Inline(runs) => {
                    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                    let kind = match &b.kind {
                        BlockKind::Paragraph => "Paragraph".to_string(),
                        BlockKind::Heading(n) => format!("Heading{n}"),
                        BlockKind::Code { .. } => "Code".to_string(),
                        BlockKind::List { .. } => "List".to_string(),
                        BlockKind::Image => "Image".to_string(),
                        BlockKind::Table => "Table".to_string(),
                        BlockKind::Opaque { type_name } => format!("Opaque({type_name})"),
                    };
                    serde_json::json!({ "id": id, "kind": kind, "text": text })
                }
                BlockBody::Table(data) => {
                    let text = crate::actions::body_to_flat_text(&BlockBody::Table(data.clone()));
                    let kind = match &b.kind {
                        BlockKind::Table => "Table".to_string(),
                        _ => "Table".to_string(),
                    };
                    serde_json::json!({ "id": id, "kind": kind, "text": text })
                }
                BlockBody::Code(text) => {
                    let lang = match &b.kind {
                        BlockKind::Code { lang } => lang.clone(),
                        _ => Rc::from(""),
                    };
                    serde_json::json!({ "id": id, "kind": "Code", "lang": &*lang, "text": text })
                }
                BlockBody::List(items) => {
                    let text = items
                        .iter()
                        .map(|item| {
                            item.runs
                                .iter()
                                .map(|r| r.text.as_str())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    serde_json::json!({ "id": id, "kind": "List", "text": text })
                }
                BlockBody::Opaque(_) => {
                    let type_name = match &b.kind {
                        BlockKind::Opaque { type_name } => type_name.clone(),
                        _ => Rc::from(""),
                    };
                    serde_json::json!({
                        "id": id,
                        "kind": format!("Opaque({type_name})"),
                        "text": ""
                    })
                }
            }
        })
        .collect();

    serde_json::json!({
        "doc_open": true,
        "path": path.map(|p| p.to_string_lossy().to_string()),
        "blocks": blocks,
    })
    .to_string()
}

// ── Startup ───────────────────────────────────────────────────────────────────

pub(crate) fn start() -> (
    CtrlHandle,
    crossbeam_channel::Receiver<CtrlActionEnvelope>,
    crossbeam_channel::Receiver<CtrlOpenEnvelope>,
    crossbeam_channel::Receiver<CtrlCloseEnvelope>,
) {
    let snapshot: Arc<Mutex<String>> = Arc::new(Mutex::new(
        r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string(),
    ));
    let (action_tx, action_rx) = crossbeam_channel::unbounded::<CtrlActionEnvelope>();
    let (open_tx, open_rx) = crossbeam_channel::unbounded::<CtrlOpenEnvelope>();
    let (close_tx, close_rx) = crossbeam_channel::unbounded::<CtrlCloseEnvelope>();

    let handle = CtrlHandle {
        snapshot: Arc::clone(&snapshot),
        open_tx: open_tx.clone(),
        close_tx: close_tx.clone(),
    };

    let server_snapshot = Arc::clone(&snapshot);
    std::thread::spawn(move || {
        serve(server_snapshot, action_tx, open_tx, close_tx);
    });

    (handle, action_rx, open_rx, close_rx)
}

// ── HTTP server ───────────────────────────────────────────────────────────────

fn serve(
    snapshot: Arc<Mutex<String>>,
    action_tx: Sender<CtrlActionEnvelope>,
    open_tx: Sender<CtrlOpenEnvelope>,
    close_tx: Sender<CtrlCloseEnvelope>,
) {
    let server = match tiny_http::Server::http("127.0.0.1:7878") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[ctrl] failed to bind 127.0.0.1:7878: {e}");
            return;
        }
    };
    eprintln!("[ctrl] listening on http://127.0.0.1:7878");

    for mut request in server.incoming_requests() {
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();

        let response = handle_request(
            &mut request,
            &method,
            &url,
            &snapshot,
            &action_tx,
            &open_tx,
            &close_tx,
        );
        let _ = request.respond(response);
    }
}

fn handle_request(
    request: &mut tiny_http::Request,
    method: &str,
    url: &str,
    snapshot: &Arc<Mutex<String>>,
    action_tx: &Sender<CtrlActionEnvelope>,
    open_tx: &Sender<CtrlOpenEnvelope>,
    close_tx: &Sender<CtrlCloseEnvelope>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match (method, url) {
        ("GET", "/ping") => text_response("ok", 200),

        ("GET", "/state") => {
            let body = snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone();
            tiny_http::Response::from_string(body)
                .with_header(json_header())
                .with_status_code(200)
        }

        ("GET", "/screenshot") => match screenshot() {
            Ok(png) => tiny_http::Response::from_data(png)
                .with_header(png_header())
                .with_status_code(200),
            Err(e) => text_response(&e, 503),
        },

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
                    let (reply_tx, reply_rx) = crossbeam_channel::bounded::<CtrlActionResult>(1);
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

        ("POST", "/input") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                return text_response("read error", 400);
            }
            match input::handle_input(&body) {
                Ok(()) => text_response("ok", 200),
                Err(e) => text_response(&e, 400),
            }
        }

        ("POST", "/click") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                return text_response("read error", 400);
            }
            match input::handle_click(&body) {
                Ok(()) => text_response("ok", 200),
                Err(e) => text_response(&e, 400),
            }
        }

        ("POST", "/open") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                return text_response("read error", 400);
            }
            match serde_json::from_str::<CtrlOpenRequest>(&body) {
                Ok(req) => {
                    let (reply_tx, reply_rx) = crossbeam_channel::bounded::<CtrlOpenResult>(1);
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
            let (reply_tx, reply_rx) = crossbeam_channel::bounded::<CtrlCloseResult>(1);
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

        _ => text_response("not found", 404),
    }
}

fn text_response(body: &str, code: u16) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(body).with_status_code(code)
}

fn json_header() -> tiny_http::Header {
    "Content-Type: application/json"
        .parse()
        .expect("static header is valid")
}

fn png_header() -> tiny_http::Header {
    "Content-Type: image/png"
        .parse()
        .expect("static header is valid")
}

// ── Screenshot ────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn screenshot() -> Result<Vec<u8>, String> {
    use std::mem;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClientRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    // Temporarily make the window topmost so DWM composites its wgpu surface
    // to the screen, then capture from the screen DC at the client-area origin.
    // BitBlt from the window DC misses wgpu content when the window is in the
    // background; the screen DC captures what's actually composited by DWM.
    const HWND_TOPMOST: HWND = HWND(-1);
    const HWND_NOTOPMOST: HWND = HWND(-2);

    unsafe {
        let title: Vec<u16> = "lopress\0".encode_utf16().collect();
        let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
        if hwnd.0 == 0 {
            return Err("window not found".to_string());
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err("window has zero size".to_string());
        }

        // Bring window to the very top (TOPMOST) so its wgpu content is
        // composited by DWM, then capture from the screen at the client origin.
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Find screen position of the client area origin.
        let mut origin = POINT { x: 0, y: 0 };
        windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin);

        // Capture from the screen DC at the client area position.
        let hdc_screen = GetDC(HWND(0)); // desktop DC
        let hdc_dst = CreateCompatibleDC(hdc_screen);
        let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
        let old = SelectObject(hdc_dst, hbm);

        BitBlt(
            hdc_dst, 0, 0, width, height, hdc_screen, origin.x, origin.y, SRCCOPY,
        )
        .map_err(|e| e.to_string())?;

        // Restore to non-topmost.
        let _ = SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );

        let pixel_count =
            usize::try_from(width).unwrap_or(0) * usize::try_from(height).unwrap_or(0);
        let mut pixels: Vec<u8> = vec![0u8; pixel_count * 4];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };

        GetDIBits(
            hdc_dst,
            hbm,
            0,
            u32::try_from(height).unwrap_or(0),
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_dst, old);
        DeleteObject(hbm);
        DeleteDC(hdc_dst);
        ReleaseDC(HWND(0), hdc_screen);

        // BGRA → RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        // Encode to PNG
        let mut png_bytes: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            &pixels,
            u32::try_from(width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;

        Ok(png_bytes)
    }
}

#[cfg(not(target_os = "windows"))]
fn screenshot() -> Result<Vec<u8>, String> {
    Err("screenshots only supported on Windows".to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::types::EditorBlock;

    fn doc_one_paragraph() -> (EditorDoc, u64) {
        let block = EditorBlock::paragraph(vec![InlineRun::plain("text")]);
        let raw = block.id.raw();
        let doc = EditorDoc {
            blocks: vec![block],
            front_matter: lopress_core::FrontMatter::default(),
        };
        (doc, raw)
    }

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
                ref new_body,
                ..
            } => match new_body.as_ref() {
                BlockBody::Inline(runs) => {
                    assert_eq!(block_id.raw(), raw);
                    assert_eq!(*runs, vec![InlineRun::plain("new")]);
                }
                other => panic!("expected EditBlockBody/Inline, got {other:?}"),
            },
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn edit_code_translates_to_edit_block_body_code() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::EditCode {
            block_id: raw,
            new_text: "fn main() {}".to_string(),
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::EditBlockBody {
                block_id,
                ref new_body,
                ..
            } => match new_body.as_ref() {
                BlockBody::Code(text) => {
                    assert_eq!(block_id.raw(), raw);
                    assert_eq!(text, "fn main() {}");
                }
                other => panic!("expected EditBlockBody/Code, got {other:?}"),
            },
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn split_translates_with_new_block_id_none() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::Split {
            block_id: raw,
            byte_offset: 5,
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::Split {
                block_id,
                byte_offset,
                new_block_id,
            } => {
                assert_eq!(block_id.raw(), raw);
                assert_eq!(byte_offset, 5);
                assert!(new_block_id.is_none());
            }
            other => panic!("expected Split, got {other:?}"),
        }
    }

    #[test]
    fn merge_with_prev_translates() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::MergeWithPrev { block_id: raw };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::MergeWithPrev { block_id } => {
                assert_eq!(block_id.raw(), raw);
            }
            other => panic!("expected MergeWithPrev, got {other:?}"),
        }
    }

    #[test]
    fn delete_translates() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::Delete { block_id: raw };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::Delete { block_id } => {
                assert_eq!(block_id.raw(), raw);
            }
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    #[test]
    fn move_action_preserves_to_index() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::Move {
            block_id: raw,
            to_index: 3,
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::Move { block_id, to_index } => {
                assert_eq!(block_id.raw(), raw);
                assert_eq!(to_index, 3);
            }
            other => panic!("expected Move, got {other:?}"),
        }
    }

    #[test]
    fn edit_attrs_round_trips_map() {
        let (doc, raw) = doc_one_paragraph();
        let mut map = serde_json::Map::new();
        map.insert("lang".into(), "rust".into());
        map.insert("count".into(), 42.into());
        let ctrl = CtrlAction::EditAttrs {
            block_id: raw,
            new_attrs: map.clone(),
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::EditAttrs {
                block_id,
                new_attrs,
            } => {
                assert_eq!(block_id.raw(), raw);
                assert_eq!(*new_attrs, map);
            }
            other => panic!("expected EditAttrs, got {other:?}"),
        }
    }

    #[test]
    fn change_type_maps_each_kind() {
        let (doc, raw) = doc_one_paragraph();
        let cases = [
            (CtrlBlockKind::Paragraph, BlockKind::Paragraph),
            (CtrlBlockKind::Heading { level: 2 }, BlockKind::Heading(2)),
            (
                CtrlBlockKind::Code {
                    lang: "rust".to_string(),
                },
                BlockKind::Code {
                    lang: Rc::from("rust"),
                },
            ),
            (
                CtrlBlockKind::List { ordered: true },
                BlockKind::List { ordered: true },
            ),
        ];
        for (ctrl_kind, expected_block_kind) in cases {
            let ctrl = CtrlAction::ChangeType {
                block_id: raw,
                new_kind: ctrl_kind,
            };
            match ctrl.into_block_action(&doc).expect("known id translates") {
                BlockAction::ChangeType { block_id, new_kind } => {
                    assert_eq!(block_id.raw(), raw);
                    assert_eq!(new_kind, expected_block_kind);
                }
                other => panic!("expected ChangeType, got {other:?}"),
            }
        }
    }

    #[test]
    fn change_type_clamps_heading_level() {
        let (doc, raw) = doc_one_paragraph();

        // level 9 clamps to 6
        let ctrl = CtrlAction::ChangeType {
            block_id: raw,
            new_kind: CtrlBlockKind::Heading { level: 9 },
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::ChangeType { new_kind, .. } => {
                assert_eq!(new_kind, BlockKind::Heading(6));
            }
            other => panic!("expected ChangeType, got {other:?}"),
        }

        // level 0 clamps to 1
        let ctrl = CtrlAction::ChangeType {
            block_id: raw,
            new_kind: CtrlBlockKind::Heading { level: 0 },
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::ChangeType { new_kind, .. } => {
                assert_eq!(new_kind, BlockKind::Heading(1));
            }
            other => panic!("expected ChangeType, got {other:?}"),
        }
    }

    #[test]
    fn ctrl_action_result_dispatched_http_parts() {
        let (code, body) = CtrlActionResult::Dispatched.http_response_parts();
        assert_eq!(code, 200);
        assert!(body.contains(r#""status":"dispatched""#));
    }

    #[test]
    fn ctrl_action_result_no_document_http_parts() {
        let (code, body) = CtrlActionResult::NoDocument.http_response_parts();
        assert_eq!(code, 409);
        assert!(body.contains(r#""status":"no_document""#));
        assert!(body.contains(r#""detail":"no document is open""#));
    }

    #[test]
    fn ctrl_action_result_block_not_found_http_parts() {
        let id: u64 = 42;
        let (code, body) = CtrlActionResult::BlockNotFound { block_id: id }.http_response_parts();
        assert_eq!(code, 422);
        assert!(body.contains(r#""status":"block_not_found""#));
        assert!(body.contains(r#""block_id":42"#));
    }

    #[test]
    fn block_id_accessor_returns_each_variant_id() {
        // Each variant gets a distinct id (1..=8) so a slipped match arm
        // would produce the wrong value.
        assert_eq!(
            CtrlAction::Split {
                block_id: 1,
                byte_offset: 0
            }
            .block_id(),
            1
        );
        assert_eq!(CtrlAction::MergeWithPrev { block_id: 2 }.block_id(), 2);
        assert_eq!(CtrlAction::Delete { block_id: 3 }.block_id(), 3);
        assert_eq!(
            CtrlAction::Move {
                block_id: 4,
                to_index: 0
            }
            .block_id(),
            4
        );
        assert_eq!(
            CtrlAction::ChangeType {
                block_id: 5,
                new_kind: CtrlBlockKind::Paragraph
            }
            .block_id(),
            5
        );
        assert_eq!(
            CtrlAction::EditInline {
                block_id: 6,
                new_runs: vec![]
            }
            .block_id(),
            6
        );
        assert_eq!(
            CtrlAction::EditCode {
                block_id: 7,
                new_text: "".to_string()
            }
            .block_id(),
            7
        );
        assert_eq!(
            CtrlAction::EditAttrs {
                block_id: 8,
                new_attrs: serde_json::Map::new()
            }
            .block_id(),
            8
        );
    }

    #[test]
    fn unknown_block_id_returns_none() {
        let (doc, _raw) = doc_one_paragraph();
        let unknown_id = u64::MAX;
        let ctrl = CtrlAction::Delete {
            block_id: unknown_id,
        };
        assert!(ctrl.into_block_action(&doc).is_none());
    }

    #[test]
    fn open_result_http_parts() {
        assert_eq!(
            CtrlOpenResult::Opened.http_parts(),
            (200, r#"{"status":"opened"}"#.to_string()),
        );
        assert_eq!(
            CtrlOpenResult::NotFound.http_parts(),
            (404, r#"{"status":"not_found"}"#.to_string()),
        );
        assert_eq!(
            CtrlOpenResult::NoWorkspace.http_parts(),
            (409, r#"{"status":"no_workspace"}"#.to_string()),
        );
    }

    #[test]
    fn close_result_http_parts() {
        assert_eq!(
            CtrlCloseResult::Closed.http_parts(),
            (200, r#"{"status":"closed"}"#.to_string()),
        );
        assert_eq!(
            CtrlCloseResult::NoWorkspace.http_parts(),
            (409, r#"{"status":"no_workspace"}"#.to_string()),
        );
    }
}

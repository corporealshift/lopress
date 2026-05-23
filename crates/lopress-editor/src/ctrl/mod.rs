#![allow(unsafe_code)]
#![cfg(debug_assertions)]
// Debug-only control server: the Win32 screenshot path and static header
// parsing inherently need casts / `expect` that the workspace lints forbid.
#![allow(clippy::cast_possible_truncation, clippy::expect_used)]

pub(crate) mod input;

use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use serde::Deserialize;

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, BlockKind, EditorDoc, InlineRun};

// ── Public handle ─────────────────────────────────────────────────────────────

pub(crate) struct CtrlHandle {
    pub snapshot: Arc<Mutex<String>>,
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CtrlBlockKind {
    Paragraph,
    Heading { level: u8 },
    Code { lang: String },
    List { ordered: bool },
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
                    CtrlBlockKind::Code { lang } => BlockKind::Code { lang },
                    CtrlBlockKind::List { ordered } => BlockKind::List { ordered },
                },
            },
            CtrlAction::EditInline { block_id, new_runs } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: crate::model::types::BlockBody::Inline(new_runs),
            },
            CtrlAction::EditCode { block_id, new_text } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: crate::model::types::BlockBody::Code(new_text),
            },
            CtrlAction::EditAttrs {
                block_id,
                new_attrs,
            } => BlockAction::EditAttrs {
                block_id: find(doc, block_id)?,
                new_attrs,
            },
        })
    }
}

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
                        BlockKind::Opaque { type_name } => format!("Opaque({type_name})"),
                    };
                    serde_json::json!({ "id": id, "kind": kind, "text": text })
                }
                BlockBody::Code(text) => {
                    let lang = match &b.kind {
                        BlockKind::Code { lang } => lang.clone(),
                        _ => String::new(),
                    };
                    serde_json::json!({ "id": id, "kind": "Code", "lang": lang, "text": text })
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
                        _ => String::new(),
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

pub(crate) fn start() -> (CtrlHandle, crossbeam_channel::Receiver<CtrlAction>) {
    let snapshot: Arc<Mutex<String>> = Arc::new(Mutex::new(
        r#"{"doc_open":false,"path":null,"blocks":[]}"#.to_string(),
    ));
    let (action_tx, action_rx) = crossbeam_channel::unbounded::<CtrlAction>();

    let handle = CtrlHandle {
        snapshot: Arc::clone(&snapshot),
    };

    let server_snapshot = Arc::clone(&snapshot);
    std::thread::spawn(move || {
        serve(server_snapshot, action_tx);
    });

    (handle, action_rx)
}

// ── HTTP server ───────────────────────────────────────────────────────────────

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
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();

        let response = handle_request(&mut request, &method, &url, &snapshot, &action_tx);
        let _ = request.respond(response);
    }
}

fn handle_request(
    request: &mut tiny_http::Request,
    method: &str,
    url: &str,
    snapshot: &Arc<Mutex<String>>,
    action_tx: &Sender<CtrlAction>,
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
                    let _ = action_tx.send(action);
                    text_response("ok", 200)
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
                new_body: BlockBody::Inline(runs),
            } => {
                assert_eq!(block_id.raw(), raw);
                assert_eq!(runs, vec![InlineRun::plain("new")]);
            }
            other => panic!("expected EditBlockBody/Inline, got {other:?}"),
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
                new_body: BlockBody::Code(text),
            } => {
                assert_eq!(block_id.raw(), raw);
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected EditBlockBody/Code, got {other:?}"),
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
                assert_eq!(new_attrs, map);
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
                    lang: "rust".to_string(),
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
    fn unknown_block_id_returns_none() {
        let (doc, _raw) = doc_one_paragraph();
        let unknown_id = u64::MAX;
        let ctrl = CtrlAction::Delete {
            block_id: unknown_id,
        };
        assert!(ctrl.into_block_action(&doc).is_none());
    }
}

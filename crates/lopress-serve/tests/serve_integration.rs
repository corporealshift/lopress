#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::integer_division,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use lopress_serve::{serve, ServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn make_minimal_workspace(root: &std::path::Path) {
    std::fs::write(
        root.join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://example.com\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();
    std::fs::write(
        root.join("src/posts/hi.md"),
        "---\ntitle: Hi\ndate: 2026-04-19\n---\n\n# Hi\n",
    )
    .unwrap();
}

fn start_server(root: std::path::PathBuf) -> u16 {
    // Let the server bind port 0 and tell us the chosen port through a
    // channel — avoids the bind-then-drop race where another process can
    // claim the port between probe.drop() and serve.bind().
    let (tx, rx) = std::sync::mpsc::channel::<std::net::SocketAddr>();
    std::thread::spawn(move || {
        let _ = serve(ServeOptions {
            workspace: root,
            bind: "127.0.0.1".into(),
            port: 0,
            open_browser: false,
            on_ready: Some(Box::new(move |addr| {
                let _ = tx.send(addr);
            })),
        });
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("server never signaled ready");
    // Wait for the accept loop to actually be accepting.
    for _ in 0..50 {
        if TcpStream::connect(addr).is_ok() {
            return addr.port();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server never accepted on {addr}");
}

fn get(port: u16, path: &str) -> (String, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).into_owned();
    (head, buf[split + 4..].to_vec())
}

#[test]
fn index_has_reload_script() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let (head, body) = get(port, "/");
    assert!(head.contains("200 OK"));
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("EventSource"),
        "missing reload script: {body_str}"
    );
}

#[test]
fn sse_endpoint_returns_event_stream_headers() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    write!(
        s,
        "GET /__lopress/reload HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n"
    )
    .unwrap();
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).unwrap();
    let head = String::from_utf8_lossy(&buf[..n]);
    assert!(head.contains("text/event-stream"), "got: {head}");
    assert!(head.contains("retry: 1000"), "got: {head}");
}

#[test]
fn missing_path_returns_404_body() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    make_minimal_workspace(&root);
    let port = start_server(root);

    let (head, _body) = get(port, "/not/found");
    assert!(head.contains("404"));
}

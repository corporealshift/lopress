use crate::error::ServeError;
use crate::http::{read_request, write_response};
use crate::router::{resolve, Resolved};
use crate::sse::Subscribers;
use lopress_watch::Watcher;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ServeOptions {
    pub workspace: PathBuf,
    pub bind: String,
    pub port: u16,
    pub open_browser: bool,
    /// Invoked once after the listener is bound, with the actual bound
    /// address. Used by tests to learn the port when `port: 0` is passed.
    pub on_ready: Option<Box<dyn FnOnce(std::net::SocketAddr) + Send>>,
}

pub fn serve(opts: ServeOptions) -> Result<(), ServeError> {
    // 1. Initial full build.
    let report = lopress_build::build(&opts.workspace)?;
    eprintln!(
        "initial build: {} rendered, {} skipped, {} failure(s)",
        report.pages_rendered,
        report.pages_skipped,
        report.failures.len()
    );

    // 2. Bind HTTP listener. Use local_addr() for logging/open so that
    // `port: 0` (pick an ephemeral port) reports the real port.
    let bind_addr = format!("{}:{}", opts.bind, opts.port);
    let listener = TcpListener::bind(&bind_addr).map_err(|source| ServeError::Bind {
        addr: bind_addr.clone(),
        source,
    })?;
    let local = listener.local_addr().map_err(|source| ServeError::Bind {
        addr: bind_addr.clone(),
        source,
    })?;
    let addr = local.to_string();
    if let Some(cb) = opts.on_ready {
        cb(local);
    }
    eprintln!(
        "serving http://{addr}/  (watching {})",
        opts.workspace.display()
    );

    let subs = Subscribers::default();
    let _ping = subs.clone().ping_loop();

    // 3. Watcher: on change, rebuild and broadcast.
    let ws = opts.workspace.clone();
    let subs_for_watch = subs.clone();
    let _watcher = Watcher::spawn(&opts.workspace, move |_cs| {
        match lopress_build::build(&ws) {
            Ok(r) => {
                eprintln!(
                    "rebuild: {} rendered, {} skipped, {} failure(s)",
                    r.pages_rendered,
                    r.pages_skipped,
                    r.failures.len()
                );
                subs_for_watch.broadcast_reload();
            }
            Err(e) => eprintln!("rebuild failed: {e}"),
        }
    })?;

    // 4. Optionally open the default browser.
    if opts.open_browser {
        let url = format!("http://{addr}/");
        std::thread::spawn(move || open_url(&url));
    }

    // 5. Accept loop.
    let www = Arc::new(opts.workspace.join("www"));
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        let www = Arc::clone(&www);
        let subs = subs.clone();
        std::thread::spawn(move || {
            let _ = handle_conn(stream, &www, &subs);
        });
    }
    Ok(())
}

fn handle_conn(
    mut stream: std::net::TcpStream,
    www: &std::path::Path,
    subs: &Subscribers,
) -> std::io::Result<()> {
    let req = match read_request(&stream)? {
        Some(r) => r,
        None => return Ok(()),
    };
    if req.method != "GET" {
        return write_response(&mut stream, 405, "Method Not Allowed", &[], b"");
    }
    if req.path.starts_with("/__lopress/reload") {
        // Hand the stream to the SSE subscribers; return without closing.
        return subs.add(stream);
    }

    match resolve(www, &req.path)? {
        Resolved::File { content_type, body } => write_response(
            &mut stream,
            200,
            "OK",
            &[("content-type", content_type)],
            &body,
        ),
        Resolved::Redirect { location } => write_response(
            &mut stream,
            301,
            "Moved Permanently",
            &[("location", &location)],
            b"",
        ),
        Resolved::NotFound { body } => write_response(
            &mut stream,
            404,
            "Not Found",
            &[("content-type", "text/html; charset=utf-8")],
            &body,
        ),
        Resolved::Forbidden => write_response(&mut stream, 403, "Forbidden", &[], b""),
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}

/// A running background HTTP+SSE server. Drop to stop the ping thread;
/// the accept thread runs until process exit.
pub struct ServerHandle {
    /// The bound URL, e.g. `"http://127.0.0.1:8080"`.
    pub url: String,
    subscribers: Subscribers,
}

impl ServerHandle {
    /// Broadcast a reload event to all connected SSE clients.
    pub fn broadcast_reload(&self) {
        self.subscribers.broadcast_reload();
    }
}

/// Bind a static file server for `www_dir` on a background thread.
/// Does **not** run an initial build — the caller is responsible.
/// Port 0 selects an ephemeral port; the resolved URL is in `ServerHandle::url`.
///
/// # Errors
/// Returns `ServeError::Bind` if the port is already in use.
pub fn serve_in_background(
    www_dir: std::path::PathBuf,
    bind: String,
    port: u16,
) -> Result<ServerHandle, ServeError> {
    let bind_addr = format!("{bind}:{port}");
    let listener = TcpListener::bind(&bind_addr).map_err(|source| ServeError::Bind {
        addr: bind_addr.clone(),
        source,
    })?;
    let local = listener.local_addr().map_err(|source| ServeError::Bind {
        addr: bind_addr,
        source,
    })?;
    let url = format!("http://{local}");

    let subs = Subscribers::default();
    let _ping = subs.clone().ping_loop();

    let www = Arc::new(www_dir);
    let subs_accept = subs.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let www = Arc::clone(&www);
            let subs = subs_accept.clone();
            std::thread::spawn(move || {
                let _ = handle_conn(stream, &www, &subs);
            });
        }
    });

    Ok(ServerHandle {
        url,
        subscribers: subs,
    })
}

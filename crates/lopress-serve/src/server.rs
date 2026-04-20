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

    // 2. Bind HTTP listener.
    let addr = format!("{}:{}", opts.bind, opts.port);
    let listener = TcpListener::bind(&addr).map_err(|source| ServeError::Bind {
        addr: addr.clone(),
        source,
    })?;
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

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice
)]

use lopress_gui_host::{BuildStatus, ServeStatus, Session};
use std::fs;
use tempfile::TempDir;

fn make_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    fs::write(
        p.join("lopress.toml"),
        "[site]\ntitle = \"Test\"\nbase_url = \"https://example.com\"\n",
    )
    .unwrap();
    fs::create_dir_all(p.join("src/posts")).unwrap();
    fs::create_dir_all(p.join("src/pages")).unwrap();
    fs::create_dir_all(p.join("src/images")).unwrap();
    fs::create_dir_all(p.join("plugins")).unwrap();
    fs::write(
        p.join("src/posts/hello.md"),
        "---\ntitle: Hello\ndate: 2026-04-20\n---\n\n# Hello\n\nWorld.\n",
    )
    .unwrap();
    dir
}

/// Poll `read()` every 20 ms until `done(result)` returns true, or panic after
/// `timeout`. Used by tests that have to wait for the deferred background
/// thread in `Session::open` to finish its work.
fn wait_until<T>(
    timeout: std::time::Duration,
    mut read: impl FnMut() -> T,
    done: impl Fn(&T) -> bool,
) -> T {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let v = read();
        if done(&v) {
            return v;
        }
        if std::time::Instant::now() > deadline {
            panic!("wait_until timed out after {:?}", timeout);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn open_valid_workspace_succeeds() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let summary = session.workspace();
    assert_eq!(summary.name, "Test");
    assert_eq!(summary.posts.len(), 1);
    assert_eq!(summary.posts[0].title, "Hello");
    assert!(!summary.posts[0].has_parse_error);
}

#[test]
fn open_invalid_workspace_errors() {
    let dir = TempDir::new().unwrap();
    assert!(Session::open(dir.path()).is_err());
}

#[test]
fn build_status_is_ok_after_open() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let final_status = wait_until(
        std::time::Duration::from_secs(5),
        || session.build_status(),
        |s| matches!(s, BuildStatus::Ok { .. } | BuildStatus::Failed { .. }),
    );
    assert!(matches!(final_status, BuildStatus::Ok { .. }));
}

#[test]
fn load_and_save_document_roundtrip() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let post_path = dir.path().join("src/posts/hello.md");
    let mut doc = session.load_document(&post_path).unwrap();
    if let Some(b) = doc.blocks.iter_mut().find(|b| b.r#type == "paragraph") {
        b.text = Some("Edited paragraph.".into());
    }
    session.save(&doc).unwrap();
    let doc2 = session.load_document(&post_path).unwrap();
    assert!(doc2
        .blocks
        .iter()
        .any(|b| b.text.as_deref() == Some("Edited paragraph.")));
}

#[test]
fn serve_status_is_listening_after_open() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let final_status = wait_until(
        std::time::Duration::from_secs(5),
        || session.serve_status(),
        |s| !matches!(s, ServeStatus::Starting),
    );
    assert!(matches!(final_status, ServeStatus::Listening { .. }));
}

#[test]
fn serve_responds_to_get() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    let final_status = wait_until(
        std::time::Duration::from_secs(5),
        || session.serve_status(),
        |s| !matches!(s, ServeStatus::Starting),
    );
    let url = match final_status {
        ServeStatus::Listening { url } => url,
        ServeStatus::Unavailable { .. } | ServeStatus::Starting => {
            panic!("expected serve to be listening")
        }
    };
    let addr = url.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(stream, "GET / HTTP/1.0\r\nHost: {addr}\r\n\r\n").unwrap();
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.starts_with("HTTP/1.1"));
}

#[test]
fn import_image_copies_file_into_src_images() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    // Create a source image file.
    let src_img = dir.path().join("src").join("photo.png");
    fs::write(&src_img, b"\x89PNG\r\n\x1a\nfake_png_bytes").unwrap();
    let web = session.import_image(&src_img).unwrap();
    assert!(web.starts_with("/images/"));
    let filename = web.trim_start_matches("/images/");
    assert!(dir
        .path()
        .join("src")
        .join("images")
        .join(filename)
        .exists());
}

#[test]
fn workspace_summary_has_slugs_and_tags() {
    let dir = make_workspace();
    let p = dir.path();
    // Page with explicit front-matter slug.
    fs::write(
        p.join("src/pages/about.md"),
        "---\ntitle: About Me\nslug: about\n---\n\nHi.\n",
    )
    .unwrap();
    // Page without slug — the file stem is the slug.
    fs::write(
        p.join("src/pages/contact.md"),
        "---\ntitle: Contact\n---\n\nHi.\n",
    )
    .unwrap();
    // Posts with overlapping tags to prove sort + de-dup.
    fs::write(
        p.join("src/posts/tagged.md"),
        "---\ntitle: Tagged\ndate: 2026-04-21\ntags: [web, rust]\n---\n\nBody.\n",
    )
    .unwrap();
    fs::write(
        p.join("src/posts/tagged2.md"),
        "---\ntitle: Tagged Two\ndate: 2026-04-22\ntags: [rust]\n---\n\nBody.\n",
    )
    .unwrap();

    let session = Session::open(p).unwrap();
    let ws = session.workspace();

    let about = ws.pages.iter().find(|d| d.title == "About Me").unwrap();
    assert_eq!(about.slug, "about", "front-matter slug wins");
    let contact = ws.pages.iter().find(|d| d.title == "Contact").unwrap();
    assert_eq!(contact.slug, "contact", "file stem is the fallback slug");
    assert_eq!(ws.tags, vec!["rust".to_string(), "web".to_string()]);
}

#[test]
fn import_image_disambiguates_collisions() {
    let dir = make_workspace();
    let session = Session::open(dir.path()).unwrap();
    // First import → creates photo_a.png.
    let src1 = dir.path().join("src").join("photo_a.png");
    fs::write(&src1, b"\x89PNG\r\n\x1a\nbytes_a").unwrap();
    let web1 = session.import_image(&src1).unwrap();
    assert_eq!(web1, "/images/photo_a.png");
    // Different bytes with the same source name → disambiguate.
    let src2 = dir.path().join("src").join("photo_a.png");
    fs::write(&src2, b"\x89PNG\r\n\x1a\nbytes_b").unwrap();
    let web2 = session.import_image(&src2).unwrap();
    assert_eq!(web2, "/images/photo_a-1.png");
    // Identical bytes to the first import → reuse photo_a.png.
    let src3 = dir.path().join("src").join("photo_a.png");
    fs::write(&src3, b"\x89PNG\r\n\x1a\nbytes_a").unwrap();
    let web3 = session.import_image(&src3).unwrap();
    assert_eq!(web3, "/images/photo_a.png");
    // Verify the file on disk is the original bytes_a.
    let disk = dir.path().join("src").join("images").join("photo_a.png");
    assert_eq!(fs::read(&disk).unwrap(), b"\x89PNG\r\n\x1a\nbytes_a");
}

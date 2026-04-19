use lopress_build::build;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn copy_fixture(name: &str) -> (TempDir, PathBuf) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let dst = TempDir::new().unwrap();
    copy_dir(&src, dst.path());
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let rel = entry.path().strip_prefix(from).unwrap();
        let dst = to.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dst).unwrap();
        } else {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(entry.path(), &dst).unwrap();
        }
    }
}

#[test]
fn minimal_site_builds_expected_files() {
    let (_tmp, root) = copy_fixture("minimal");
    let report = build(&root).unwrap();
    assert!(report.failures.is_empty(), "failures: {failures:?}", failures = report.failures);

    let www = root.join("www");
    assert!(www.join("index.html").exists());
    assert!(www.join("posts/hello/index.html").exists());
    assert!(www.join("about/index.html").exists());
    assert!(www.join("tags/intro/index.html").exists());
    assert!(www.join("feed.xml").exists());
    assert!(www.join("sitemap.xml").exists());
    assert!(www.join("robots.txt").exists());
    assert!(www.join("404.html").exists());
    assert!(www.join("assets/theme.css").exists());

    let index = fs::read_to_string(www.join("index.html")).unwrap();
    assert!(index.contains("Test Site"));
    assert!(index.contains("posts") && index.contains("hello"));

    let hello = fs::read_to_string(www.join("posts/hello/index.html")).unwrap();
    assert!(hello.contains("<h1>Hello</h1>"));
    assert!(hello.contains("<p>First post.</p>"));

    let feed = fs::read_to_string(www.join("feed.xml")).unwrap();
    assert!(feed.contains("<title>Hello</title>"));
    assert!(feed.contains("https://example.com/posts/hello/"));
}

#[test]
fn drafts_are_excluded_from_every_output() {
    let (_tmp, root) = copy_fixture("with-draft");
    let report = build(&root).unwrap();
    let failures = &report.failures;
    assert!(failures.is_empty(), "failures: {failures:?}");

    let www = root.join("www");
    assert!(www.join("posts/done/index.html").exists());
    assert!(
        !www.join("posts/wip/index.html").exists(),
        "draft post was written"
    );

    let feed = fs::read_to_string(www.join("feed.xml")).unwrap();
    assert!(!feed.contains("WIP"), "draft appears in feed");

    let sitemap = fs::read_to_string(www.join("sitemap.xml")).unwrap();
    assert!(!sitemap.contains("wip"), "draft appears in sitemap");

    let index = fs::read_to_string(www.join("index.html")).unwrap();
    assert!(!index.contains("WIP"), "draft appears in index");
}

#[test]
fn plugin_block_renders_with_inner_content_and_asset_is_copied() {
    let (_tmp, root) = copy_fixture("with-plugin");
    let report = build(&root).unwrap();
    let failures = &report.failures;
    assert!(failures.is_empty(), "failures: {failures:?}");

    let www = root.join("www");
    let html = fs::read_to_string(www.join("posts/demo/index.html")).unwrap();
    assert!(html.contains("class=\"callout callout-warning\""));
    assert!(html.contains("Inside"));
    assert!(html.contains("<p>Before."));
    assert!(html.contains("<p>After."));

    assert!(www.join("assets/callout/callout.css").exists());
}

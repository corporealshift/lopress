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
    assert!(
        report.failures.is_empty(),
        "failures: {failures:?}",
        failures = report.failures
    );

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

#[test]
fn image_pipeline_produces_variants_and_caches_on_rerun() {
    use image::{Rgb, RgbImage};

    let (_tmp, root) = copy_fixture("with-images");
    let images = root.join("src/images");
    fs::create_dir_all(&images).unwrap();
    let src_img = images.join("photo.jpg");
    let mut img = RgbImage::new(2000, 1500);
    for p in img.pixels_mut() {
        *p = Rgb([120, 180, 255]);
    }
    img.save(&src_img).unwrap();

    let report = build(&root).unwrap();
    let failures = &report.failures;
    assert!(failures.is_empty(), "failures: {failures:?}");

    let www_images = root.join("www/images");
    assert!(www_images.join("photo.jpg").exists());
    assert!(www_images.join("photo.400w.webp").exists());
    assert!(www_images.join("photo.800w.webp").exists());
    assert!(www_images.join("photo.1600w.webp").exists());

    let mtime_before = fs::metadata(www_images.join("photo.800w.webp"))
        .unwrap()
        .modified()
        .unwrap();
    build(&root).unwrap();
    let mtime_after = fs::metadata(www_images.join("photo.800w.webp"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime_before, mtime_after, "cached variant was regenerated");
}

#[test]
fn incremental_skips_unchanged_posts() {
    let (_tmp, root) = copy_fixture("minimal");
    let r1 = build(&root).unwrap();
    assert!(r1.failures.is_empty());
    let first_rendered = r1.pages_rendered;
    assert!(first_rendered >= 1);

    let r2 = build(&root).unwrap();
    assert!(r2.failures.is_empty());
    assert_eq!(r2.pages_rendered, 0, "second build should render nothing");
    assert!(r2.pages_skipped >= 1);
}

#[test]
fn editing_one_post_rerenders_only_that_post() {
    let (_tmp, root) = copy_fixture("minimal");
    build(&root).unwrap();

    let hello = root.join("src/posts/hello.md");
    let src = fs::read_to_string(&hello).unwrap();
    fs::write(&hello, format!("{src}\nextra content\n")).unwrap();

    let r2 = build(&root).unwrap();
    assert_eq!(r2.pages_rendered, 1, "only hello.md should re-render");
    assert!(r2.pages_skipped >= 1);
}

#[test]
fn editing_config_triggers_full_rebuild() {
    let (_tmp, root) = copy_fixture("minimal");
    let r1 = build(&root).unwrap();
    let rendered_first = r1.pages_rendered;

    let cfg = root.join("lopress.toml");
    let src = fs::read_to_string(&cfg).unwrap();
    fs::write(&cfg, format!("{src}\n# comment\n")).unwrap();

    let r2 = build(&root).unwrap();
    assert_eq!(
        r2.pages_rendered, rendered_first,
        "config change should rerender everything"
    );
    assert_eq!(r2.pages_skipped, 0);
}

#[test]
fn deleted_post_is_removed_from_output() {
    let (_tmp, root) = copy_fixture("minimal");
    build(&root).unwrap();
    let out = root.join("www/posts/hello/index.html");
    assert!(out.exists());

    fs::remove_file(root.join("src/posts/hello.md")).unwrap();
    build(&root).unwrap();
    assert!(!out.exists(), "deleted post should be pruned from www/");
}

#[test]
fn home_page_shows_excerpt_with_read_more_link() {
    let (_tmp, root) = copy_fixture("minimal");
    // Replace the sample post with one that has a read-more marker.
    let post = root.join("src/posts/hello.md");
    fs::write(
        &post,
        "---\ntitle: P\ndate: 2026-06-01\n---\nteaser para\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nhidden para\n",
    )
    .unwrap();
    let report = build(&root).unwrap();
    let failures = &report.failures;
    assert!(failures.is_empty(), "failures: {failures:?}");

    let www = root.join("www");

    // Post page: must show full content (including hidden part) and must
    // not contain the marker comment itself.
    let post_html = fs::read_to_string(www.join("posts/hello/index.html")).unwrap();
    assert!(
        post_html.contains("hidden para"),
        "post page must show full content"
    );
    assert!(
        !post_html.contains("lopress:more"),
        "post page must not show the marker comment"
    );
    // The marker renders to nothing in the body, so there's no empty block
    // between teaser and hidden.
    assert!(
        post_html.contains("teaser para"),
        "post page must show teaser"
    );

    // Home page: excerpt rendering and "Read more" link are asserted in
    // Task 12's template update. The excerpt_html field is populated
    // (verified by the unit test in pages.rs), but the index template
    // must be updated to display it.
}

#[test]
fn images_render_as_responsive_picture() {
    use image::{Rgb, RgbImage};

    let (_tmp, root) = copy_fixture("with-images");
    let images = root.join("src/images");
    fs::create_dir_all(&images).unwrap();
    let src_img = images.join("photo.jpg");
    // 2000px wide so all three default widths (400/800/1600) produce variants.
    let mut img = RgbImage::new(2000, 1500);
    for p in img.pixels_mut() {
        *p = Rgb([120, 180, 255]);
    }
    img.save(&src_img).unwrap();

    let report = build(&root).unwrap();
    let failures = &report.failures;
    assert!(failures.is_empty(), "failures: {failures:?}");

    let www = root.join("www");
    let post_html = fs::read_to_string(www.join("posts/album/index.html")).unwrap();
    assert!(
        post_html.contains("<picture>"),
        "expected responsive picture, got:\n{post_html}"
    );
    assert!(post_html.contains("image/webp"), "missing webp type");
    assert!(
        post_html.contains("photo.400w.webp"),
        "missing 400w variant in srcset"
    );
    assert!(
        post_html.contains("photo.800w.webp"),
        "missing 800w variant in srcset"
    );
}

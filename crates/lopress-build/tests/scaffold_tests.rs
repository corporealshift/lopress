#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Tests for `lopress new` site scaffolding.

use lopress_build::scaffold::new_site;
use lopress_build::site::Workspace;

#[test]
fn new_site_writes_a_loadable_config_with_title_and_base_url() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("my-site");

    new_site(&dir, "My Site", "https://example.com").unwrap();

    let ws = Workspace::load(&dir).expect("scaffolded site must load as a workspace");
    assert_eq!(ws.config.site.title, "My Site");
    assert_eq!(ws.config.site.base_url, "https://example.com");
}

#[test]
fn new_site_creates_posts_dir_with_a_sample_post() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("s");

    new_site(&dir, "T", "http://localhost:8080").unwrap();

    let posts_dir = dir.join("src").join("posts");
    assert!(posts_dir.is_dir(), "src/posts must exist");
    let md_count = std::fs::read_dir(&posts_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .count();
    assert!(
        md_count >= 1,
        "expected at least one sample post, found {md_count}"
    );
}

#[test]
fn scaffolded_site_builds_cleanly() {
    // The "buildable starter" promise: a freshly scaffolded site produces a
    // static build with no setup beyond `new`.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("site");
    new_site(&dir, "Buildable", "https://example.com").unwrap();

    lopress_build::build::build(&dir).expect("a scaffolded site must build cleanly");
    assert!(
        dir.join("www").is_dir(),
        "build must produce a www/ directory"
    );
}

#[test]
fn new_site_refuses_to_scaffold_into_a_non_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("existing");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("keep.txt"), "do not clobber").unwrap();

    let result = new_site(&dir, "T", "http://localhost:8080");

    assert!(result.is_err(), "must refuse a non-empty target directory");
    assert!(
        dir.join("keep.txt").exists(),
        "pre-existing files must be left untouched"
    );
    assert!(
        !dir.join("lopress.toml").exists(),
        "must not write config into a non-empty dir"
    );
}

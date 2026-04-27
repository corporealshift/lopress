#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
)]

use lopress_core::{parse, serialize};
use lopress_editor::ops;
use std::fs;
use tempfile::TempDir;

fn make_workspace_with_post(content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    fs::write(
        p.join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://x.com\"\n",
    )
    .unwrap();
    for d in ["src/posts", "src/pages", "src/images", "plugins"] {
        fs::create_dir_all(p.join(d)).unwrap();
    }
    let post = p.join("src/posts/test.md");
    fs::write(&post, content).unwrap();
    (dir, post)
}

#[test]
fn edit_paragraph_leaves_opaque_blocks_intact() {
    let content = concat!(
        "---\ntitle: T\ndate: 2026-04-20\n---\n\n",
        "# Heading\n\n",
        "A paragraph.\n\n",
        "<!-- lopress:video {\"src\":\"v.mp4\"} -->\n",
        "<!-- /lopress:video -->\n\n",
        "Another paragraph.\n",
    );
    let (_dir, post) = make_workspace_with_post(content);

    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();

    let para_idx = doc
        .blocks
        .iter()
        .position(|b| b.r#type == "paragraph")
        .unwrap();
    if let Some(b) = doc.blocks.get_mut(para_idx) {
        b.text = Some("Edited paragraph.".into());
    }

    let serialized = serialize(&doc);
    let reparsed = parse(&serialized).unwrap();

    let edited = reparsed.blocks.iter().find(|b| b.r#type == "paragraph");
    assert_eq!(
        edited.and_then(|b| b.text.as_deref()),
        Some("Edited paragraph.")
    );

    assert!(reparsed.blocks.iter().any(|b| b.r#type == "lopress:video"));
    let video = reparsed
        .blocks
        .iter()
        .find(|b| b.r#type == "lopress:video")
        .unwrap();
    assert_eq!(
        video.attrs.get("src").and_then(|v| v.as_str()),
        Some("v.mp4")
    );
}

#[test]
fn split_and_serialize_roundtrips() {
    let content = "---\ntitle: T\n---\n\nhello world\n";
    let (_dir, post) = make_workspace_with_post(content);
    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();

    ops::split_block_at_caret(&mut doc.blocks, 0, 5);
    let s = serialize(&doc);
    let reparsed = parse(&s).unwrap();
    assert_eq!(reparsed.blocks.len(), 2);
    assert_eq!(
        reparsed.blocks.first().and_then(|b| b.text.as_deref()),
        Some("hello")
    );
    // Markdown parsers strip leading whitespace from paragraph text, so " world" → "world"
    assert_eq!(
        reparsed.blocks.get(1).and_then(|b| b.text.as_deref()),
        Some("world")
    );
}

#[test]
fn delete_block_serializes_correctly() {
    let content = "---\ntitle: T\n---\n\nfirst\n\nsecond\n\nthird\n";
    let (_dir, post) = make_workspace_with_post(content);
    let raw = fs::read_to_string(&post).unwrap();
    let mut doc = parse(&raw).unwrap();
    assert_eq!(doc.blocks.len(), 3);

    ops::delete_block(&mut doc.blocks, 1);
    let s = serialize(&doc);
    let reparsed = parse(&s).unwrap();
    assert_eq!(reparsed.blocks.len(), 2);
    assert_eq!(
        reparsed.blocks.first().and_then(|b| b.text.as_deref()),
        Some("first")
    );
    assert_eq!(
        reparsed.blocks.get(1).and_then(|b| b.text.as_deref()),
        Some("third")
    );
}

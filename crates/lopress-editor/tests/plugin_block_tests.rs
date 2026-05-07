#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{BlockBody, BlockKind};
use lopress_plugin::{load_dir, PluginRegistry};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugin-roundtrip")
}

#[test]
fn plugin_block_round_trips_byte_identical() {
    let root = fixture_root();
    let registry = load_dir(&root.join("plugins"), None).unwrap_or_default();
    assert!(
        registry.block("lopress:codehighlight").is_some(),
        "fixture should declare codehighlight"
    );

    let post_path = root.join("src/posts/example.md");
    let raw = std::fs::read_to_string(&post_path).unwrap();
    let core = lopress_core::parse(&raw).unwrap();

    let editor = doc_from_core(&core, &registry);
    // First block is the plugin block; second is a plain paragraph.
    assert_eq!(editor.blocks.len(), 2);
    let first = &editor.blocks[0];
    assert!(first.plugin.is_some(), "plugin block should be detected");
    let meta = first.plugin.as_ref().unwrap();
    assert_eq!(meta.block_type_name, "lopress:codehighlight");
    assert_eq!(
        meta.attrs.get("lang").and_then(|v| v.as_str()),
        Some("rust")
    );
    assert!(matches!(first.kind, BlockKind::Code { .. }));
    if let BlockBody::Code(t) = &first.body {
        assert!(t.contains("println!"));
    } else {
        panic!("expected Code body");
    }

    // Round-trip: editor → core → markdown should equal the original raw text.
    let core_back = doc_to_core(&editor);
    let serialized = lopress_core::serialize(&core_back);
    assert_eq!(serialized, raw, "round-trip should be byte-identical");
}

#[test]
fn unknown_plugin_falls_back_to_opaque_and_round_trips() {
    let raw = "<!-- lopress:unknown {\"k\":\"v\"} -->\nbody\n<!-- /lopress:unknown -->\n";
    let core = lopress_core::parse(raw).unwrap();

    // Empty registry: no plugin matches → opaque path.
    let editor = doc_from_core(&core, &PluginRegistry::default());
    let first = &editor.blocks[0];
    assert!(first.plugin.is_none(), "unknown plugin should not flag plugin meta");
    assert!(matches!(first.kind, BlockKind::Opaque { .. }));

    let core_back = doc_to_core(&editor);
    let serialized = lopress_core::serialize(&core_back);
    assert_eq!(serialized, raw, "opaque round-trip should be byte-identical");
}

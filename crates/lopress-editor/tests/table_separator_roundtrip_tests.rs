//! Editor model round-trip for separator and table blocks.
#![allow(clippy::unwrap_used)]

use lopress_core::parser::parse;
use lopress_core::serializer::serialize;
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_plugin::PluginRegistry;

fn registry() -> PluginRegistry {
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    reg
}

fn roundtrip(src: &str) -> String {
    let core = parse(src).unwrap();
    let editor_doc = doc_from_core(&core, &registry());
    let back = doc_to_core(&editor_doc);
    serialize(&back)
}

#[test]
fn separator_survives_editor_roundtrip() {
    let out = roundtrip("a\n\n---\n\nb\n");
    assert!(out.contains("---\n"), "got: {out}");
    assert!(out.contains("a"));
    assert!(out.contains("b"));
}

#[test]
fn table_survives_editor_roundtrip() {
    let src = "| H1 | H2 |\n| :--- | ---: |\n| a | **b** |\n";
    let out = roundtrip(src);
    // Re-parse the output and confirm it is still one table with the same shape.
    let reparsed = parse(&out).unwrap();
    assert_eq!(reparsed.blocks.len(), 1);
    assert_eq!(reparsed.blocks[0].r#type, "table");
    assert_eq!(
        reparsed.blocks[0].attrs,
        serde_json::json!({ "align": ["left", "right"] })
    );
    assert_eq!(
        reparsed.blocks[0].children[1].children[1].text.as_deref(),
        Some("**b**")
    );
}

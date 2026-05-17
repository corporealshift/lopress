#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use lopress_core::{Block, Document, FrontMatter};
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{BlockBody, BlockKind};
use lopress_plugin::PluginRegistry;

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::default();
    r.load_base_plugins().unwrap();
    r
}

fn list_doc() -> Document {
    Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "list".into(),
            attrs: serde_json::json!({ "ordered": true }),
            children: vec![Block {
                r#type: "list_item".into(),
                attrs: serde_json::json!({}),
                children: vec![Block {
                    r#type: "paragraph".into(),
                    attrs: serde_json::json!({}),
                    children: vec![],
                    text: Some("first".into()),
                }],
                text: None,
            }],
            text: None,
        }],
    }
}

#[test]
fn list_block_gets_plugin_meta_when_base_plugin_registered() {
    let editor_doc = doc_from_core(&list_doc(), &registry());
    let block = &editor_doc.blocks[0];
    assert!(matches!(block.kind, BlockKind::List { ordered: true }));
    assert!(matches!(block.body, BlockBody::List(_)));
    let meta = block.plugin.as_ref().expect("list block has plugin meta");
    assert_eq!(meta.block_type_name, "list");
    assert!(meta.builtin);
    assert_eq!(
        meta.attrs.get("ordered"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[test]
fn list_block_serializes_back_to_core_list_type() {
    let editor_doc = doc_from_core(&list_doc(), &registry());
    let core = doc_to_core(&editor_doc);
    assert_eq!(core.blocks[0].r#type, "list");
    assert_eq!(core.blocks[0].children[0].r#type, "list_item");
}

#[test]
fn list_without_registered_base_plugin_has_no_plugin_meta() {
    let editor_doc = doc_from_core(&list_doc(), &PluginRegistry::default());
    assert!(editor_doc.blocks[0].plugin.is_none());
}

/// End-to-end: a *tight* markdown list (no blank lines between items) must
/// load as an editable `List` block, not fall back to `Opaque`.
#[test]
fn tight_markdown_list_loads_as_an_editable_list_block() {
    let parsed = lopress_core::parse("- one\n- two\n- three\n").unwrap();
    let editor_doc = doc_from_core(&parsed, &registry());
    assert_eq!(editor_doc.blocks.len(), 1);
    let block = &editor_doc.blocks[0];
    assert!(
        matches!(block.kind, BlockKind::List { ordered: false }),
        "tight list should be a List block, got {:?}",
        block.kind
    );
    match &block.body {
        BlockBody::List(items) => {
            let texts: Vec<String> = items
                .iter()
                .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect())
                .collect();
            assert_eq!(texts, vec!["one", "two", "three"]);
        }
        other => panic!("expected List body, got {other:?}"),
    }
}

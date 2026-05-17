#![allow(clippy::unwrap_used)]

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
    assert_eq!(meta.attrs.get("ordered"), Some(&serde_json::Value::Bool(true)));
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

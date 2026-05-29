use crate::model::inline::serialize_inline;
use crate::model::types::{BlockBody, BlockKind, EditorBlock, EditorDoc, PluginMeta};
use lopress_core::{Block, Document};
use serde_json::{json, Map, Value};

/// Convert the editor's working model back into a `lopress_core::Document`.
///
/// Pairs with [`crate::model::from_core::doc_from_core`]; together they form a
/// loss-free round-trip for the supported subset and a verbatim round-trip for
/// `Opaque` blocks (whose original `Block` JSON is stashed inside the body).
pub fn doc_to_core(doc: &EditorDoc) -> Document {
    Document {
        front_matter: doc.front_matter.clone(),
        blocks: doc.blocks.iter().map(block_to_core).collect(),
    }
}

fn block_to_core(b: &EditorBlock) -> Block {
    // Plugin-flagged blocks: a `native` claim serializes as bare native
    // markdown of that core type; otherwise the comment container is used.
    if let Some(meta) = &b.plugin {
        return match &meta.native {
            Some(core_type) => native_block_to_core(b, meta, core_type),
            None => plugin_block_to_core(b, meta),
        };
    }
    match (&b.kind, &b.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => Block {
            r#type: "heading".into(),
            attrs: json!({ "level": level }),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.to_string(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            })
        }
        // kind / body mismatch shouldn't arise from the constructors, but if
        // it does, fall back to an empty paragraph rather than panic.
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    }
}

/// Serialize a `native`-claiming plugin block to its core markdown form.
/// Dispatches on the body shape; `list` and `code` are the native types today.
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => {
            let ordered = meta
                .attrs
                .get("ordered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "ordered": ordered }),
                children: items
                    .iter()
                    .map(|i| Block {
                        r#type: "list_item".into(),
                        attrs: empty_attrs(),
                        children: vec![Block {
                            r#type: "paragraph".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&i.runs)),
                        }],
                        text: None,
                    })
                    .collect(),
                text: None,
            }
        }
        BlockBody::Code(text) => {
            let lang = meta.attrs.get("lang").and_then(Value::as_str).unwrap_or("");
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "lang": lang }),
                children: vec![],
                text: Some(text.clone()),
            }
        }
        // Other body shapes belong to native types not yet migrated; emit a
        // typed block carrying the attrs rather than panicking.
        _ => Block {
            r#type: core_type.to_string(),
            attrs: Value::Object(meta.attrs.clone()),
            children: vec![],
            text: None,
        },
    }
}

fn empty_attrs() -> Value {
    Value::Object(Map::new())
}

/// Reconstruct a plugin-flagged block to core form. The plugin block itself
/// carries only `type` + `attrs` + `children`; the body lives inside a
/// single child block whose shape is dictated by `kind` (this matches the
/// core serializer's `<!-- lopress:foo -->` contract: anything between the
/// markers is parsed as markdown into `children`, and `text` is ignored).
fn plugin_block_to_core(b: &EditorBlock, meta: &PluginMeta) -> Block {
    let attrs = Value::Object(meta.attrs.clone());
    let inner = match (&b.kind, &b.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => Block {
            r#type: "heading".into(),
            attrs: json!({ "level": level }),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::List { ordered }, BlockBody::List(items)) => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": ordered }),
            children: items
                .iter()
                .map(|i| Block {
                    r#type: "list_item".into(),
                    attrs: empty_attrs(),
                    children: vec![Block {
                        r#type: "paragraph".into(),
                        attrs: empty_attrs(),
                        children: vec![],
                        text: Some(serialize_inline(&i.runs)),
                    }],
                    text: None,
                })
                .collect(),
            text: None,
        },
        // Body/kind mismatch: emit empty paragraph child rather than panic.
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    };
    Block {
        r#type: meta.block_type_name.to_string(),
        attrs,
        children: vec![inner],
        text: None,
    }
}

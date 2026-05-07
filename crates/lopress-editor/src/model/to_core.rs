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
    if let Some(meta) = &b.plugin {
        return plugin_block_to_core(b, meta);
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
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
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
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            // The Opaque body holds the original `Block` JSON verbatim.
            // Reconstructing from it gives us byte-identical round-trip.
            // On the impossible failure path, emit a typed empty block rather
            // than panicking.
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.clone(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            })
        }
        // kind / body mismatch shouldn't arise from the constructors, but if it
        // does, fall back to an empty paragraph rather than panic.
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
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
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
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
        r#type: meta.block_type_name.clone(),
        attrs,
        children: vec![inner],
        text: None,
    }
}

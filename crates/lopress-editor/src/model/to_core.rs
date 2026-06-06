use crate::model::descriptor;
use crate::model::inline::serialize_inline;
use crate::model::types::{BlockBody, EditorBlock, EditorDoc, PluginMeta};
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
    // Every block carries PluginMeta. A `native` claim serializes as bare native
    // markdown of that core type; otherwise the comment container is used.
    let meta = &b.plugin;
    // The read-more marker is an empty container: emit no children so it
    // round-trips as a clean `<!-- lopress:more -->`/`<!-- /lopress:more -->`
    // pair (plugin_block_to_core would otherwise emit one inner child).
    if &*meta.block_type_name == "lopress:more" {
        return Block {
            r#type: "lopress:more".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: None,
        };
    }
    // Opaque bodies from unknown/removed plugin types or unconvertible native
    // types carry the original serialized `Block` in their body value.
    // Deserialize it directly so attrs and children round-trip verbatim.
    // Skip `Value::Null` which is used by built-in image blocks (no body).
    if let BlockBody::Opaque(ref value) = b.body {
        if !value.is_null() {
            return serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: meta.block_type_name.to_string(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            });
        }
    }
    return match &meta.native {
        Some(core_type) => native_block_to_core(b, meta, core_type),
        None => plugin_block_to_core(b, meta),
    };
}

/// Serialize a `native`-claiming plugin block to its core markdown form.
/// Dispatches on the body shape; `list` and `code` are the native types today.
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    // The descriptor's editor key drives the inline paragraph/heading/other
    // distinction (replacing the old `core_type == "paragraph"/"heading"` string
    // guards). The body shape comes from matching `&b.body`; each per-shape
    // serializer below is byte-for-byte the existing one. Note: only `paragraph`
    // and `heading` serialize their inline runs as text — other inline-bodied
    // native types (e.g. `separator`, whose body is an empty Inline) must fall to
    // the `_` arm (text: None), not the paragraph arm (which would emit text: "").
    let editor = descriptor::descriptor_for_native(core_type).map(|d| d.editor);
    let is_heading = editor == Some(descriptor::EDITOR_HEADING);
    let is_paragraph = editor == Some(descriptor::EDITOR_PARAGRAPH);
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
        BlockBody::Table(data) => {
            let align: Vec<Value> = data
                .align
                .iter()
                .map(|a| Value::String(a.as_str().to_string()))
                .collect();
            let rows: Vec<Block> = data
                .rows
                .iter()
                .map(|row| Block {
                    r#type: "table_row".into(),
                    attrs: empty_attrs(),
                    children: row
                        .cells
                        .iter()
                        .map(|cell| Block {
                            r#type: "table_cell".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&cell.runs)),
                        })
                        .collect(),
                    text: None,
                })
                .collect();
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "align": align }),
                children: rows,
                text: None,
            }
        }
        BlockBody::Inline(runs) if is_heading => {
            let level = meta
                .attrs
                .get("level")
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "level": level }),
                children: vec![],
                text: Some(serialize_inline(runs)),
            }
        }
        BlockBody::Inline(runs) if is_paragraph => Block {
            r#type: core_type.to_string(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        // Other native types (e.g. separator: an empty Inline body) and other
        // body shapes: emit a bare typed block carrying its attrs, text None.
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
/// single child block whose shape is dictated by the body (this matches the
/// core serializer's `<!-- lopress:foo -->` contract: anything between the
/// markers is parsed as markdown into `children`, and `text` is ignored).
fn plugin_block_to_core(b: &EditorBlock, meta: &PluginMeta) -> Block {
    let attrs = Value::Object(meta.attrs.clone());
    let inner = match &b.body {
        BlockBody::Inline(runs) => {
            // Determine the inner type from the editor key in PluginMeta.
            let inner_type = match meta.editor.as_deref() {
                Some(descriptor::EDITOR_HEADING) => {
                    let level = meta
                        .attrs
                        .get("level")
                        .and_then(Value::as_u64)
                        .and_then(|n| u8::try_from(n).ok())
                        .unwrap_or(1);
                    Block {
                        r#type: "heading".into(),
                        attrs: json!({ "level": level }),
                        children: vec![],
                        text: Some(serialize_inline(runs)),
                    }
                }
                _ => Block {
                    r#type: "paragraph".into(),
                    attrs: empty_attrs(),
                    children: vec![],
                    text: Some(serialize_inline(runs)),
                },
            };
            inner_type
        }
        BlockBody::Code(text) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": meta.attrs.get("lang").and_then(Value::as_str).unwrap_or("") }),
            children: vec![],
            text: Some(text.clone()),
        },
        BlockBody::List(items) => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": false }),
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
        // Body mismatch: emit empty paragraph child rather than panic.
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

#[cfg(test)]
mod more_marker_tests {
    use super::*;
    use crate::model::types::{BlockBody, BlockId, EditorBlock, PluginMeta};
    use std::rc::Rc;

    fn marker_block() -> EditorBlock {
        EditorBlock {
            id: BlockId::new(),
            body: BlockBody::Inline(vec![]),
            plugin: PluginMeta {
                block_type_name: Rc::from("lopress:more"),
                attrs: serde_json::Map::new(),
                attr_decls: Rc::from([]),
                builtin: true,
                editor: Some(Rc::from("more")),
                native: None,
            },
        }
    }

    #[test]
    fn marker_serializes_to_empty_container() {
        let core = block_to_core(&marker_block());
        assert_eq!(core.r#type, "lopress:more");
        assert!(core.children.is_empty(), "marker must have no children");
        assert!(core.text.is_none());
    }
}

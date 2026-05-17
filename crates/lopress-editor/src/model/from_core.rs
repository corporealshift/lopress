use crate::model::inline::parse_inline;
use crate::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, ListItem, PluginMeta,
};
use lopress_core::{Block, Document};
use lopress_plugin::{AttrDecl, BlockDecl, PluginRegistry};
use serde_json::{Map, Value};

/// Convert a `lopress_core::Document` into the editor's working model,
/// consulting `registry` for plugin-declared block types.
///
/// Built-in types (`paragraph`, `heading`, `code_block`, `list`) are mapped
/// directly. For any other type, the registry is consulted: when a matching
/// `BlockDecl` is found the block is rendered with the editor implied by
/// the plugin's `editor` field and `plugin: Some(PluginMeta { ... })` is
/// stamped onto the result so `to_core` can reconstruct it byte-identically.
/// Unknown types — neither built-in nor in the registry — fall through to
/// `Opaque` (so verbatim round-trip survives plugin removal).
pub fn doc_from_core(doc: &Document, registry: &PluginRegistry) -> EditorDoc {
    EditorDoc {
        front_matter: doc.front_matter.clone(),
        blocks: doc
            .blocks
            .iter()
            .map(|b| block_from_core(b, registry))
            .collect(),
    }
}

fn block_from_core(b: &Block, registry: &PluginRegistry) -> EditorBlock {
    match b.r#type.as_str() {
        "paragraph" => {
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::paragraph(parse_inline(text))
        }
        "heading" => {
            let level = b
                .attrs
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = b.text.as_deref().unwrap_or("");
            EditorBlock::heading(level, parse_inline(text))
        }
        "code_block" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = b.text.clone().unwrap_or_default();
            EditorBlock::code(lang, text)
        }
        "list" => list_from_core(b, registry),
        other => match registry.block(other) {
            Some((_plugin, decl)) => plugin_block_from_core(b, decl),
            None => EditorBlock::opaque(
                other.to_string(),
                serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
            ),
        },
    }
}

/// Build an `EditorBlock` for a plugin-declared type. Picks `kind` + `body`
/// based on `decl.editor`. The body lives in the plugin block's *children*
/// (this is how the core serializer round-trips `<!-- lopress:foo -->` blocks
/// — anything between the comment markers parses into `b.children`).
fn plugin_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let plugin = PluginMeta {
        block_type_name: b.r#type.clone(),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    };

    let editor = decl.editor.as_deref().unwrap_or("paragraph");
    let inner = b.children.first();
    let (kind, body) = match editor {
        "heading" => {
            let level = inner
                .and_then(|c| c.attrs.get("level").and_then(serde_json::Value::as_u64))
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            (
                BlockKind::Heading(level.clamp(1, 6)),
                BlockBody::Inline(parse_inline(text)),
            )
        }
        "code" => {
            let lang = inner
                .and_then(|c| c.attrs.get("lang").and_then(serde_json::Value::as_str))
                .unwrap_or("")
                .to_string();
            let text = inner.and_then(|c| c.text.clone()).unwrap_or_default();
            (BlockKind::Code { lang }, BlockBody::Code(text))
        }
        "list" => {
            let ordered = inner
                .and_then(|c| c.attrs.get("ordered").and_then(serde_json::Value::as_bool))
                .unwrap_or(false);
            let items = inner.map(list_items_from_block).unwrap_or_default();
            (BlockKind::List { ordered }, BlockBody::List(items))
        }
        _ => {
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            (BlockKind::Paragraph, BlockBody::Inline(parse_inline(text)))
        }
    };

    EditorBlock {
        id: BlockId::new(),
        kind,
        body,
        plugin: Some(plugin),
    }
}

fn block_attrs_as_object(v: &serde_json::Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    }
}

fn list_items_from_block(b: &Block) -> Vec<ListItem> {
    if b.children.is_empty() {
        return Vec::new();
    }
    b.children
        .iter()
        .map(|item| {
            let text = item
                .children
                .first()
                .and_then(|p| p.text.as_deref())
                .unwrap_or("");
            ListItem {
                id: BlockId::new(),
                runs: parse_inline(text),
            }
        })
        .collect()
}

fn list_from_core(b: &Block, registry: &PluginRegistry) -> EditorBlock {
    let ordered = b
        .attrs
        .get("ordered")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // A list is convertible only if every list_item child contains exactly one
    // paragraph child with no further nesting. Otherwise the whole list becomes
    // Opaque so its structure round-trips verbatim.
    let items: Option<Vec<ListItem>> = if b.children.is_empty() {
        None
    } else {
        b.children
            .iter()
            .map(|item| {
                if item.r#type != "list_item" || item.children.len() != 1 {
                    return None;
                }
                let para = item.children.first()?;
                if para.r#type != "paragraph" || !para.children.is_empty() {
                    return None;
                }
                let text = para.text.as_deref().unwrap_or("");
                Some(ListItem {
                    id: BlockId::new(),
                    runs: parse_inline(text),
                })
            })
            .collect()
    };

    match items {
        Some(items) => {
            let mut block = EditorBlock::list(ordered, items);
            // When the base list plugin is registered, route the block
            // through the plugin block view by stamping `PluginMeta`.
            // `BlockKind::List` is retained for serialization (see to_core).
            block.plugin = list_plugin_meta(registry, ordered);
            block
        }
        None => EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

/// Build `PluginMeta` for a list block from the registered base list plugin.
/// Returns `None` when no `"list"` block is registered (e.g. in tests that
/// build a bare registry) so lists degrade to the built-in dispatch.
fn list_plugin_meta(registry: &PluginRegistry, ordered: bool) -> Option<PluginMeta> {
    let (_, decl) = registry.block("list")?;
    let mut attrs = Map::new();
    attrs.insert("ordered".to_string(), Value::Bool(ordered));
    Some(PluginMeta {
        block_type_name: "list".to_string(),
        attrs,
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    })
}

use crate::model::descriptor;
use crate::model::inline::parse_inline;
use crate::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, ListItem, PluginMeta,
};
use lopress_core::{Block, Document};
use lopress_plugin::{BlockDecl, PluginRegistry};
use serde_json::{Map, Value};
use std::rc::Rc;

/// Convert a `lopress_core::Document` into the editor's working model,
/// consulting `registry` for plugin-declared block types.
///
/// Built-in types (`paragraph`, `heading`, `code`, `list`) are mapped
/// directly. For any other type, the registry is consulted: when a matching
/// `BlockDecl` is found the block is rendered with the editor implied by
/// the plugin's `editor` field and `plugin: PluginMeta { ... }` is
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
    match registry.native_block(b.r#type.as_str()) {
        Some((_plugin, decl)) => native_block_from_core(b, decl),
        None => match registry.block(b.r#type.as_str()) {
            Some((_plugin, decl)) => plugin_block_from_core(b, decl),
            None => EditorBlock::opaque(
                b.r#type.clone(),
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
        block_type_name: Rc::from(b.r#type.as_str()),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
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
                .unwrap_or("");
            let text = inner.and_then(|c| c.text.clone()).unwrap_or_default();
            (
                BlockKind::Code {
                    lang: Rc::from(lang),
                },
                BlockBody::Code(text),
            )
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
        plugin,
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

/// Build an `EditorBlock` for a block type that claims a native markdown
/// construct. Dispatches on the editor key's implied body shape. `list` and
/// `code` are the native types migrated so far; any other native editor key
/// is unreachable today and degrades to `Opaque` for a verbatim round-trip.
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let core_type = b.r#type.as_str();
    // Dispatch on the descriptor's editor key — the block's identity. NOTE:
    // `body_shape` is too coarse to dispatch parsers here: `image` shares the
    // `Opaque` shape with unknown/removed blocks, and `separator` shares the
    // `Inline` shape with `paragraph`/`heading`. Each editor key needs its own
    // parser, so the dispatch keys on `editor`, not `body_shape`.
    match descriptor::descriptor_for_native(core_type).map(|d| d.editor) {
        Some(descriptor::EDITOR_LIST) => native_list_from_core(b, decl),
        Some(descriptor::EDITOR_CODE) => native_code_from_core(b, decl),
        Some(descriptor::EDITOR_PARAGRAPH) => native_paragraph_from_core(b, decl),
        Some(descriptor::EDITOR_HEADING) => native_heading_from_core(b, decl),
        Some(descriptor::EDITOR_IMAGE) => native_image_from_core(b, decl),
        Some(descriptor::EDITOR_SEPARATOR) => EditorBlock::separator(),
        Some(descriptor::EDITOR_TABLE) => native_table_from_core(b),
        _ => EditorBlock::opaque(
            core_type.to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

/// Native-image body parser. An image carries no inline/structured body — its
/// `src`/`alt`/`caption` live in `attrs`. Stamps `PluginMeta` (with
/// `editor: "image"`) so the loaded block routes through the image widget
/// (`editor_for("image")`) and serializes back via `to_core`'s native arm.
/// Routing it through the generic `Opaque` path instead would drop the image
/// identity (`plugin` with no editor/native) and render it as a read-only fallback card.
fn native_image_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    EditorBlock {
        id: BlockId::new(),
        kind: BlockKind::Image,
        body: BlockBody::Opaque(Value::Null),
        plugin: PluginMeta {
            block_type_name: Rc::from(decl.name.as_str()),
            attrs: block_attrs_as_object(&b.attrs),
            attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
            builtin: decl.builtin,
            editor: decl.editor.as_deref().map(Rc::from),
            native: decl.native.as_deref().map(Rc::from),
        },
    }
}

/// Native-list body parser. A list is convertible only if every `list_item`
/// child holds exactly one `paragraph` child with no further nesting;
/// otherwise the whole list becomes `Opaque` so its structure round-trips
/// verbatim. Convertible lists are stamped with `PluginMeta` so they route
/// through the plugin view and serialize back via `to_core`'s native branch.
fn native_list_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let ordered = b
        .attrs
        .get("ordered")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

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
            let mut attrs = Map::new();
            attrs.insert("ordered".to_string(), Value::Bool(ordered));
            block.plugin = PluginMeta {
                block_type_name: Rc::from(decl.name.as_str()),
                attrs,
                attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
                builtin: decl.builtin,
                editor: decl.editor.as_deref().map(Rc::from),
                native: decl.native.as_deref().map(Rc::from),
            };
            block
        }
        None => EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

/// Native-paragraph body parser. Reads inline text from `b.text`, parses
/// it into `InlineRun`s, and stamps `PluginMeta` so the block routes
/// through the plugin view and serializes back via `to_core`'s native
/// branch.
fn native_paragraph_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let text = b.text.as_deref().unwrap_or("");
    let mut block = EditorBlock::paragraph(parse_inline(text));
    block.plugin = PluginMeta {
        block_type_name: Rc::from(decl.name.as_str()),
        attrs: serde_json::Map::new(),
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    };
    block
}

/// Native-heading body parser. Reads `level` from `b.attrs["level"]`,
/// parses inline text from `b.text`, stamps `PluginMeta` with `attrs["level"]`
/// mirrored (so the heading widget reads level from attrs), and stamps
/// `PluginMeta` so the block routes through the plugin view.
fn native_heading_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let level = b
        .attrs
        .get("level")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1)
        .clamp(1, 6);
    let text = b.text.as_deref().unwrap_or("");

    let mut block = EditorBlock::heading(level, parse_inline(text));
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "level".to_string(),
        serde_json::Value::Number(serde_json::Number::from(level)),
    );
    block.plugin = PluginMeta {
        block_type_name: Rc::from(decl.name.as_str()),
        attrs,
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    };
    block
}

/// Native-code body parser. Parses `lang` from the block's attrs and `text`
/// from `b.text`, then stamps `PluginMeta` so the block routes through the
/// plugin view (when the editable widget lands in Stage 2) and serializes
/// back via `to_core`'s native branch.
fn native_code_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let lang = b
        .attrs
        .get("lang")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = b.text.clone().unwrap_or_default();

    let mut block = EditorBlock::code(lang.clone(), text);
    let mut attrs = Map::new();
    attrs.insert("lang".to_string(), Value::String(lang));
    block.plugin = PluginMeta {
        block_type_name: Rc::from(decl.name.as_str()),
        attrs,
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    };
    block
}

/// Build a table `EditorBlock` from a core `table` block. A well-formed table
/// has only `table_row` children, each with only `table_cell` children whose
/// content is inline text. A malformed table degrades to `Opaque` so it
/// round-trips verbatim (mirrors `native_list_from_core`).
fn native_table_from_core(b: &Block) -> EditorBlock {
    use crate::model::types::{Align, TableCell, TableData, TableRow};

    let align: Vec<Align> = b
        .attrs
        .get("align")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| Align::from_str_lenient(v.as_str().unwrap_or("none")))
                .collect()
        })
        .unwrap_or_default();

    let rows: Option<Vec<TableRow>> = b
        .children
        .iter()
        .map(|row| {
            if row.r#type != "table_row" {
                return None;
            }
            let cells: Option<Vec<TableCell>> = row
                .children
                .iter()
                .map(|cell| {
                    if cell.r#type != "table_cell" || !cell.children.is_empty() {
                        return None;
                    }
                    Some(TableCell {
                        id: BlockId::new(),
                        runs: parse_inline(cell.text.as_deref().unwrap_or("")),
                    })
                })
                .collect();
            cells.map(|cells| TableRow {
                id: BlockId::new(),
                cells,
            })
        })
        .collect();

    match rows {
        // A table needs at least one row (the header).
        Some(rows) if !rows.is_empty() => EditorBlock::table(TableData { align, rows }),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

use crate::model::inline::parse_inline;
use crate::model::types::{BlockId, EditorBlock, EditorDoc, ListItem};
use lopress_core::{Block, Document};

/// Convert a `lopress_core::Document` into the editor's working model.
///
/// Plugin-aware classification is intentionally **not** wired up here; that
/// happens in Task 17. For now any block whose type isn't one of
/// `paragraph` / `heading` / `code_block` / `list` (or a `list` whose shape we
/// can't faithfully represent) is preserved verbatim as `Opaque`.
pub fn doc_from_core(doc: &Document) -> EditorDoc {
    EditorDoc {
        front_matter: doc.front_matter.clone(),
        blocks: doc.blocks.iter().map(block_from_core).collect(),
    }
}

fn block_from_core(b: &Block) -> EditorBlock {
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
        "list" => list_from_core(b),
        other => EditorBlock::opaque(
            other.to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

fn list_from_core(b: &Block) -> EditorBlock {
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
        Some(items) => EditorBlock::list(ordered, items),
        None => EditorBlock::opaque(
            "list".to_string(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}

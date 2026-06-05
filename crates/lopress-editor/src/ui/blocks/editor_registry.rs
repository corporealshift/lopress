//! Editor registry — data-driven dispatch from a manifest `editor` key to a
//! built-in editor widget.
//!
//! `editor_for(key)` maps an editor key string to an `EditorWidget`. The key
//! comes from a block's `PluginMeta.editor`, which is copied from the plugin
//! manifest — so dispatch is driven by the manifest, not the Rust `BlockKind`
//! enum. Only the `"list"` key is registered in this iteration; paragraph,
//! heading, and code keep their hardcoded arms in `render_body` until they
//! migrate the same way.

use crate::model::types::{BlockBody, EditorBlock};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::{
    code_editor, heading, image, list, paragraph, read_more, separator, table,
};
use floem::{AnyView, IntoView};

/// A built-in editor widget constructor. A plain `fn` pointer so the registry
/// is a simple `match` with no boxing or global state.
pub type EditorWidget = fn(&EditorBlock, &BlockEnv) -> AnyView;

/// Resolve an editor key to its widget. `None` for keys not (yet) registered.
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        "code" => Some(code_editor_widget),
        "paragraph" => Some(paragraph_editor_widget),
        "heading" => Some(heading_editor_widget),
        "more" => Some(read_more::read_more_widget),
        "separator" => Some(separator::separator_widget),
        "image" => Some(image::image_widget),
        "table" => Some(table::table_editor_widget),
        _ => None,
    }
}

/// The `editor = "list"` widget. Adapts the block and env to the list view:
/// pulls items from the block body and reads `ordered` from the manifest-
/// driven `PluginMeta.attrs`, not from the `BlockKind::List` enum.
fn list_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::List(items) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry list: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    let ordered = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("ordered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    list::editable_list_view(items, block.id, ordered, env)
}

/// The `editor = "paragraph"` widget. Extracts runs from the block's
/// `BlockBody::Inline` and calls `paragraph::render_paragraph_editable`.
fn paragraph_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Inline(runs) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry paragraph: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    paragraph::render_paragraph_editable(runs, block.id, env).into_any()
}

/// The `editor = "heading"` widget. Extracts runs from the block's
/// `BlockBody::Inline` and reads `level` from `PluginMeta.attrs["level"]`
/// (mirrored from `BlockKind::Heading(level)`), then calls
/// `heading::render_heading_editable`.
fn heading_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Inline(runs) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry heading: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    let level = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("level"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(1);
    heading::render_heading_editable(level, runs, block.id, env).into_any()
}

/// The `editor = "code"` widget. Extracts `body` from the block's
/// `BlockBody::Code`, reads `lang` from the manifest-driven `PluginMeta.attrs`,
/// and calls `code_editor::editable_code_view`.
fn code_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Code(body) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry code: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any();
    };
    let lang = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("lang"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    code_editor::editable_code_view(body, lang, block.id, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_for_resolves_paragraph_and_heading() {
        assert!(editor_for("paragraph").is_some());
        assert!(editor_for("heading").is_some());
    }

    #[test]
    fn editor_for_resolves_list_and_rejects_unknown() {
        assert!(editor_for("list").is_some());
        assert!(editor_for("code").is_some());
        assert!(editor_for("more").is_some());
        assert!(editor_for("paragraph").is_some());
        assert!(editor_for("bogus").is_none());
    }

    #[test]
    fn editor_for_resolves_image() {
        assert!(editor_for("image").is_some());
    }

    #[test]
    fn editor_for_resolves_separator() {
        assert!(editor_for("separator").is_some());
    }

    #[test]
    fn editor_for_resolves_table() {
        assert!(editor_for("table").is_some());
    }
}

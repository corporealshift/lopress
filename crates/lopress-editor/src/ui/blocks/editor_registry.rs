//! Editor registry — data-driven dispatch from a manifest `editor` key to a
//! built-in editor widget.
//!
//! `editor_for(key)` maps an editor key string to an `EditorWidget`. The key
//! comes from a block's `PluginMeta.editor`, which is copied from the plugin
//! manifest — so dispatch is driven by the manifest, not the Rust `BlockKind`
//! enum. Only the `"list"` key is registered in this iteration; paragraph,
//! heading, and code keep their hardcoded arms in `render_body` until they
//! migrate the same way.

use crate::model::types::{BlockBody, BlockId, EditorBlock, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::ui::blocks::{code_editor, list};
use floem::reactive::RwSignal;
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Everything a built-in editor widget needs to render one block. Built once
/// per block by `render_body` and passed by reference to the widget.
pub struct EditorContext<'a> {
    pub block: &'a EditorBlock,
    pub on_action: ActionSink,
    pub focus_target: RwSignal<Option<BlockId>>,
    pub focus_pub: FocusPublisher,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}

/// A built-in editor widget constructor. A plain `fn` pointer so the registry
/// is a simple `match` with no boxing or global state.
pub type EditorWidget = fn(&EditorContext) -> AnyView;

/// Resolve an editor key to its widget. `None` for keys not (yet) registered.
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        "code" => Some(code_editor_widget),
        _ => None,
    }
}

/// The `editor = "list"` widget. Adapts `EditorContext` to the list view:
/// pulls items from the block body and reads `ordered` from the manifest-
/// driven `PluginMeta.attrs`, not from the `BlockKind::List` enum.
fn list_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::List(items) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
    let ordered = ctx
        .block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("ordered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    list::editable_list_view(
        items,
        ctx.block.id,
        ordered,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
        Rc::clone(&ctx.on_undo),
        Rc::clone(&ctx.on_redo),
    )
}

/// The `editor = "code"` widget. Extracts `body` from the block's
/// `BlockBody::Code`, reads `lang` from the manifest-driven `PluginMeta.attrs`,
/// and calls `code_editor::editable_code_view`.
fn code_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::Code(body) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
    let lang = ctx
        .block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("lang"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    code_editor::editable_code_view(
        body,
        lang,
        ctx.block.id,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
        Rc::clone(&ctx.on_undo),
        Rc::clone(&ctx.on_redo),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_for_resolves_list_and_rejects_unknown() {
        assert!(editor_for("list").is_some());
        assert!(editor_for("code").is_some());
        assert!(editor_for("paragraph").is_none());
        assert!(editor_for("bogus").is_none());
    }
}

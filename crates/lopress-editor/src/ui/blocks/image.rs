//! The image block's editor widget: an inline preview plus editable alt and
//! caption fields. State lives in `PluginMeta.attrs`; edits emit EditAttrs.
//!
//! Per the Floem image-view soft-spot resolution (the pinned Floem 0.2
//! `img()` widget cannot receive raw bytes from `EditorContext` which lacks
//! a workspace / images_dir path), the preview renders as a bordered
//! placeholder showing the filename and alt text — not a real image.

use crate::actions::BlockAction;
use crate::ui::blocks::editor_registry::EditorContext;
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet};
use floem::views::{h_stack, label, text_input, v_stack, Decorators};
use floem::{AnyView, IntoView};
use serde_json::Value;

/// Build the image block's editor widget. Renders a bordered placeholder
/// (filename + alt) plus editable alt and caption fields that commit on
/// `FocusLost` via `BlockAction::EditAttrs`.
pub fn image_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let meta = match ctx.block.plugin.as_ref() {
        Some(m) => m,
        None => {
            return label(|| "(image: missing meta)".to_string())
                .style(|s| s.color(Color::rgb8(180, 60, 60)))
                .into_any()
        }
    };
    let attrs = meta.attrs.clone();
    let src = attrs
        .get("src")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let alt = attrs
        .get("alt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let caption = attrs
        .get("caption")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // PREVIEW: bordered placeholder (filename + alt). The src is a web path
    // `/images/<file>`; loading raw bytes for a real Floem image view is
    // not possible from EditorContext (no workspace/images_dir in scope).
    let preview = build_placeholder_preview(&src, &alt);

    // alt + caption fields: commit on FocusLost via EditAttrs, exactly like
    // attr_text in ui/blocks/plugin.rs.
    let alt_field = attr_field(
        "alt".to_string(),
        alt.clone(),
        attrs.clone(),
        block_id,
        ctx.on_action.clone(),
    );
    let caption_field = attr_field(
        "caption".to_string(),
        caption,
        attrs,
        block_id,
        ctx.on_action.clone(),
    );

    v_stack((
        preview,
        labeled("Alt", alt_field),
        labeled("Caption", caption_field),
    ))
    .style(|s| s.gap(4.).width_full())
    .into_any()
}

/// Build a bordered placeholder view showing the filename and alt text.
fn build_placeholder_preview(src: &str, alt: &str) -> AnyView {
    let filename = src.split('/').next_back().unwrap_or(src).to_string();
    let alt_owned = alt.to_string();
    let lbl = label(move || {
        if alt_owned.is_empty() {
            format!("🖼 {filename}")
        } else {
            format!("🖼 {filename} — {alt_owned}")
        }
    })
    .style(|s| s.font_size(12.));
    h_stack((lbl,))
        .style(|s| {
            s.border(1.)
                .border_color(Color::rgb8(210, 210, 215))
                .border_radius(4.)
                .padding(8.)
                .width_full()
                .background(Color::rgb8(248, 248, 250))
                .items_center()
        })
        .into_any()
}

/// Build a single editable attribute field that commits on FocusLost.
/// Mirrors `attr_text` from `plugin.rs`: the text input is bound to an
/// `RwSignal<String>`; on focus lost the full attrs map is cloned, the
/// key is updated, and `BlockAction::EditAttrs` is emitted.
fn attr_field(
    name: String,
    initial: String,
    attrs: serde_json::Map<String, Value>,
    block_id: crate::model::types::BlockId,
    on_action: crate::ui::blocks::inline_editor::ActionSink,
) -> impl IntoView + 'static {
    let buf: RwSignal<String> = RwSignal::new(initial);
    let name_for_commit = name.clone();
    let on_action_for_commit = on_action;
    text_input(buf)
        .placeholder(name.clone())
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            let s = buf.get_untracked();
            let mut updated = attrs.clone();
            updated.insert(name_for_commit.clone(), Value::String(s));
            on_action_for_commit(BlockAction::EditAttrs {
                block_id,
                new_attrs: Box::new(updated),
            });
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.font_size(12.).padding_horiz(4.).min_width(160.))
}

/// Build a simple label + field row.
fn labeled(label_text: &'static str, field: impl IntoView + 'static) -> impl IntoView + 'static {
    let lbl = label(|| label_text.to_string()).style(|s| {
        s.min_width(70.)
            .color(Color::rgb8(110, 100, 130))
            .font_size(12.)
    });
    h_stack((lbl, field)).style(|s| s.gap(8.).items_center())
}

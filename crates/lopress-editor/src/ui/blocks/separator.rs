//! The separator block's editor widget: a slim, full-width horizontal rule.
//! It ignores the (empty) body and is focusable on PointerDown so the block
//! can be selected and deleted via the toolbar — mirroring `read_more.rs`.

use crate::ui::blocks::editor_registry::EditorContext;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{empty, Decorators};
use floem::{AnyView, IntoView};

const RULE: Color = Color::rgb8(180, 180, 188);

pub fn separator_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
    empty()
        .style(move |s| s.width_full().height(1.).margin_vert(10.).background(RULE))
        .on_event(EventListener::PointerDown, move |_| {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        })
        .into_any()
}

//! The read-more marker's editor widget: a slim, full-width divider labeled
//! "Read more". It ignores the (empty) body and is focusable on PointerDown so
//! the block can be selected and deleted via the toolbar — mirroring the focus
//! handoff in `fallback.rs`.

use crate::ui::blocks::editor_registry::EditorContext;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{label, Decorators};
use floem::{AnyView, IntoView};

const RULE: Color = Color::rgb8(180, 160, 210);
const FG: Color = Color::rgb8(120, 100, 150);

pub fn read_more_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
    label(|| "— Read more —".to_string())
        .style(move |s| {
            s.width_full()
                .padding_vert(6.)
                .color(FG)
                .font_size(11.)
                .items_center()
                .justify_center()
                .border_top(1.)
                .border_bottom(1.)
                .border_color(RULE)
        })
        .on_event(EventListener::PointerDown, move |_| {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        })
        .into_any()
}

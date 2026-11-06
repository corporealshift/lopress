//! The separator block's editor widget: a slim, full-width horizontal rule.
//! It ignores the (empty) body and is focusable on PointerDown so the block
//! can be selected and deleted via the toolbar — mirroring `read_more.rs`.

use crate::model::types::EditorBlock;
use crate::ui::blocks::env::BlockEnv;
use crate::ui::theme;
use floem::event::{EventListener, EventPropagation};
use floem::reactive::SignalUpdate;
use floem::views::{empty, Decorators};
use floem::{AnyView, IntoView};

pub fn separator_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let focus_pub = env.focus_pub;
    empty()
        .style(move |s| {
            s.width_full()
                .height(1.)
                .margin_vert(theme::SPACE_10)
                .background(theme::RULE)
        })
        .on_event(EventListener::PointerDown, move |_| {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        })
        .into_any()
}

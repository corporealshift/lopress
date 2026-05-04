//! The vertical scrollable editor pane (Task 7, read-only).

use crate::model::types::EditorDoc;
use crate::ui::blocks::block_view;
use floem::views::{scroll, v_stack_from_iter, Decorators};
use floem::IntoView;

/// Render the editor pane: vertical scroll container, max content width 720
/// logical px, centered, with one block view per `EditorBlock`.
pub fn editor_pane(doc: &EditorDoc) -> impl IntoView {
    let blocks: Vec<_> = doc.blocks.iter().map(block_view).collect();
    let column = v_stack_from_iter(blocks).style(|s| {
        s.max_width(720.)
            .width_full()
            .margin_horiz(floem::unit::PxPctAuto::Auto)
            .padding(24.)
    });
    scroll(column).style(|s| s.width_full().height_full())
}

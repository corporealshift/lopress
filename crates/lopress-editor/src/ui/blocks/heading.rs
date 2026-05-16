//! Heading rendering. Levels 1..=6 map to 32 / 26 / 22 / 18 / 16 / 14 logical
//! pixels and render semibold. The editable path wraps the inline-runs
//! editor at the appropriate font size; the read-only path is preserved for
//! callers that don't yet need editing.

use crate::model::types::{BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::inline_editor::{
    build_block_editor, editable_inline, ActionSink, FocusPublisher,
};
use crate::ui::blocks::paragraph::render_runs_with_size;
use floem::reactive::{RwSignal, Scope};
use floem::views::{container, Decorators};
use floem::IntoView;
use std::rc::Rc;

fn font_size_for(level: u8) -> f32 {
    match level {
        1 => 32.0,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

/// Editable heading: inline-runs editor at the level's font size, semibold.
pub fn render_heading_editable(
    level: u8,
    runs: &[InlineRun],
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> impl IntoView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, font_size_for(level) as usize);
    // The inner editor carries a rigid `height`; the heading's vertical
    // padding goes on an outer container so it cannot squeeze the editor's
    // content box and let the text overflow into the adjacent block.
    container(editable_inline(
        state,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        false,
        on_undo,
        on_redo,
    ))
    .style(|s| s.width_full().padding_top(16.).padding_bottom(8.))
}

/// Read-only heading rendering, kept for any non-editable surfaces.
pub fn render_heading(level: u8, runs: &[InlineRun]) -> impl IntoView {
    render_runs_with_size(runs, font_size_for(level), true)
        .style(|s| s.padding_top(16.).padding_bottom(8.))
}

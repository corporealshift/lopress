//! Heading rendering. Levels 1..=6 map to 32 / 26 / 22 / 18 / 16 / 14 logical
//! pixels and render semibold. The editable path wraps the inline-runs
//! editor at the appropriate font size; the read-only path is preserved for
//! callers that don't yet need editing.

use crate::model::types::{BlockId, InlineRun};
use crate::ui::blocks::inline_editor::{
    editable_inline, ActionSink, FocusPublisher, LocalSelection,
};
use crate::ui::blocks::paragraph::render_runs_with_size;
use floem::reactive::RwSignal;
use floem::views::Decorators;
use floem::IntoView;

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
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
) -> impl IntoView {
    editable_inline(
        runs,
        selection,
        font_size_for(level),
        true,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        false,
    )
    .style(|s| s.padding_top(16.).padding_bottom(8.))
}

/// Read-only heading rendering, kept for any non-editable surfaces.
pub fn render_heading(level: u8, runs: &[InlineRun]) -> impl IntoView {
    render_runs_with_size(runs, font_size_for(level), true)
        .style(|s| s.padding_top(16.).padding_bottom(8.))
}

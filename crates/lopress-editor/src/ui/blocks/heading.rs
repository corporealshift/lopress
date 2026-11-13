//! Heading rendering. Levels 1..=6 map to the heading roles in `ui/theme.rs`.
//! The editable path wraps the inline-runs editor in the level's role; the
//! read-only path is preserved for callers that don't yet need editing.

use crate::model::types::{BlockId, InlineRun};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::inline_editor::{build_block_editor, editable_inline};
use crate::ui::blocks::paragraph::render_runs;
use crate::ui::theme::heading_role;
use floem::reactive::Scope;
use floem::views::{container, Decorators};
use floem::IntoView;

/// Editable heading: inline-runs editor in the level's role.
pub fn render_heading_editable(
    level: u8,
    runs: &[InlineRun],
    block_id: BlockId,
    env: &BlockEnv,
) -> impl IntoView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, heading_role(level));
    // The inner editor carries a rigid `height`; the heading's vertical
    // padding goes on an outer container so it cannot squeeze the editor's
    // content box and let the text overflow into the adjacent block.
    container(editable_inline(state, block_id, env, false))
        .style(|s| s.width_full().padding_top(16.).padding_bottom(8.))
}

/// Read-only heading rendering, kept for any non-editable surfaces.
pub fn render_heading(level: u8, runs: &[InlineRun]) -> impl IntoView {
    render_runs(runs, heading_role(level)).style(|s| s.padding_top(16.).padding_bottom(8.))
}

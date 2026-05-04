//! Read-only heading rendering. Levels 1..=6 map to 32 / 26 / 22 / 18 / 16 / 14
//! logical pixels, all rendered semibold.

use crate::model::types::InlineRun;
use crate::ui::blocks::paragraph::render_runs_with_size;
use floem::views::Decorators;
use floem::IntoView;

pub fn render_heading(level: u8, runs: &[InlineRun]) -> impl IntoView {
    let size = match level {
        1 => 32.0_f32,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    };
    render_runs_with_size(runs, size, true).style(|s| s.padding_top(16.).padding_bottom(8.))
}

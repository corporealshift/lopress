//! Paragraph rendering. The editable path (used for `Paragraph` blocks) wraps
//! the inline-runs editor widget. The read-only path (used for list items)
//! lays out one styled `text` element per inline run in a wrapping flex row.

use crate::model::types::{BlockId, InlineRun};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::inline_editor::{
    build_block_editor, editable_inline,
};
use floem::peniko::Color;
use floem::reactive::Scope;
use floem::style::FlexWrap;
use floem::text::Weight;
use floem::views::{container, h_stack_from_iter, text, Decorators};
use floem::IntoView;

/// Body font size (logical px) for paragraphs and list items.
pub const BODY_FONT_SIZE: f32 = 15.0;

/// Monospace font family used for inline `code` runs and code blocks.
pub const MONO_FAMILY: &str = "monospace";

/// Theme link color (read-only — no link interaction yet).
pub const LINK_COLOR: Color = Color::rgb8(70, 110, 200);

/// Editable paragraph: backed by the inline-runs editor widget so the user
/// can click in and type. Used by the block dispatcher in `blocks::mod`.
// `BODY_FONT_SIZE` is a small positive integer-valued constant, so the
// f32->usize conversion is exact.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn render_paragraph_editable(
    runs: &[InlineRun],
    block_id: BlockId,
    env: &BlockEnv,
) -> impl IntoView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, BODY_FONT_SIZE as usize);
    // The inner editor carries a rigid `height`; the block's vertical padding
    // goes on an outer container so it cannot squeeze the editor's content
    // box (which would let the text overflow into the next block).
    container(editable_inline(
        state,
        block_id,
        env,
        true,
    ))
    .style(|s| s.width_full().padding_vert(6.))
}

/// Read-only render of a slice of inline runs as a wrapping flex row.
/// Used inside list items (and other contexts that don't need editing yet).
pub fn render_paragraph(runs: &[InlineRun]) -> impl IntoView {
    render_runs_with_size(runs, BODY_FONT_SIZE, false)
}

/// Render `InlineRun`s at a custom font size, optionally bold.
/// Used by `heading::render_heading` so heading runs share inline styling.
pub fn render_runs_with_size(
    runs: &[InlineRun],
    font_size: f32,
    force_bold: bool,
) -> impl IntoView {
    let spans: Vec<_> = runs
        .iter()
        .map(|r| run_span(r, font_size, force_bold))
        .collect();
    h_stack_from_iter(spans).style(|s| s.flex_wrap(FlexWrap::Wrap))
}

fn run_span(run: &InlineRun, font_size: f32, force_bold: bool) -> impl IntoView {
    let txt = run.text.clone();
    let bold = run.bold || force_bold;
    let italic = run.italic;
    let code = run.code;
    let is_link = run.link.is_some();
    text(txt).style(move |mut s| {
        s = s.font_size(font_size);
        if bold {
            s = s.font_weight(Weight::BOLD);
        }
        if italic {
            s = s.font_style(floem::text::Style::Italic);
        }
        if code {
            s = s
                .font_family(MONO_FAMILY.to_string())
                .background(Color::rgb8(240, 240, 240))
                .padding_horiz(3.)
                .border_radius(3.);
        }
        if is_link {
            s = s.color(LINK_COLOR);
        }
        s
    })
}

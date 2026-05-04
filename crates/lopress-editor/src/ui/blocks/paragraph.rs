//! Read-only paragraph rendering. One styled `text` element per inline run,
//! laid out in a wrapping flex row so spans flow visually like a paragraph.

use crate::model::types::InlineRun;
use floem::peniko::Color;
use floem::style::FlexWrap;
use floem::text::Weight;
use floem::views::{h_stack_from_iter, text, Decorators};
use floem::IntoView;

/// Body font size (logical px) for paragraphs and list items.
pub const BODY_FONT_SIZE: f32 = 15.0;

/// Monospace font family used for inline `code` runs and code blocks.
pub const MONO_FAMILY: &str = "monospace";

/// Theme link color (read-only — no link interaction yet).
pub const LINK_COLOR: Color = Color::rgb8(70, 110, 200);

/// Render a slice of `InlineRun`s as a wrapping row of styled spans.
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
    let spans: Vec<_> = runs.iter().map(|r| run_span(r, font_size, force_bold)).collect();
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

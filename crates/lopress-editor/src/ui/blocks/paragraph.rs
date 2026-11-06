//! Paragraph rendering. The editable path (used for `Paragraph` blocks) wraps
//! the inline-runs editor widget. The read-only path (used for list items)
//! lays out one styled `text` element per inline run in a wrapping flex row.

use crate::model::types::{BlockId, InlineRun};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::inline_editor::{build_block_editor, editable_inline};
use crate::ui::theme::{self, RoleFamily, TextRole};
use floem::reactive::Scope;
use floem::style::FlexWrap;
use floem::text::Weight;
use floem::views::{container, h_stack_from_iter, text, Decorators};
use floem::IntoView;

/// Monospace font family used for inline `code` runs and code blocks.
pub const MONO_FAMILY: &str = "monospace";

/// Editable paragraph: backed by the inline-runs editor widget so the user
/// can click in and type. Used by the block dispatcher in `blocks::mod`.
pub fn render_paragraph_editable(
    runs: &[InlineRun],
    block_id: BlockId,
    env: &BlockEnv,
) -> impl IntoView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, theme::BODY);
    // The inner editor carries a rigid `height`; the block's vertical padding
    // goes on an outer container so it cannot squeeze the editor's content
    // box (which would let the text overflow into the next block).
    container(editable_inline(state, block_id, env, true))
        .style(|s| s.width_full().padding_vert(6.))
}

/// Read-only render of a slice of inline runs as a wrapping flex row.
/// Used inside list items (and other contexts that don't need editing yet).
pub fn render_paragraph(runs: &[InlineRun]) -> impl IntoView {
    render_runs(runs, theme::BODY)
}

/// Render `InlineRun`s in a text role.
/// Used by `heading::render_heading` so heading runs share inline styling.
pub fn render_runs(runs: &[InlineRun], role: TextRole) -> impl IntoView {
    let spans: Vec<_> = runs.iter().map(|r| run_span(r, role)).collect();
    h_stack_from_iter(spans).style(|s| s.flex_wrap(FlexWrap::Wrap))
}

fn run_span(run: &InlineRun, role: TextRole) -> impl IntoView {
    let txt = run.text.clone();
    let bold = run.bold;
    let italic = run.italic;
    let code = run.code;
    let is_link = run.link.is_some();
    text(txt).style(move |mut s| {
        s = s
            .font_size(role.font_size_px())
            .line_height(role.line_height);
        if let RoleFamily::Named(name) = role.family {
            s = s.font_family(name.to_string());
        }
        if bold {
            s = s.font_weight(Weight::BOLD);
        }
        if italic {
            s = s.font_style(floem::text::Style::Italic);
        }
        if code {
            s = s
                .font_family(MONO_FAMILY.to_string())
                .font_size(role.code_run_size_px())
                .background(theme::DRAWER_BG)
                .padding_horiz(3.)
                .border_radius(3.);
        }
        if is_link {
            s = s.color(theme::ACCENT_HOVER);
        }
        s
    })
}

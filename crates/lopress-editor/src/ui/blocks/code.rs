//! Read-only code block: monospace text in a neutral-background frame, with
//! a small language label in the top-right corner.

use crate::ui::blocks::paragraph::MONO_FAMILY;
use floem::peniko::Color;
use floem::views::{empty, h_stack, label, stack, text, Decorators};
use floem::IntoView;

pub fn render_code(lang: &str, body: &str) -> impl IntoView {
    let lang_label_text = lang.to_string();
    let body_text = body.to_string();

    let body_view = text(body_text).style(|s| {
        s.font_family(MONO_FAMILY.to_string())
            .font_size(13.)
            .padding(10.)
            .width_full()
    });

    let lang_label = label(move || lang_label_text.clone()).style(|s| {
        s.color(Color::rgb8(120, 120, 120))
            .font_size(11.)
            .padding_horiz(8.)
            .padding_vert(2.)
    });

    let header = h_stack((empty().style(|s| s.flex_grow(1.0)), lang_label));

    stack((header, body_view)).style(|s| {
        s.flex_col()
            .width_full()
            .background(Color::rgb8(245, 245, 245))
            .border_radius(4.)
            .border(1.)
            .border_color(Color::rgb8(220, 220, 220))
            .margin_vert(8.)
    })
}

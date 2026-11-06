//! The editor's line icons, as inline SVG. floem tints them with the view's
//! text colour, so the colour in the markup doesn't matter.

use floem::peniko::Color;
use floem::views::{empty, svg, Decorators};
use floem::IntoView;

pub const PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M7 2.5v9M2.5 7h9"/></svg>"#;

pub const SLIDERS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round"><path d="M2 4h6M11 4h1M2 10h1M6 10h6"/><circle cx="9.5" cy="4" r="1.5"/><circle cx="4.5" cy="10" r="1.5"/></svg>"#;

pub const CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7"/></svg>"#;

pub const CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 2l3 3-3 3"/></svg>"#;

pub const CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3.5l3 3 3-3"/></svg>"#;

/// A square icon.
pub fn icon(markup: &'static str, size: f32, color: Color) -> impl IntoView {
    svg(markup).style(move |s| s.width(size).height(size).flex_shrink(0.0f32).color(color))
}

/// A small filled status dot.
pub fn dot(color: Color) -> impl IntoView {
    empty().style(move |s| {
        s.width(DOT_SIZE)
            .height(DOT_SIZE)
            .flex_shrink(0.0f32)
            .border_radius(DOT_SIZE / 2.0)
            .background(color)
    })
}

const DOT_SIZE: f32 = 6.0;

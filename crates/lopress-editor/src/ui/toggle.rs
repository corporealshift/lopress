//! The on/off switch used in place of a checkbox.

use floem::views::{container, empty, Decorators};
use floem::IntoView;

use crate::ui::theme;

const WIDTH: f32 = 34.0;
const HEIGHT: f32 = 20.0;
const INSET: f32 = 2.0;
const KNOB: f32 = HEIGHT - 2.0 * INSET;

/// A switch showing `get`. Clicking it, or pressing Space or Enter while it has
/// keyboard focus, calls `on_change` with the new value.
pub fn toggle(
    get: impl Fn() -> bool + Copy + 'static,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoView {
    let knob = empty().style(|s| {
        s.width(KNOB)
            .height(KNOB)
            .border_radius(KNOB / 2.0)
            .background(theme::SURFACE)
    });

    container(knob)
        .keyboard_navigable()
        .on_click_stop(move |_| on_change(!get()))
        .style(move |s| {
            let on = get();
            s.width(WIDTH)
                .height(HEIGHT)
                .flex_shrink(0.0f32)
                .padding(INSET)
                .border_radius(HEIGHT / 2.0)
                .cursor(floem::style::CursorStyle::Pointer)
                .outline_color(theme::ACCENT_TINT)
                .focus_visible(|s| s.outline(3.0))
                .background(if on { theme::ACCENT } else { theme::RULE })
                .apply_if(on, |s| s.justify_end())
        })
}

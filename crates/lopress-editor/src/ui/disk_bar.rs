//! Bar above the footer shown while the open files differ from what was last
//! read or written. In flow rather than an overlay, like the notes drawer.

use crate::ui::editing::save_pipeline::DocStatus;
use crate::ui::theme;
use floem::reactive::{SignalGet, SignalWith};
use floem::views::{button, empty, h_stack, label, Decorators};
use floem::IntoView;
use lopress_gui_host::DiskState;
use std::rc::Rc;

const BAR_HEIGHT: f64 = 32.0;
const BUTTON_HEIGHT: f64 = 24.0;

/// Build the bar. `on_refresh` is what the Refresh button runs.
pub fn disk_bar_view(status: DocStatus, on_refresh: Rc<dyn Fn()>) -> impl IntoView {
    let message = label(move || match status.disk.get() {
        DiskState::Unchanged => String::new(),
        DiskState::Changed => "Changed on disk".to_string(),
        DiskState::Missing => "This file is no longer on disk".to_string(),
    })
    .style(|s| s.font_size(theme::UI_SMALL).color(theme::TEXT));

    let refresh = button(label(|| "Refresh".to_string()))
        .action(move || on_refresh())
        .style(move |s| {
            s.font_size(theme::UI_SMALL)
                .height(BUTTON_HEIGHT)
                .padding_horiz(theme::SPACE_8)
                .padding_vert(0.)
                .color(theme::ACCENT)
                .hover(|s| s.background(theme::SURFACE))
                .focus(|s| s.hover(|s| s.background(theme::SURFACE)))
                .apply_if(status.disk.get() != DiskState::Changed, |s| s.hide())
        });

    let error = label(move || status.refresh_error.get().unwrap_or_default()).style(move |s| {
        s.font_size(theme::UI_SMALL)
            .color(theme::ACCENT)
            .apply_if(status.refresh_error.with(Option::is_none), |s| s.hide())
    });

    h_stack((
        message,
        refresh,
        error,
        empty().style(|s| s.flex_grow(1.0f32)),
    ))
    .style(move |s| {
        s.width_full()
            .height(BAR_HEIGHT)
            .flex_shrink(0.0f32)
            .items_center()
            .gap(theme::SPACE_10)
            .padding_horiz(theme::SPACE_16)
            .background(theme::ACCENT_TINT)
            .border_top(1.)
            .border_color(theme::RULE)
            .apply_if(status.disk.get() == DiskState::Unchanged, |s| s.hide())
    })
}

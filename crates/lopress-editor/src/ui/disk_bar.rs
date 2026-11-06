//! Bar above the footer shown while the open files differ from what was last
//! read or written. In flow rather than an overlay, like the notes drawer.

use crate::ui::editing::save_pipeline::DocStatus;
use floem::peniko::Color;
use floem::reactive::{SignalGet, SignalWith};
use floem::views::{button, empty, h_stack, label, Decorators};
use floem::IntoView;
use lopress_gui_host::DiskState;
use std::rc::Rc;

const BAR_HEIGHT: f64 = 32.0;
const BG: Color = Color::rgb8(255, 244, 214);
const BORDER: Color = Color::rgb8(230, 210, 160);
const FG: Color = Color::rgb8(100, 72, 0);
const ERR_FG: Color = Color::rgb8(170, 40, 40);

/// Build the bar. `on_refresh` is what the Refresh button runs.
pub fn disk_bar_view(status: DocStatus, on_refresh: Rc<dyn Fn()>) -> impl IntoView {
    let message = label(move || match status.disk.get() {
        DiskState::Unchanged => String::new(),
        DiskState::Changed => "Changed on disk".to_string(),
        DiskState::Missing => "This file is no longer on disk".to_string(),
    })
    .style(|s| s.font_size(12.).color(FG));

    let refresh = button(label(|| "Refresh".to_string()))
        .action(move || on_refresh())
        .style(move |s| {
            s.font_size(12.)
                .padding_horiz(8.)
                .padding_vert(0.)
                .apply_if(status.disk.get() != DiskState::Changed, |s| s.hide())
        });

    let error = label(move || status.refresh_error.get().unwrap_or_default()).style(move |s| {
        s.font_size(12.)
            .color(ERR_FG)
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
            .gap(10.)
            .padding_horiz(10.)
            .background(BG)
            .border_top(1.)
            .border_color(BORDER)
            .apply_if(status.disk.get() == DiskState::Unchanged, |s| s.hide())
    })
}

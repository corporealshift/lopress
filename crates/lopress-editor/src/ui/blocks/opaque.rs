//! Read-only opaque-block placeholder card.
//!
//! Renders a neutral card showing the block's `[type_name]` and a collapsed
//! "raw JSON" toggle. The toggle expands to show the pretty-printed JSON
//! payload underneath. No editing is possible.

use crate::ui::blocks::paragraph::MONO_FAMILY;
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{button, dyn_container, empty, label, stack, text, v_stack, Decorators};
use floem::IntoView;
use serde_json::Value;

pub fn render_opaque(type_name: &str, value: &Value) -> impl IntoView {
    let title = format!("[{type_name}]");
    let json_pretty =
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let expanded: RwSignal<bool> = RwSignal::new(false);

    let header = stack((
        label(move || title.clone()).style(|s| {
            s.color(Color::rgb8(90, 90, 90))
                .font_weight(floem::text::Weight::SEMIBOLD)
                .padding(8.)
        }),
        button(label(move || {
            if expanded.get() {
                "Hide raw JSON".to_string()
            } else {
                "Show raw JSON".to_string()
            }
        }))
        .action(move || expanded.update(|e| *e = !*e))
        .style(|s| s.margin_left(8.).padding_horiz(8.).padding_vert(4.)),
    ))
    .style(|s| s.flex_row().items_center());

    let body = dyn_container(
        move || expanded.get(),
        move |open| {
            if open {
                let body = json_pretty.clone();
                text(body)
                    .style(|s| {
                        s.font_family(MONO_FAMILY.to_string())
                            .font_size(12.)
                            .padding(8.)
                            .background(Color::rgb8(245, 245, 245))
                    })
                    .into_any()
            } else {
                empty().into_any()
            }
        },
    );

    v_stack((header, body)).style(|s| {
        s.background(Color::rgb8(252, 252, 252))
            .border(1.)
            .border_color(Color::rgb8(220, 220, 220))
            .border_radius(4.)
            .margin_vert(6.)
            .width_full()
    })
}

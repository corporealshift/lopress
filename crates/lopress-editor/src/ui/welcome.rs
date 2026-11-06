//! Welcome view — shown on launch and when no workspace is open.

use floem::reactive::{RwSignal, SignalGet};
use floem::text::Weight;
use floem::views::{
    button, dyn_container, empty, h_stack, label, v_stack, v_stack_from_iter, Decorators,
};
use floem::IntoView;
use std::path::PathBuf;

use crate::settings::Settings;
use crate::state::WelcomeState;
use crate::ui::icons::{self, icon};
use crate::ui::theme;

const COLUMN_WIDTH: f64 = 340.0;
const TITLE_SIZE: f32 = 52.0;
const OPEN_BUTTON_HEIGHT: f64 = 40.0;
const RECENT_ROW_HEIGHT: f64 = 38.0;
const CHEVRON_SIZE: f32 = 10.0;

/// Build the Welcome view.
///
/// `welcome` and `settings` are reactive signals. `on_open` is called with the
/// chosen workspace path whenever the user picks a folder (either via the file
/// dialog or a recent button).
pub fn welcome_view(
    welcome: RwSignal<WelcomeState>,
    settings: RwSignal<Settings>,
    on_open: impl Fn(PathBuf) + 'static + Clone,
) -> impl IntoView {
    let on_open_btn = on_open.clone();

    // Error banner — shown only when WelcomeState::error is Some.
    let error_view = dyn_container(
        move || welcome.get().error,
        move |maybe_err| match maybe_err {
            Some(msg) => label(move || msg.clone())
                .style(|s| {
                    s.color(theme::ACCENT)
                        .font_size(theme::UI)
                        .padding_top(theme::SPACE_8)
                })
                .into_any(),
            None => empty().into_any(),
        },
    );

    // "Open workspace…" button opens the native folder picker.
    let open_btn = button(label(|| "Open workspace…"))
        .action(move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                on_open_btn(path);
            }
        })
        .style(|s| {
            theme::primary_button(s)
                .width_full()
                .height(OPEN_BUTTON_HEIGHT)
                .padding_vert(0.)
        });

    // One button per recent workspace.
    let recents_view = dyn_container(
        move || settings.get().recents,
        move |recents| {
            let recents = crate::recents::dedup_canonical(&recents);
            if recents.is_empty() {
                return empty().into_any();
            }
            let on_open_recent = on_open.clone();
            let heading = label(|| "Recent").style(|s| {
                s.padding_bottom(theme::SPACE_8)
                    .font_size(theme::UI_SMALL)
                    .font_weight(Weight::SEMIBOLD)
                    .color(theme::TEXT_MUTED)
            });
            let buttons = recents.into_iter().map(|path| {
                let label_text = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("<unknown>")
                    .to_string();
                let on_open_entry = on_open_recent.clone();
                let path_clone = path.clone();
                button(
                    h_stack((
                        label(move || label_text.clone()).style(|s| s.color(theme::TEXT)),
                        empty().style(|s| s.flex_grow(1.0f32)),
                        icon(icons::CHEVRON_RIGHT, CHEVRON_SIZE, theme::TEXT_MUTED),
                    ))
                    .style(|s| s.width_full().items_center()),
                )
                .action(move || {
                    on_open_entry(path_clone.clone());
                })
                .style(|s| {
                    s.width_full()
                        .height(RECENT_ROW_HEIGHT)
                        .padding_vert(0.)
                        .padding_horiz(0.)
                        .border_radius(0.)
                        .border_top(1.)
                        .border_color(theme::RULE_SOFT)
                        .hover(|s| s.background(floem::peniko::Color::TRANSPARENT))
                        .focus(|s| s.hover(|s| s.background(floem::peniko::Color::TRANSPARENT)))
                })
            });

            v_stack((
                heading,
                v_stack_from_iter(buttons).style(|s| s.width_full()),
            ))
            .style(|s| s.width_full())
            .into_any()
        },
    )
    .style(|s| s.width_full());

    let column = v_stack((
        label(|| "lopress").style(|s| {
            s.font_family(theme::DISPLAY_FAMILY.to_string())
                .font_size(TITLE_SIZE)
                .line_height(1.0)
                .color(theme::TEXT)
        }),
        v_stack((open_btn, error_view)).style(|s| s.width_full()),
        recents_view,
    ))
    .style(|s| {
        s.width_full()
            .max_width(COLUMN_WIDTH)
            .gap(theme::SPACE_28)
            .font_size(theme::UI)
    });

    v_stack((column,)).style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .padding_vert(40.)
            .padding_horiz(theme::SPACE_16)
            .background(theme::PAGE_BG)
    })
}

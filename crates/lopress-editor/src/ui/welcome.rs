//! Welcome view — shown on launch and when no workspace is open.

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet};
use floem::views::{button, dyn_container, empty, label, v_stack, v_stack_from_iter, Decorators};
use floem::IntoView;
use std::path::PathBuf;

use crate::settings::Settings;
use crate::state::WelcomeState;

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
                    s.color(Color::rgb8(200, 50, 50))
                        .padding(8.)
                        .margin_bottom(8.)
                })
                .into_any(),
            None => empty().into_any(),
        },
    );

    // "Open workspace…" button opens the native folder picker.
    let open_btn = button(label(|| "Open workspace…")).action(move || {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            on_open_btn(path);
        }
    });

    // One button per recent workspace.
    let recents_view = dyn_container(
        move || settings.get().recents,
        move |recents| {
            let recents = crate::recents::dedup_canonical(&recents);
            let on_open_recent = on_open.clone();
            let buttons = recents.into_iter().map(|path| {
                let label_text = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("<unknown>")
                    .to_string();
                let on_open_entry = on_open_recent.clone();
                let path_clone = path.clone();
                button(label(move || label_text.clone()))
                    .action(move || {
                        on_open_entry(path_clone.clone());
                    })
                    .style(|s| s.margin_top(4.))
            });

            v_stack_from_iter(buttons).into_any()
        },
    );

    v_stack((
        label(|| "lopress").style(|s| s.font_size(32.).margin_bottom(24.)),
        error_view,
        open_btn.style(|s| s.margin_bottom(8.)),
        recents_view,
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .padding(40.)
    })
}

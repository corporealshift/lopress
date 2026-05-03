//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod welcome;

use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{dyn_container, label, Decorators};
use floem::IntoView;

use crate::settings::{self, Settings};
use crate::state::{AppContext, AppState, EditingState, WelcomeState};
use lopress_gui_host::Session;

/// Maximum number of recent workspaces to retain.
const MAX_RECENTS: usize = 5;

/// Build the root view, consuming the loaded `AppContext`.
///
/// `settings_signal` is pre-created by the caller so the window-close handler
/// in `lib.rs` can read the latest settings (recents + geometry) for
/// persistence.
pub fn root_view(ctx: AppContext, settings_signal: RwSignal<Settings>) -> impl IntoView {
    // Initialise the signal with the loaded settings.
    settings_signal.set(ctx.settings);

    let initial_welcome = match ctx.state {
        AppState::Welcome(w) => w,
        AppState::Editing(_) => {
            // Graceful fallback — shouldn't happen at launch.
            WelcomeState::default()
        }
    };

    let welcome_signal: RwSignal<WelcomeState> = RwSignal::new(initial_welcome);

    // Lightweight discriminant that drives dyn_container view switching.
    let state_tag: RwSignal<StateTag> = RwSignal::new(StateTag::Welcome);

    // Workspace name shown on the Editing placeholder screen.
    let editing_name: RwSignal<String> = RwSignal::new(String::new());

    // Callback invoked by the welcome view when the user picks a path.
    let on_open = move |path: std::path::PathBuf| {
        match Session::open(&path) {
            Ok(session) => {
                let ws_name = session.workspace().name.clone();

                // Add to recents, deduplicate, cap, then persist.
                settings_signal.update(|s| {
                    s.recents.retain(|p| p != &path);
                    s.recents.insert(0, path.clone());
                    s.recents.truncate(MAX_RECENTS);
                });
                if let Some(sp) = settings::default_path() {
                    settings_signal.with(|s| {
                        s.save_to(&sp).ok();
                    });
                }

                // Transition to Editing placeholder.
                editing_name.set(ws_name);
                // Session is captured in EditingState but dropped at end of
                // scope for now; full wiring of the editing view comes in a
                // later task.
                let _ = EditingState::new(session);
                state_tag.set(StateTag::Editing);
            }
            Err(e) => {
                welcome_signal.update(|w| {
                    w.error = Some(e.to_string());
                });
            }
        }
    };

    dyn_container(
        move || state_tag.get(),
        move |tag| match tag {
            StateTag::Welcome => {
                welcome::welcome_view(welcome_signal, settings_signal, on_open).into_any()
            }
            StateTag::Editing => {
                let name = editing_name.get();
                label(move || format!("Editing: {name}"))
                    .style(|s| s.width_full().height_full().items_center().justify_center())
                    .into_any()
            }
        },
    )
    .style(|s| s.width_full().height_full())
}

/// Lightweight discriminant so `dyn_container` can derive equality cheaply.
#[derive(Clone, PartialEq)]
enum StateTag {
    Welcome,
    Editing,
}

pub mod actions;
#[cfg(debug_assertions)]
pub(crate) mod ctrl;
pub mod model;
pub mod recents;
pub mod settings;
pub mod state;
pub mod ui;
pub mod undo;

use floem::event::{Event, EventListener};
use floem::kurbo::{Point, Size};
use floem::reactive::{RwSignal, SignalUpdate, SignalWith};
use floem::views::Decorators;
use floem::window::WindowConfig;
use floem::{Application, WindowIdExt};

use settings::Settings;
use state::AppContext;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Floem launch failed: {0}")]
    Launch(String),
}

/// Run the editor app. Returns when the window closes.
///
/// # Errors
/// Returns `AppError::Launch` if the Floem runtime fails to start.
pub fn run() -> Result<(), AppError> {
    // ── Load settings ──────────────────────────────────────────────────────
    let settings = match (settings::default_path(), settings::legacy_recents_path()) {
        (Some(ref s), Some(ref r)) => Settings::load_or_migrate(s, r).unwrap_or_default(),
        (Some(ref s), None) => Settings::load_from(s).unwrap_or_default(),
        _ => Settings::default(),
    };

    // ── Build window config from saved geometry ────────────────────────────
    let ws = &settings.window;
    let win_cfg = WindowConfig::default()
        .title("lopress")
        .size(Size::new(ws.width, ws.height))
        .position(Point::new(ws.x, ws.y));

    let ctx = AppContext::new(settings);

    // Create the settings signal here so both the root view (which updates
    // recents) and the window-close handler (which persists geometry) share it.
    // Floem signals are Send + Sync, so it is safe to capture in both closures.
    let settings_signal: RwSignal<Settings> = RwSignal::new(Settings::default());
    let settings_for_close = settings_signal;

    #[cfg(debug_assertions)]
    let (ctrl_handle, ctrl_action_rx, ctrl_open_rx, ctrl_close_rx) = ctrl::start();

    Application::new()
        .on_event(move |event| {
            // WillTerminate fires just before the event loop exits.
            if let floem::AppEvent::WillTerminate = event {
                if let Some(path) = settings::default_path() {
                    settings_for_close.with(|s| {
                        s.save_to(&path).ok();
                    });
                }
            }
        })
        .window(
            move |window_id| {
                let view = ui::root_view(
                    ctx,
                    settings_signal,
                    #[cfg(debug_assertions)]
                    ctrl_handle,
                    #[cfg(debug_assertions)]
                    ctrl_action_rx,
                    #[cfg(debug_assertions)]
                    ctrl_open_rx,
                    #[cfg(debug_assertions)]
                    ctrl_close_rx,
                );

                // On window close: capture current geometry and persist.
                view.on_event_stop(EventListener::WindowClosed, move |_e: &Event| {
                    let size = window_id
                        .bounds_of_content_on_screen()
                        .map(|r| (r.width(), r.height()));
                    let pos = window_id.position_on_screen_including_frame();

                    settings_signal.update(|s| {
                        if let Some((w, h)) = size {
                            s.window.width = w;
                            s.window.height = h;
                        }
                        if let Some(p) = pos {
                            s.window.x = p.x;
                            s.window.y = p.y;
                        }
                    });

                    if let Some(path) = settings::default_path() {
                        settings_signal.with(|s| {
                            s.save_to(&path).ok();
                        });
                    }
                })
            },
            Some(win_cfg),
        )
        .run();
    Ok(())
}

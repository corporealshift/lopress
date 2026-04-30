use crate::recents;
use crate::state::{AppState, EditingState, WelcomeState};
use crate::ui;
use lopress_gui_host::Session;
use std::path::PathBuf;

pub struct LopressApp {
    state: AppState,
}

impl LopressApp {
    /// Create the app. If `workspace` is provided, open it immediately.
    pub fn new(workspace: Option<PathBuf>) -> Self {
        let state = match workspace {
            Some(path) => open_workspace(path),
            None => AppState::Welcome(WelcomeState::default()),
        };
        Self { state }
    }
}

/// Open a workspace, updating recents on success.
fn open_workspace(path: PathBuf) -> AppState {
    match Session::open(&path) {
        Ok(session) => {
            recents::push(&path);
            AppState::Editing(Box::new(EditingState::new(session)))
        }
        Err(e) => AppState::Welcome(WelcomeState {
            error: Some(e.to_string()),
        }),
    }
}

impl eframe::App for LopressApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Collect the next state transition here; apply after the match so we
        // don't borrow `self.state` and `self` simultaneously.
        let mut next_state: Option<AppState> = None;

        match &mut self.state {
            AppState::Welcome(ws) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let action = ui::welcome::show(ui, &ws.error);
                    match action {
                        ui::welcome::WelcomeAction::OpenPicker => {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                next_state = Some(open_workspace(path));
                            }
                        }
                        ui::welcome::WelcomeAction::OpenPath(path) => {
                            next_state = Some(open_workspace(path));
                        }
                        ui::welcome::WelcomeAction::None => {}
                    }
                });
            }
            AppState::Editing(es) => {
                next_state = show_editing(ctx, es);
            }
        }

        if let Some(state) = next_state {
            self.state = state;
        }
    }
}

/// Render all editing panels. Returns `Some(AppState)` when a workspace
/// transition is requested (open another or close), `None` otherwise.
fn show_editing(ctx: &egui::Context, es: &mut EditingState) -> Option<AppState> {
    let mut next: Option<AppState> = None;

    // Ctrl-S / Cmd-S forced flush
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
            es.flush_current();
        }
    });

    // Debounce check: flush if 500 ms have passed since last edit
    if let Some(doc) = &es.current_doc {
        if doc.dirty {
            if let Some(dirty_at) = doc.dirty_at {
                let elapsed = dirty_at.elapsed().as_millis();
                if elapsed >= 500 {
                    es.flush_current();
                } else {
                    let remaining = 500u64.saturating_sub(elapsed.try_into().unwrap_or(500));
                    ctx.request_repaint_after(std::time::Duration::from_millis(remaining));
                }
            }
        }
    }

    // Rapid repaint while building
    if matches!(
        es.session.build_status(),
        lopress_gui_host::BuildStatus::Building
    ) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    // Menu bar
    egui::TopBottomPanel::top("menu").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Open Workspace…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    es.flush_current();
                    next = Some(open_workspace(path));
                }
            }
            if ui.button("Save").clicked() {
                es.flush_current();
            }
            if ui.button("Close Workspace").clicked() {
                es.flush_current();
                next = Some(AppState::Welcome(WelcomeState::default()));
            }
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    });

    // Status footer
    egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
        ui::footer::show(ui, es);
    });

    // Sidebar
    egui::SidePanel::left("sidebar")
        .default_width(220.0)
        .show(ctx, |ui| {
            let action = ui::sidebar::show(ui, es);
            match action {
                ui::sidebar::SidebarAction::SelectDocument(doc_ref) => {
                    es.open_document(&doc_ref);
                }
                ui::sidebar::SidebarAction::OpenPreview => {
                    let url = es
                        .current_ref
                        .as_ref()
                        .and_then(|r| es.session.preview_url_for(r))
                        .unwrap_or_else(|| match es.session.serve_status() {
                            lopress_gui_host::ServeStatus::Listening { url } => url.clone(),
                            lopress_gui_host::ServeStatus::Unavailable { .. } => String::new(),
                        });
                    if !url.is_empty() {
                        if let Err(e) = open::that(&url) {
                            eprintln!("failed to open browser: {e}");
                        }
                    }
                }
                ui::sidebar::SidebarAction::None => {}
            }
        });

    // Inspector
    egui::SidePanel::right("inspector")
        .default_width(260.0)
        .show(ctx, |ui| {
            ui::inspector::show(ui, es);
        });

    // Block editor (central panel)
    egui::CentralPanel::default().show(ctx, |ui| {
        ui::editor::show(ui, es);
    });

    next
}

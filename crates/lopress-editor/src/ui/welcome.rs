use crate::recents;
use std::path::PathBuf;

pub enum WelcomeAction {
    None,
    OpenPicker,
    OpenPath(PathBuf),
}

pub fn show(ui: &mut egui::Ui, error: &Option<String>) -> WelcomeAction {
    let mut action = WelcomeAction::None;

    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("lopress");
        ui.add_space(24.0);

        if ui.button("Open Workspace…").clicked() {
            action = WelcomeAction::OpenPicker;
        }

        ui.add_space(16.0);

        if let Some(err) = error {
            ui.colored_label(egui::Color32::RED, format!("Error: {err}"));
            ui.add_space(8.0);
        }

        let recents = recents::load();
        if !recents.is_empty() {
            ui.separator();
            ui.add_space(8.0);
            ui.label("Recent workspaces:");
            ui.add_space(4.0);
            for path in &recents {
                let label = path.display().to_string();
                if ui.link(&label).clicked() {
                    action = WelcomeAction::OpenPath(path.clone());
                }
            }
        }
    });

    action
}

use crate::state::EditingState;
use lopress_gui_host::{BuildStatus, ServeStatus};

pub fn show(ui: &mut egui::Ui, es: &EditingState) {
    ui.horizontal(|ui| {
        // Build status (left)
        match es.session.build_status() {
            BuildStatus::Idle => {
                ui.label("—");
            }
            BuildStatus::Building => {
                ui.spinner();
                ui.label("Building…");
            }
            BuildStatus::Ok {
                pages_rendered,
                pages_skipped,
                duration_ms,
            } => {
                ui.label(
                    egui::RichText::new(format!(
                        "Built {pages_rendered} rendered, {pages_skipped} skipped in {duration_ms}ms"
                    ))
                    .weak(),
                );
            }
            BuildStatus::Failed { message } => {
                ui.colored_label(egui::Color32::RED, format!("Build failed: {message}"));
            }
        }

        ui.separator();

        // Save state (middle)
        if let Some(doc) = &es.current_doc {
            if let Some(err) = &doc.last_save_error {
                ui.colored_label(egui::Color32::RED, format!("save failed: {err}"));
            } else if doc.dirty {
                ui.label(egui::RichText::new("unsaved changes").weak());
            } else {
                ui.label(egui::RichText::new("saved").weak());
            }
        }

        ui.separator();

        // Serve URL (right)
        match es.session.serve_status() {
            ServeStatus::Listening { url } => {
                if ui.small_button(url).clicked() {
                    ui.ctx().copy_text(url.clone());
                }
            }
            ServeStatus::Unavailable { reason } => {
                ui.label(egui::RichText::new(format!("serve: {reason}")).weak());
            }
        }
    });
}

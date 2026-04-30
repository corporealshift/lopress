use crate::state::EditingState;
use lopress_gui_host::{DocumentRef, ServeStatus};

pub enum SidebarAction {
    None,
    SelectDocument(DocumentRef),
    OpenPreview,
}

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) -> SidebarAction {
    let mut action = SidebarAction::None;
    let summary = es.session.workspace();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);

        // Posts section
        if !summary.posts.is_empty() {
            ui.label(egui::RichText::new("posts/").weak());
            for doc_ref in &summary.posts {
                if let Some(a) = show_entry(ui, doc_ref, &es.current_ref) {
                    action = a;
                }
            }
            ui.add_space(4.0);
        }

        // Pages section
        if !summary.pages.is_empty() {
            ui.label(egui::RichText::new("pages/").weak());
            for doc_ref in &summary.pages {
                if let Some(a) = show_entry(ui, doc_ref, &es.current_ref) {
                    action = a;
                }
            }
            ui.add_space(8.0);
        }

        ui.separator();
        ui.add_space(4.0);

        // Preview URL button
        let (btn_label, enabled) = match es.session.serve_status() {
            ServeStatus::Listening { url } => (format!("Preview ↗ {url}"), true),
            ServeStatus::Unavailable { reason } => {
                (format!("Preview unavailable: {reason}"), false)
            }
        };
        ui.add_enabled_ui(enabled, |ui| {
            if ui.button(&btn_label).clicked() {
                action = SidebarAction::OpenPreview;
            }
        });
    });

    action
}

fn show_entry(
    ui: &mut egui::Ui,
    doc_ref: &DocumentRef,
    current: &Option<DocumentRef>,
) -> Option<SidebarAction> {
    let is_selected = current.as_ref().is_some_and(|c| c.path == doc_ref.path);
    let mut label = egui::RichText::new(&doc_ref.title);
    if is_selected {
        label = label.strong();
    }
    if doc_ref.has_parse_error {
        label = label.color(egui::Color32::YELLOW);
    }

    let mut result = None;
    ui.horizontal(|ui| {
        if doc_ref.is_draft {
            ui.label(egui::RichText::new("draft").weak().small());
        }
        if doc_ref.has_parse_error {
            ui.label("⚠");
        }
        if ui.selectable_label(is_selected, label).clicked() {
            result = Some(SidebarAction::SelectDocument(doc_ref.clone()));
        }
    });
    result
}

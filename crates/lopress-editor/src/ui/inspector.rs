use crate::state::EditingState;

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) {
    let Some(doc) = &mut es.current_doc else {
        ui.label(egui::RichText::new("No document open.").weak());
        return;
    };

    egui::CollapsingHeader::new("Post")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("fm_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("title");
                    let title = doc.front_matter.title.get_or_insert_with(String::new);
                    if ui.text_edit_singleline(title).changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("slug");
                    let slug = doc.front_matter.slug.get_or_insert_with(String::new);
                    if ui.text_edit_singleline(slug).changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("date");
                    let mut date_str = doc
                        .front_matter
                        .date
                        .map(|d| d.to_string())
                        .unwrap_or_default();
                    if ui.text_edit_singleline(&mut date_str).changed() {
                        if let Ok(parsed) = date_str.parse::<chrono::NaiveDate>() {
                            doc.front_matter.date = Some(parsed);
                            doc.mark_dirty();
                        }
                    }
                    ui.end_row();

                    ui.label("draft");
                    if ui.checkbox(&mut doc.front_matter.draft, "").changed() {
                        doc.mark_dirty();
                    }
                    ui.end_row();

                    ui.label("description");
                    let desc = doc
                        .front_matter
                        .description
                        .get_or_insert_with(String::new);
                    if ui
                        .add(
                            egui::TextEdit::multiline(desc)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        doc.mark_dirty();
                    }
                    ui.end_row();
                });
        });
}

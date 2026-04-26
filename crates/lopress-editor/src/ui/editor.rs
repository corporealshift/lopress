use crate::ops;
use crate::state::EditingState;
use lopress_core::Block;

pub fn show(ui: &mut egui::Ui, es: &mut EditingState) {
    // Parse error fallback
    if let Some(raw) = &es.parse_error_raw {
        ui.label(
            egui::RichText::new(es.parse_error_msg.as_deref().unwrap_or("Parse error"))
                .color(egui::Color32::RED),
        );
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut raw_clone = raw.clone();
            ui.add(
                egui::TextEdit::multiline(&mut raw_clone)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
        });
        return;
    }

    let Some(doc) = &mut es.current_doc else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Select a post from the sidebar.").weak());
        });
        return;
    };

    let mut deferred: Option<BlockAction> = None;
    let mut became_dirty = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        let block_count = doc.blocks.len();
        for idx in 0..block_count {
            let Some(block) = doc.blocks.get_mut(idx) else {
                continue;
            };

            if !ops::is_editable(&block.r#type) {
                let display = placeholder_text(block);
                ui.group(|ui| {
                    ui.label(egui::RichText::new(format!("[{}]", block.r#type)).weak());
                    ui.add(
                        egui::TextEdit::multiline(&mut display.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                    if ui.small_button("×").clicked() {
                        deferred = Some(BlockAction::Delete { idx });
                    }
                });
                continue;
            }

            let block_type = block.r#type.clone();
            let level = block
                .attrs
                .get("level")
                .and_then(|v| v.as_u64())
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let type_lbl = type_label(&block_type, level);

            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(format!("type_{idx}"))
                    .selected_text(type_lbl)
                    .show_ui(ui, |ui| {
                        for opt in ["¶", "H1", "H2", "H3", "H4", "H5", "H6"] {
                            if ui.selectable_label(type_lbl == opt, opt).clicked() {
                                let (nt, lv) = parse_type_label(opt);
                                deferred = Some(BlockAction::ChangeType {
                                    idx,
                                    new_type: nt,
                                    level: lv,
                                });
                            }
                        }
                    });
                if ui.small_button("×").clicked() {
                    deferred = Some(BlockAction::Delete { idx });
                }
            });

            let text = block.text.get_or_insert_with(String::new);
            let font = if block_type == "heading" {
                egui::TextStyle::Heading
            } else {
                egui::TextStyle::Body
            };
            let resp = ui.add(
                egui::TextEdit::multiline(text)
                    .font(font)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1),
            );

            if resp.changed() {
                became_dirty = true;
            }

            if resp.has_focus() {
                ui.input(|i| {
                    if i.key_pressed(egui::Key::Enter) && !i.modifiers.shift {
                        deferred = Some(BlockAction::Split {
                            idx,
                            caret: text.len(),
                        });
                    }
                    if i.key_pressed(egui::Key::Backspace) && text.is_empty() {
                        deferred = Some(BlockAction::MergeWithPrev { idx });
                    }
                });
            }
        }

        ui.add_space(8.0);
        if ui.button("+ Add block").clicked() {
            ops::add_paragraph_at_end(&mut doc.blocks);
            became_dirty = true;
        }
    });

    if became_dirty {
        doc.mark_dirty();
    }
    if let Some(act) = deferred {
        apply_block_action(&mut doc.blocks, act);
        doc.mark_dirty();
    }
}

fn apply_block_action(blocks: &mut Vec<Block>, action: BlockAction) {
    match action {
        BlockAction::Split { idx, caret } => ops::split_block_at_caret(blocks, idx, caret),
        BlockAction::MergeWithPrev { idx } => ops::merge_with_previous(blocks, idx),
        BlockAction::ChangeType {
            idx,
            new_type,
            level,
        } => {
            ops::change_block_type(blocks, idx, new_type, level);
        }
        BlockAction::Delete { idx } => ops::delete_block(blocks, idx),
    }
}

fn type_label(block_type: &str, level: u8) -> &'static str {
    match block_type {
        "heading" => match level {
            1 => "H1",
            2 => "H2",
            3 => "H3",
            4 => "H4",
            5 => "H5",
            _ => "H6",
        },
        _ => "¶",
    }
}

fn parse_type_label(label: &str) -> (&'static str, Option<u8>) {
    match label {
        "H1" => ("heading", Some(1)),
        "H2" => ("heading", Some(2)),
        "H3" => ("heading", Some(3)),
        "H4" => ("heading", Some(4)),
        "H5" => ("heading", Some(5)),
        "H6" => ("heading", Some(6)),
        _ => ("paragraph", None),
    }
}

fn placeholder_text(block: &Block) -> String {
    let mut out = String::new();
    if let Some(t) = &block.text {
        out.push_str(t);
    }
    for c in &block.children {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&placeholder_text(c));
    }
    out
}

enum BlockAction {
    Split {
        idx: usize,
        caret: usize,
    },
    MergeWithPrev {
        idx: usize,
    },
    ChangeType {
        idx: usize,
        new_type: &'static str,
        level: Option<u8>,
    },
    Delete {
        idx: usize,
    },
}

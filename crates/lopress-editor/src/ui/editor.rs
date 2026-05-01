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

            'block_render: {
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
                break 'block_render;
            }

            // --- Code block editor ---
            if block.r#type == "code_block" {
                let mut lang_buf = block
                    .attrs
                    .get("lang")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut lang_changed = false;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Code").weak());
                    ui.label("lang:");
                    if ui.text_edit_singleline(&mut lang_buf).changed() {
                        lang_changed = true;
                    }
                    if ui.small_button("×").clicked() {
                        deferred = Some(BlockAction::Delete { idx });
                    }
                });
                if lang_changed {
                    if let Some(b) = doc.blocks.get_mut(idx) {
                        b.attrs = serde_json::json!({ "lang": lang_buf });
                        became_dirty = true;
                    }
                }
                let Some(block) = doc.blocks.get_mut(idx) else {
                    break 'block_render;
                };
                let text = block.text.get_or_insert_with(String::new);
                let resp = ui.add(
                    egui::TextEdit::multiline(text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3)
                        .code_editor(),
                );
                if resp.changed() {
                    became_dirty = true;
                }
                break 'block_render;
            }

            // --- List editor ---
            if block.r#type == "list" {
                let ordered = block
                    .attrs
                    .get("ordered")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(if ordered { "Ordered list" } else { "Unordered list" })
                            .weak(),
                    );
                    if ui.small_button("×").clicked() {
                        deferred = Some(BlockAction::Delete { idx });
                    }
                });

                let item_count = block.children.len();
                for item_idx in 0..item_count {
                    let prefix = if ordered {
                        format!("{}. ", item_idx + 1)
                    } else {
                        "• ".to_string()
                    };

                    let Some(list) = doc.blocks.get_mut(idx) else {
                        break;
                    };
                    let Some(item) = list.children.get_mut(item_idx) else {
                        break;
                    };
                    if item.children.is_empty() {
                        item.children.push(Block::paragraph(""));
                    }
                    let Some(para) = item.children.get_mut(0) else {
                        break;
                    };
                    let text = para.text.get_or_insert_with(String::new);

                    ui.horizontal(|ui| {
                        ui.label(&prefix);
                        let te_id = egui::Id::new(("list_item", idx, item_idx));
                        let te_output = egui::TextEdit::singleline(text)
                            .id(te_id)
                            .desired_width(f32::INFINITY)
                            .show(ui);
                        if te_output.response.changed() {
                            became_dirty = true;
                        }
                        if te_output.response.has_focus() {
                            ui.input(|i| {
                                if i.key_pressed(egui::Key::Enter) {
                                    deferred = Some(BlockAction::AddListItem { list_idx: idx });
                                }
                                if i.key_pressed(egui::Key::Backspace) && text.is_empty() {
                                    deferred = Some(BlockAction::DeleteListItem {
                                        list_idx: idx,
                                        item_idx,
                                    });
                                }
                            });
                        }
                        if ui.small_button("×").clicked() {
                            deferred = Some(BlockAction::DeleteListItem {
                                list_idx: idx,
                                item_idx,
                            });
                        }
                    });
                }
                if ui.small_button("+ Add item").clicked() {
                    deferred = Some(BlockAction::AddListItem { list_idx: idx });
                }
                break 'block_render;
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
                        for opt in ["¶", "H1", "H2", "H3", "H4", "H5", "H6", "Code", "List"] {
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
            let te_id = egui::Id::new(("block_text", idx));
            let te_output = egui::TextEdit::multiline(text)
                .id(te_id)
                .font(font)
                .desired_width(f32::INFINITY)
                .desired_rows(1)
                .show(ui);
            let resp = te_output.response;

            if resp.changed() {
                became_dirty = true;
            }

            if resp.has_focus() {
                let cursor_byte = te_output
                    .cursor_range
                    .map(|cr| char_to_byte(text, cr.primary.ccursor.index))
                    .unwrap_or_else(|| text.len());
                ui.input(|i| {
                    if i.key_pressed(egui::Key::Enter) && !i.modifiers.shift {
                        deferred = Some(BlockAction::Split {
                            idx,
                            caret: cursor_byte,
                        });
                    }
                    if i.key_pressed(egui::Key::Backspace) && text.is_empty() {
                        deferred = Some(BlockAction::MergeWithPrev { idx });
                    }
                });
            }
            } // end 'block_render

            // Insert-between button
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.menu_button("+", |ui| {
                    let after = idx + 1;
                    let opts: [(&str, InsertBlockType); 7] = [
                        ("Paragraph", InsertBlockType::Paragraph),
                        ("Heading 1", InsertBlockType::Heading(1)),
                        ("Heading 2", InsertBlockType::Heading(2)),
                        ("Heading 3", InsertBlockType::Heading(3)),
                        ("Code block", InsertBlockType::CodeBlock),
                        ("Unordered list", InsertBlockType::UnorderedList),
                        ("Ordered list", InsertBlockType::OrderedList),
                    ];
                    for (label, bt) in opts {
                        if ui.button(label).clicked() {
                            deferred = Some(BlockAction::Insert {
                                idx: after,
                                block_type: bt,
                            });
                            ui.close_menu();
                        }
                    }
                });
            });
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
        BlockAction::AddListItem { list_idx } => ops::add_list_item(blocks, list_idx),
        BlockAction::DeleteListItem { list_idx, item_idx } => {
            ops::delete_list_item(blocks, list_idx, item_idx);
        }
        BlockAction::Insert { idx, block_type } => {
            ops::insert_block_at(blocks, idx, build_insert_block(block_type));
        }
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
        "code_block" => "Code",
        "list" => "List",
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
        "Code" => ("code_block", None),
        "List" => ("list", None),
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
    AddListItem {
        list_idx: usize,
    },
    DeleteListItem {
        list_idx: usize,
        item_idx: usize,
    },
    Insert {
        idx: usize,
        block_type: InsertBlockType,
    },
}

#[derive(Clone, Copy)]
enum InsertBlockType {
    Paragraph,
    Heading(u8),
    CodeBlock,
    UnorderedList,
    OrderedList,
}

fn build_insert_block(block_type: InsertBlockType) -> Block {
    use serde_json::json;
    match block_type {
        InsertBlockType::Paragraph => Block::paragraph(""),
        InsertBlockType::Heading(lvl) => Block::heading(lvl, ""),
        InsertBlockType::CodeBlock => Block {
            r#type: "code_block".into(),
            attrs: json!({ "lang": "" }),
            children: vec![],
            text: Some(String::new()),
        },
        InsertBlockType::UnorderedList => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": false }),
            children: vec![Block {
                r#type: "list_item".into(),
                attrs: json!({}),
                children: vec![Block::paragraph("")],
                text: None,
            }],
            text: None,
        },
        InsertBlockType::OrderedList => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": true }),
            children: vec![Block {
                r#type: "list_item".into(),
                attrs: json!({}),
                children: vec![Block::paragraph("")],
                text: None,
            }],
            text: None,
        },
    }
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

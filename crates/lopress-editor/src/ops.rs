use lopress_core::Block;
use serde_json::Value;

/// Which block types the editor can edit (not read-only placeholders).
pub fn is_editable(block_type: &str) -> bool {
    matches!(block_type, "paragraph" | "heading" | "code_block" | "list")
}

/// Split the block at `idx` at byte offset `caret`. The left half stays at
/// `idx`; the right half is inserted at `idx + 1` with the same block type.
pub fn split_block_at_caret(blocks: &mut Vec<Block>, idx: usize, caret: usize) {
    let Some(block) = blocks.get(idx) else { return };
    let block_type = block.r#type.clone();
    let attrs = block.attrs.clone();
    let text = block.text.clone().unwrap_or_default();

    let left = if caret <= text.len() {
        text.get(..caret).unwrap_or(&text).to_string()
    } else {
        text.clone()
    };
    let right = if caret <= text.len() {
        text.get(caret..).unwrap_or("").to_string()
    } else {
        String::new()
    };

    let right_block = Block {
        r#type: block_type,
        attrs,
        children: vec![],
        text: Some(right),
    };

    if let Some(b) = blocks.get_mut(idx) {
        b.text = Some(left);
    }
    blocks.insert(idx + 1, right_block);
}

/// Merge the block at `idx` into the previous block (text appended; the
/// previous block's type wins). No-op if `idx == 0` or blocks is empty.
pub fn merge_with_previous(blocks: &mut Vec<Block>, idx: usize) {
    if idx == 0 || blocks.is_empty() {
        return;
    }
    let Some(current) = blocks.get(idx).cloned() else {
        return;
    };
    let Some(prev) = blocks.get_mut(idx - 1) else {
        return;
    };
    let prev_text = prev.text.get_or_insert_with(String::new);
    if let Some(cur_text) = &current.text {
        prev_text.push_str(cur_text);
    }
    blocks.remove(idx);
}

/// Change the type of the block at `idx`. Valid targets: `"paragraph"`,
/// `"heading"` (levels 1–6), `"code_block"`, `"list"`.
pub fn change_block_type(blocks: &mut [Block], idx: usize, new_type: &str, level: Option<u8>) {
    let Some(block) = blocks.get_mut(idx) else {
        return;
    };
    match new_type {
        "paragraph" => {
            block.r#type = "paragraph".into();
            block.attrs = Value::Object(serde_json::Map::new());
        }
        "heading" => {
            let lvl = level.unwrap_or(1).clamp(1, 6);
            block.r#type = "heading".into();
            block.attrs = serde_json::json!({ "level": lvl });
        }
        "code_block" => {
            block.r#type = "code_block".into();
            block.attrs = serde_json::json!({ "lang": "" });
        }
        "list" => {
            let text = block.text.take().unwrap_or_default();
            block.r#type = "list".into();
            block.attrs = serde_json::json!({ "ordered": false });
            block.children = vec![Block {
                r#type: "list_item".into(),
                attrs: Value::Object(serde_json::Map::new()),
                children: vec![Block::paragraph(text)],
                text: None,
            }];
        }
        _ => {}
    }
}

/// Insert `block` at position `idx`, shifting later blocks right.
/// If `idx >= blocks.len()`, appends instead.
pub fn insert_block_at(blocks: &mut Vec<Block>, idx: usize, block: Block) {
    if idx >= blocks.len() {
        blocks.push(block);
    } else {
        blocks.insert(idx, block);
    }
}

/// Append an empty paragraph at the end of `blocks`.
pub fn add_paragraph_at_end(blocks: &mut Vec<Block>) {
    blocks.push(Block::paragraph(""));
}

/// Delete the block at `idx`. If removing the last block, replaces it with
/// an empty paragraph so the editor always has at least one block.
pub fn delete_block(blocks: &mut Vec<Block>, idx: usize) {
    if blocks.len() <= idx {
        return;
    }
    blocks.remove(idx);
    if blocks.is_empty() {
        blocks.push(Block::paragraph(""));
    }
}

/// Add an empty item to the list block at `list_idx`.
/// No-op if the block is not a list.
pub fn add_list_item(blocks: &mut [Block], list_idx: usize) {
    let Some(list) = blocks.get_mut(list_idx) else {
        return;
    };
    if list.r#type != "list" {
        return;
    }
    let new_item = Block {
        r#type: "list_item".into(),
        attrs: Value::Object(serde_json::Map::new()),
        children: vec![Block::paragraph("")],
        text: None,
    };
    list.children.push(new_item);
}

/// Remove item at `item_idx` from the list block at `list_idx`.
/// If removing the last item, replaces it with a single empty item so the
/// list always has at least one item.
pub fn delete_list_item(blocks: &mut [Block], list_idx: usize, item_idx: usize) {
    let Some(list) = blocks.get_mut(list_idx) else {
        return;
    };
    if list.r#type != "list" || list.children.is_empty() {
        return;
    }
    if list.children.len() == 1 {
        if let Some(item) = list.children.get_mut(0) {
            if let Some(para) = item.children.get_mut(0) {
                para.text = Some(String::new());
            } else {
                item.children.push(Block::paragraph(""));
            }
        }
        return;
    }
    if item_idx < list.children.len() {
        list.children.remove(item_idx);
    }
}

//! `BlockAction` and the `apply` chokepoint.
//!
//! Every block-tree mutation goes through `apply(doc, action)`. Inline-edit
//! actions (`EditInline`, `EditCode`) are also routed here so the document
//! model stays the single source of truth for persistence — even though
//! per-block widgets keep reactive copies for live editing.

use crate::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};

/// One discrete edit. Each variant maps to one function below.
#[derive(Debug, Clone)]
pub enum BlockAction {
    /// Split the block at `byte_offset` into the block's flat text. The
    /// trailing portion becomes a new block of the same kind directly after
    /// the original.
    Split {
        block_id: BlockId,
        byte_offset: usize,
    },
    /// Merge `block_id` into its predecessor. No-op for the first block.
    MergeWithPrev {
        block_id: BlockId,
    },
    /// Insert `new_block` immediately after `anchor`. If `anchor` is missing,
    /// appends to the end.
    InsertAfter {
        anchor: BlockId,
        new_block: EditorBlock,
    },
    Delete {
        block_id: BlockId,
    },
    /// Move `block_id` to gap `to_index`. Gaps are numbered in pre-removal
    /// coordinates: gap `i` is the slot before block `i`, so `to_index = 0`
    /// drops at the very start and `to_index = blocks.len()` drops at the
    /// very end. Dropping into the gap immediately before or after `block_id`
    /// is a no-op.
    Move {
        block_id: BlockId,
        to_index: usize,
    },
    /// Change the block's kind. Body is converted when reasonable.
    ChangeType {
        block_id: BlockId,
        new_kind: BlockKind,
    },
    /// Replace the inline runs of an `Inline`-bodied block.
    EditInline {
        block_id: BlockId,
        new_runs: Vec<InlineRun>,
    },
    /// Replace the text of a `Code`-bodied block.
    EditCode {
        block_id: BlockId,
        new_text: String,
    },
    /// Replace the runs of a single list item. No-op when the block isn't a
    /// list or the item id is unknown.
    EditListItem {
        block_id: BlockId,
        item_id: BlockId,
        new_runs: Vec<InlineRun>,
    },
    /// Split a list item at `byte_offset` into the item's flat text. The
    /// trailing portion becomes a new `ListItem` directly after it.
    SplitListItem {
        block_id: BlockId,
        item_id: BlockId,
        byte_offset: usize,
    },
    /// Merge a list item into its predecessor item. No-op for the first item.
    MergeListItemWithPrev {
        block_id: BlockId,
        item_id: BlockId,
    },
    /// UI-only action: request the slash command menu for `block_id`. Handled
    /// by the editor pane's action sink (which sets a reactive flag); the
    /// document model is unchanged, so `apply` is a no-op for this variant.
    OpenSlashMenu {
        block_id: BlockId,
    },
    /// Replace the attrs map of `block_id`'s `PluginMeta`. No-op when the
    /// block isn't plugin-flagged. Used by the plugin block's attr form.
    EditAttrs {
        block_id: BlockId,
        new_attrs: serde_json::Map<String, serde_json::Value>,
    },
}

/// Apply one `BlockAction` to the document. Unknown block ids are no-ops.
pub fn apply(doc: &mut EditorDoc, action: BlockAction) {
    match action {
        BlockAction::Split {
            block_id,
            byte_offset,
        } => apply_split(doc, block_id, byte_offset),
        BlockAction::MergeWithPrev { block_id } => apply_merge(doc, block_id),
        BlockAction::InsertAfter { anchor, new_block } => {
            apply_insert_after(doc, anchor, new_block)
        }
        BlockAction::Delete { block_id } => apply_delete(doc, block_id),
        BlockAction::Move { block_id, to_index } => apply_move(doc, block_id, to_index),
        BlockAction::ChangeType { block_id, new_kind } => {
            apply_change_type(doc, block_id, new_kind)
        }
        BlockAction::EditInline { block_id, new_runs } => {
            apply_edit_inline(doc, block_id, new_runs)
        }
        BlockAction::EditCode { block_id, new_text } => apply_edit_code(doc, block_id, new_text),
        BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs,
        } => apply_edit_list_item(doc, block_id, item_id, new_runs),
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset,
        } => apply_split_list_item(doc, block_id, item_id, byte_offset),
        BlockAction::MergeListItemWithPrev { block_id, item_id } => {
            apply_merge_list_item(doc, block_id, item_id)
        }
        // UI-only — handled by the editor pane's action sink, not the model.
        BlockAction::OpenSlashMenu { .. } => {}
        BlockAction::EditAttrs {
            block_id,
            new_attrs,
        } => apply_edit_attrs(doc, block_id, new_attrs),
    }
}

fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if let Some(meta) = block.plugin.as_mut() {
        meta.attrs = new_attrs;
    }
}

fn find_idx(doc: &EditorDoc, id: BlockId) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

fn apply_split(doc: &mut EditorDoc, id: BlockId, byte_offset: usize) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get(idx) else {
        return;
    };
    let kind = block.kind.clone();
    let body = block.body.clone();

    match body {
        BlockBody::Code(text) => {
            let mut new_text = text;
            new_text.insert(byte_offset.min(new_text.len()), '\n');
            apply_edit_code(doc, id, new_text);
        }
        BlockBody::Inline(runs) => {
            let flat: String = runs.iter().map(|r| r.text.as_str()).collect();
            // Snap to a valid UTF-8 char boundary at or after byte_offset.
            let safe_offset = flat
                .char_indices()
                .map(|(b, _)| b)
                .chain(std::iter::once(flat.len()))
                .find(|&b| b >= byte_offset)
                .unwrap_or(flat.len());
            let head = flat.get(..safe_offset).unwrap_or("").to_owned();
            let tail = flat.get(safe_offset..).unwrap_or("").to_owned();
            if let Some(b) = doc.blocks.get_mut(idx) {
                b.body = BlockBody::Inline(vec![InlineRun::plain(head)]);
            }
            let tail_block = match kind {
                BlockKind::Paragraph => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
                BlockKind::Heading(level) => {
                    EditorBlock::heading(level, vec![InlineRun::plain(tail)])
                }
                _ => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
            };
            doc.blocks.insert(idx + 1, tail_block);
        }
        BlockBody::List(items) => {
            // The ctrl API's `Split` command treats a list as the flat text
            // of its items joined by '\n'. Walk cumulative byte offsets to
            // find the item containing `byte_offset` and split it there.
            let mut cumulative = 0usize;
            let mut target: Option<(usize, usize)> = None;
            for (i, it) in items.iter().enumerate() {
                let item_len: usize = it.runs.iter().map(|r| r.text.len()).sum();
                if byte_offset <= cumulative + item_len {
                    target = Some((i, byte_offset - cumulative));
                    break;
                }
                cumulative += item_len + 1; // +1 for the joining '\n'
            }
            let (pos, local) = target.unwrap_or((items.len().saturating_sub(1), 0));
            if let Some(b) = doc.blocks.get_mut(idx) {
                if let BlockBody::List(list) = &mut b.body {
                    split_item_at(list, pos, local);
                }
            }
        }
        BlockBody::Opaque(_) => {}
    }
}

fn apply_merge(doc: &mut EditorDoc, id: BlockId) {
    let Some(idx) = find_idx(doc, id) else { return };
    if idx == 0 {
        return;
    }
    // A list block merges only its *first item* into the previous block; the
    // remaining items stay as a list. Merging the whole list away would
    // silently drop every item — this fires on Backspace at the start of the
    // first list item.
    if matches!(
        doc.blocks.get(idx).map(|b| &b.body),
        Some(BlockBody::List(_))
    ) {
        let first_runs = match doc.blocks.get_mut(idx).map(|b| &mut b.body) {
            Some(BlockBody::List(items)) if !items.is_empty() => Some(items.remove(0).runs),
            _ => None,
        };
        if let Some(runs) = first_runs {
            if let Some(BlockBody::Inline(prev_runs)) =
                doc.blocks.get_mut(idx - 1).map(|b| &mut b.body)
            {
                prev_runs.extend(runs);
            }
        }
        // Drop the list block once it has no items left.
        let empty = matches!(
            doc.blocks.get(idx).map(|b| &b.body),
            Some(BlockBody::List(items)) if items.is_empty()
        );
        if empty {
            doc.blocks.remove(idx);
        }
        return;
    }
    let cur = doc.blocks.remove(idx);
    let Some(prev) = doc.blocks.get_mut(idx - 1) else {
        doc.blocks.insert(idx, cur);
        return;
    };
    if let (BlockBody::Inline(prev_runs), BlockBody::Inline(cur_runs)) = (&mut prev.body, cur.body)
    {
        prev_runs.extend(cur_runs);
    }
}

fn apply_insert_after(doc: &mut EditorDoc, anchor: BlockId, new_block: EditorBlock) {
    let pos = find_idx(doc, anchor)
        .map(|i| i + 1)
        .unwrap_or(doc.blocks.len());
    if pos > doc.blocks.len() {
        doc.blocks.push(new_block);
    } else {
        doc.blocks.insert(pos, new_block);
    }
}

fn apply_delete(doc: &mut EditorDoc, id: BlockId) {
    let Some(idx) = find_idx(doc, id) else { return };
    doc.blocks.remove(idx);
    if doc.blocks.is_empty() {
        doc.blocks
            .push(EditorBlock::paragraph(vec![InlineRun::plain("")]));
    }
}

fn apply_move(doc: &mut EditorDoc, id: BlockId, to_index: usize) {
    let Some(from) = find_idx(doc, id) else {
        return;
    };
    let target_gap = to_index.min(doc.blocks.len());
    // Dropping into the gap immediately before or after self is a no-op.
    if target_gap == from || target_gap == from + 1 {
        return;
    }
    let block = doc.blocks.remove(from);
    let insert_at = if target_gap > from {
        target_gap - 1
    } else {
        target_gap
    };
    doc.blocks.insert(insert_at, block);
}

fn apply_change_type(doc: &mut EditorDoc, id: BlockId, new_kind: BlockKind) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    match (&new_kind, &block.body) {
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_)) => {
            block.kind = new_kind;
        }
        (BlockKind::Code { lang }, BlockBody::Inline(runs)) => {
            let text: String = runs.iter().map(|r| r.text.clone()).collect();
            block.kind = BlockKind::Code { lang: lang.clone() };
            block.body = BlockBody::Code(text);
        }
        (BlockKind::List { ordered }, BlockBody::Inline(runs)) => {
            block.kind = BlockKind::List { ordered: *ordered };
            block.body = BlockBody::List(vec![ListItem {
                id: BlockId::new(),
                runs: runs.clone(),
            }]);
            // A list block must carry list `PluginMeta` to render via the
            // plugin path and serialize natively — `from_core` stamps it for
            // loaded lists; do the same for one created here.
            block.plugin = Some(PluginMeta::list(*ordered));
        }
        _ => {
            block.kind = new_kind;
        }
    }
}

fn apply_edit_inline(doc: &mut EditorDoc, id: BlockId, new_runs: Vec<InlineRun>) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if matches!(block.body, BlockBody::Inline(_)) {
        block.body = BlockBody::Inline(new_runs);
    }
}

fn apply_edit_code(doc: &mut EditorDoc, id: BlockId, new_text: String) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if matches!(block.body, BlockBody::Code(_)) {
        block.body = BlockBody::Code(new_text);
    }
}

fn apply_edit_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    new_runs: Vec<InlineRun>,
) {
    let Some(idx) = find_idx(doc, block_id) else {
        return;
    };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if let BlockBody::List(items) = &mut block.body {
        if let Some(item) = items.iter_mut().find(|it| it.id == item_id) {
            item.runs = new_runs;
        }
    }
}

fn apply_split_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    byte_offset: usize,
) {
    let Some(idx) = find_idx(doc, block_id) else {
        return;
    };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if let BlockBody::List(items) = &mut block.body {
        if let Some(pos) = items.iter().position(|it| it.id == item_id) {
            split_item_at(items, pos, byte_offset);
        }
    }
}

fn apply_merge_list_item(doc: &mut EditorDoc, block_id: BlockId, item_id: BlockId) {
    let Some(idx) = find_idx(doc, block_id) else {
        return;
    };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if let BlockBody::List(items) = &mut block.body {
        let Some(pos) = items.iter().position(|it| it.id == item_id) else {
            return;
        };
        if pos == 0 {
            return;
        }
        let cur = items.remove(pos);
        if let Some(prev) = items.get_mut(pos - 1) {
            prev.runs.extend(cur.runs);
        }
    }
}

/// Split `items[pos]` at `byte_offset` into its flat text. The head stays in
/// place; the tail becomes a fresh `ListItem` inserted at `pos + 1`. Styling
/// is dropped on both sides (the split produces plain runs), matching the
/// behaviour of `apply_split` for paragraphs.
fn split_item_at(items: &mut Vec<ListItem>, pos: usize, byte_offset: usize) {
    let Some(item) = items.get(pos) else { return };
    let flat: String = item.runs.iter().map(|r| r.text.as_str()).collect();
    let safe_offset = flat
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(flat.len()))
        .find(|&b| b >= byte_offset)
        .unwrap_or(flat.len());
    let head = flat.get(..safe_offset).unwrap_or("").to_owned();
    let tail = flat.get(safe_offset..).unwrap_or("").to_owned();
    if let Some(item) = items.get_mut(pos) {
        item.runs = vec![InlineRun::plain(head)];
    }
    items.insert(
        pos + 1,
        ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain(tail)],
        },
    );
}

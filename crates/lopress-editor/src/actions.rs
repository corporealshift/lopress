//! `BlockAction` and the `apply` chokepoint.
//!
//! Every block-tree mutation goes through `apply(doc, action)`. Inline-edit
//! actions (`EditInline`, `EditCode`) are also routed here so the document
//! model stays the single source of truth for persistence — even though
//! per-block widgets keep reactive copies for live editing.

use crate::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem,
};
use crate::selection::{DocPosition, DocSelection};
use crate::ui::blocks::inline_editor::{toggle_inline, Caret, InlineFlag, LocalSelection};

/// One discrete edit. Each variant maps to one function below.
#[derive(Debug, Clone)]
pub enum BlockAction {
    /// Split the block at `(run, offset)`. The trailing portion becomes a
    /// new block of the same kind directly after the original.
    Split {
        block_id: BlockId,
        run: usize,
        offset: usize,
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
    /// UI-only action: request the slash command menu for `block_id`. Handled
    /// by the editor pane's action sink (which sets a reactive flag); the
    /// document model is unchanged, so `apply` is a no-op for this variant.
    OpenSlashMenu {
        block_id: BlockId,
    },
    /// Replace the contents of `selection` with empty text — for cross-block
    /// selections this splices the leading and trailing partial runs and
    /// drops everything in between, merging into a single block of the
    /// leading kind.
    DeleteRange {
        selection: DocSelection,
    },
    /// Toggle `flag` across every Inline-bodied block touched by `selection`.
    /// The toggle is consistent across the range: if every overlapping run
    /// already has the flag, all are cleared; otherwise all are set.
    ToggleInlineRange {
        selection: DocSelection,
        flag: InlineFlag,
    },
    /// Splice `blocks` into the document at `at`. If `at` lands inside an
    /// Inline block, the block is split first.
    PasteBlocks {
        at: DocPosition,
        blocks: Vec<EditorBlock>,
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
            run,
            offset,
        } => apply_split(doc, block_id, run, offset),
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
        // UI-only — handled by the editor pane's action sink, not the model.
        BlockAction::OpenSlashMenu { .. } => {}
        BlockAction::DeleteRange { selection } => apply_delete_range(doc, selection),
        BlockAction::ToggleInlineRange { selection, flag } => {
            apply_toggle_inline_range(doc, selection, flag)
        }
        BlockAction::PasteBlocks { at, blocks } => apply_paste_blocks(doc, at, blocks),
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

fn apply_split(doc: &mut EditorDoc, id: BlockId, run: usize, offset: usize) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get(idx) else {
        return;
    };
    let kind = block.kind.clone();
    let runs = match &block.body {
        BlockBody::Inline(runs) => runs.clone(),
        BlockBody::Code(text) => {
            let mut new_text = text.clone();
            let byte = new_text.len().min(offset);
            new_text.insert(byte, '\n');
            apply_edit_code(doc, id, new_text);
            return;
        }
        _ => return,
    };

    let mut left: Vec<InlineRun> = Vec::new();
    let mut right: Vec<InlineRun> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        if i < run {
            left.push(r.clone());
        } else if i > run {
            right.push(r.clone());
        } else {
            let chars: Vec<char> = r.text.chars().collect();
            let split_at = offset.min(chars.len());
            let left_text: String = chars.iter().take(split_at).collect();
            let right_text: String = chars.iter().skip(split_at).collect();
            if !left_text.is_empty() {
                let mut lr = r.clone();
                lr.text = left_text;
                left.push(lr);
            }
            if !right_text.is_empty() {
                let mut rr = r.clone();
                rr.text = right_text;
                right.push(rr);
            }
        }
    }

    if let Some(b) = doc.blocks.get_mut(idx) {
        b.body = BlockBody::Inline(left);
    }
    let right_block = match kind {
        BlockKind::Paragraph => EditorBlock::paragraph(right),
        BlockKind::Heading(level) => EditorBlock::heading(level, right),
        // Other kinds shouldn't reach Inline split; fall back to paragraph.
        _ => EditorBlock::paragraph(right),
    };
    doc.blocks.insert(idx + 1, right_block);
}

fn apply_merge(doc: &mut EditorDoc, id: BlockId) {
    let Some(idx) = find_idx(doc, id) else { return };
    if idx == 0 {
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

// ── Multi-block operations ───────────────────────────────────────────────────

/// Splice the content covered by `selection` out of the document. The
/// leading block survives, keeps its kind, and absorbs the surviving
/// trailing-block runs. Blocks strictly between the endpoints are removed.
fn apply_delete_range(doc: &mut EditorDoc, selection: DocSelection) {
    let (start, end) = selection.ordered(doc);
    let Some(start_idx) = find_idx(doc, start.block) else {
        return;
    };
    let Some(end_idx) = find_idx(doc, end.block) else {
        return;
    };
    if start_idx == end_idx {
        // Single-block range: splice within the one block via the existing
        // local helper, then commit.
        let Some(block) = doc.blocks.get_mut(start_idx) else {
            return;
        };
        if let BlockBody::Inline(runs) = &mut block.body {
            let local = LocalSelection {
                anchor: Caret {
                    run: start.run,
                    offset: start.offset,
                },
                head: Caret {
                    run: end.run,
                    offset: end.offset,
                },
            };
            crate::ui::blocks::inline_editor::delete_selection(runs, local);
        }
        return;
    }

    // Multi-block: build the merged runs for the leading block.
    let leading_runs = match &doc.blocks[start_idx].body {
        BlockBody::Inline(r) => r.clone(),
        _ => Vec::new(),
    };
    let trailing_runs = match &doc.blocks[end_idx].body {
        BlockBody::Inline(r) => r.clone(),
        _ => Vec::new(),
    };

    let mut merged: Vec<InlineRun> = Vec::new();
    // Prefix from the leading block: runs[..start.run] plus the head of
    // runs[start.run] up to start.offset chars.
    for (i, r) in leading_runs.iter().enumerate() {
        if i < start.run {
            merged.push(r.clone());
        } else if i == start.run {
            let chars: Vec<char> = r.text.chars().collect();
            let take = start.offset.min(chars.len());
            if take > 0 {
                let mut clipped = r.clone();
                clipped.text = chars.iter().take(take).collect();
                merged.push(clipped);
            }
            break;
        }
    }
    // Suffix from the trailing block: tail of runs[end.run] from end.offset
    // chars onward, then runs[end.run + 1..].
    for (i, r) in trailing_runs.iter().enumerate() {
        if i < end.run {
            continue;
        }
        if i == end.run {
            let chars: Vec<char> = r.text.chars().collect();
            let skip = end.offset.min(chars.len());
            if skip < chars.len() {
                let mut clipped = r.clone();
                clipped.text = chars.iter().skip(skip).collect();
                merged.push(clipped);
            }
        } else {
            merged.push(r.clone());
        }
    }
    if merged.is_empty() {
        merged.push(InlineRun::plain(""));
    }

    // Drop the in-between and trailing blocks.
    doc.blocks.drain((start_idx + 1)..=end_idx);
    if let Some(b) = doc.blocks.get_mut(start_idx) {
        b.body = BlockBody::Inline(merged);
    }
}

/// Toggle `flag` across every Inline-bodied block touched by `selection`.
fn apply_toggle_inline_range(doc: &mut EditorDoc, selection: DocSelection, flag: InlineFlag) {
    let (start, end) = selection.ordered(doc);
    let Some(start_idx) = find_idx(doc, start.block) else {
        return;
    };
    let Some(end_idx) = find_idx(doc, end.block) else {
        return;
    };

    if start_idx == end_idx {
        // Single block — just delegate to toggle_inline.
        if let Some(block) = doc.blocks.get_mut(start_idx) {
            if let BlockBody::Inline(runs) = &mut block.body {
                let local = LocalSelection {
                    anchor: Caret {
                        run: start.run,
                        offset: start.offset,
                    },
                    head: Caret {
                        run: end.run,
                        offset: end.offset,
                    },
                };
                toggle_inline(runs, local, flag);
            }
        }
        return;
    }

    // Multi-block: pick the toggle direction based on whether *every*
    // overlapping run already has the flag. If so, clear; otherwise set.
    // Use toggle_inline's per-block "all_set then clear" convention by
    // directly inspecting state across the range.
    let all_set = inline_range_uniformly_set(doc, &selection, flag, start_idx, end_idx);
    let target_value = !all_set;

    for i in start_idx..=end_idx {
        let block = match doc.blocks.get_mut(i) {
            Some(b) => b,
            None => continue,
        };
        let BlockBody::Inline(runs) = &mut block.body else {
            continue;
        };
        let local_anchor = if i == start_idx {
            Caret {
                run: start.run,
                offset: start.offset,
            }
        } else {
            Caret::START
        };
        let local_head = if i == end_idx {
            Caret {
                run: end.run,
                offset: end.offset,
            }
        } else {
            Caret::end(runs)
        };
        let local = LocalSelection {
            anchor: local_anchor,
            head: local_head,
        };
        // toggle_inline's direction is decided per-block, so call set/clear
        // directly via a single-block helper that respects target_value.
        set_inline_range(runs, local, flag, target_value);
    }
}

/// Splice `blocks` into the document at `at`. If `at` lands inside an
/// Inline block, the block is split first; the pasted blocks are inserted
/// between the halves.
fn apply_paste_blocks(doc: &mut EditorDoc, at: DocPosition, blocks: Vec<EditorBlock>) {
    if blocks.is_empty() {
        return;
    }
    let Some(idx) = find_idx(doc, at.block) else {
        // Anchor block is gone; append.
        doc.blocks.extend(blocks);
        return;
    };
    let target_block = &doc.blocks[idx];
    match &target_block.body {
        BlockBody::Inline(_) => {
            // Split at (run, offset), then insert pasted blocks between.
            apply_split(doc, at.block, at.run, at.offset);
            for (i, b) in blocks.into_iter().enumerate() {
                doc.blocks.insert(idx + 1 + i, b);
            }
        }
        _ => {
            // Non-inline target: just insert after.
            for (i, b) in blocks.into_iter().enumerate() {
                doc.blocks.insert(idx + 1 + i, b);
            }
        }
    }
}

/// True iff every run that overlaps `selection` (in the index span
/// `start_idx..=end_idx`) has `flag` set.
fn inline_range_uniformly_set(
    doc: &EditorDoc,
    selection: &DocSelection,
    flag: InlineFlag,
    start_idx: usize,
    end_idx: usize,
) -> bool {
    let (start, end) = selection.ordered(doc);
    let mut saw_any = false;
    for i in start_idx..=end_idx {
        let block = match doc.blocks.get(i) {
            Some(b) => b,
            None => continue,
        };
        let BlockBody::Inline(runs) = &block.body else {
            continue;
        };
        // For this block, walk runs and check those whose char-span overlaps
        // the per-block selection extent.
        let mut acc = 0usize;
        let local_lo: usize = if i == start_idx {
            // Convert (start.run, start.offset) to absolute char offset.
            absolute_char(runs, start.run, start.offset)
        } else {
            0
        };
        let local_hi: usize = if i == end_idx {
            absolute_char(runs, end.run, end.offset)
        } else {
            runs.iter().map(|r| r.text.chars().count()).sum()
        };
        if local_lo >= local_hi {
            continue;
        }
        for r in runs {
            let len = r.text.chars().count();
            let run_lo = acc;
            let run_hi = acc + len;
            acc = run_hi;
            let overlap_lo = run_lo.max(local_lo);
            let overlap_hi = run_hi.min(local_hi);
            if overlap_lo >= overlap_hi {
                continue;
            }
            saw_any = true;
            let has = match flag {
                InlineFlag::Bold => r.bold,
                InlineFlag::Italic => r.italic,
                InlineFlag::Code => r.code,
                InlineFlag::Link => r.link.is_some(),
            };
            if !has {
                return false;
            }
        }
    }
    saw_any
}

fn absolute_char(runs: &[InlineRun], run: usize, offset: usize) -> usize {
    let mut acc = 0usize;
    for (i, r) in runs.iter().enumerate() {
        if i == run {
            return acc + offset;
        }
        acc += r.text.chars().count();
    }
    acc
}

/// Set `flag` on every run inside `local` to `value`. Splits runs at the
/// selection bounds first so the change applies only to overlapping chars.
/// Mirrors `toggle_inline`'s split-then-flag-then-coalesce pattern, but with
/// an explicit target value instead of an XOR-style toggle.
fn set_inline_range(
    runs: &mut Vec<InlineRun>,
    local: LocalSelection,
    flag: InlineFlag,
    value: bool,
) {
    // Use toggle_inline's behavior conditionally: if its derived all_set
    // matches what we want as a no-op, run it once to drive to !all_set;
    // otherwise leave alone. This avoids re-implementing the splitting
    // logic. We compute the per-block all-set by reading runs first.
    use crate::ui::blocks::inline_editor::compare;
    let (start, end) = if compare(local.anchor, local.head).is_le() {
        (local.anchor, local.head)
    } else {
        (local.head, local.anchor)
    };
    if start == end {
        return;
    }
    // Determine current all_set via the same per-run scan toggle_inline uses.
    let abs_start = absolute_char(runs, start.run, start.offset);
    let abs_end = absolute_char(runs, end.run, end.offset);
    let all_set = run_range_all_have_flag(runs, abs_start, abs_end, flag);
    if all_set == value {
        return; // Already in the target state.
    }
    toggle_inline(runs, local, flag);
}

fn run_range_all_have_flag(
    runs: &[InlineRun],
    abs_lo: usize,
    abs_hi: usize,
    flag: InlineFlag,
) -> bool {
    let mut acc = 0usize;
    let mut saw_any = false;
    for r in runs {
        let len = r.text.chars().count();
        let run_lo = acc;
        let run_hi = acc + len;
        acc = run_hi;
        let overlap_lo = run_lo.max(abs_lo);
        let overlap_hi = run_hi.min(abs_hi);
        if overlap_lo >= overlap_hi {
            continue;
        }
        saw_any = true;
        let has = match flag {
            InlineFlag::Bold => r.bold,
            InlineFlag::Italic => r.italic,
            InlineFlag::Code => r.code,
            InlineFlag::Link => r.link.is_some(),
        };
        if !has {
            return false;
        }
    }
    saw_any
}

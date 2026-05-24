//! `BlockAction` and the `apply` chokepoint.
//!
//! Every block-tree mutation goes through `apply(doc, action)`. Inline-edit
//! actions (`EditInline`, `EditCode`) are also routed here so the document
//! model stays the single source of truth for persistence — even though
//! per-block widgets keep reactive copies for live editing.

use crate::model::sync::canonicalize_body;
use crate::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
use serde_json::Value;

/// One discrete edit. Each variant maps to one function below.
#[derive(Debug, Clone)]
pub enum BlockAction {
    /// Split the block at `byte_offset` into the block's flat text. The
    /// trailing portion becomes a new block of the same kind directly after
    /// the original. `new_block_id`: `None` mints a fresh id; `Some(id)`
    /// uses the provided id so undo↔redo round-trips are id-stable.
    Split {
        block_id: BlockId,
        byte_offset: usize,
        new_block_id: Option<BlockId>,
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
    /// Replace `block_id`'s entire `body` with `new_body`. Generic content
    /// edit — works for any body shape (Inline, Code, List, Opaque). The
    /// inverse swaps the old body back. Used by widgets that construct the
    /// target body locally rather than declaring a per-shape intent.
    EditBlockBody {
        block_id: BlockId,
        new_body: BlockBody,
    },
}

/// Apply one `BlockAction` to the document.
///
/// Returns `Some((canonical_action, inverse_action))` for any recordable
/// action — the action that, when applied to the post-state, restores the
/// pre-state. `canonical_action` differs from the input only for `Split`,
/// which mints ids: the returned form has `new_block_id: Some(...)`
/// filled in (for inline-bodied splits, the new block's id; for list-
/// bodied splits, the new list item's id), so a future redo reuses the
/// same id and undo↔redo stays id-stable without post-apply patching.
///
/// Returns `None` when the action does not produce a recordable inverse.
/// Two cases:
/// 1. **UI-only actions** (`OpenSlashMenu`) — never touch the model.
/// 2. **No-op or first-block actions** — target block id not found,
///    `Move` with a same-position gap, `MergeWithPrev` on the first
///    block, or `Delete` of the first block (no predecessor anchor
///    exists for the `InsertAfter` inverse). The model may be unchanged
///    or, for first-block `Delete`, mutated in a way that cannot be
///    undone via the current action enum. First-block `Delete` is the
///    lone intentionally-unrecordable mutation remaining after stage 3.
pub fn apply(doc: &mut EditorDoc, action: BlockAction) -> Option<(BlockAction, BlockAction)> {
    match action {
        BlockAction::Split {
            block_id,
            byte_offset,
            new_block_id,
        } => apply_split(doc, block_id, byte_offset, new_block_id),
        BlockAction::MergeWithPrev { block_id } => apply_merge(doc, block_id),
        BlockAction::InsertAfter { anchor, new_block } => {
            apply_insert_after(doc, anchor, new_block)
        }
        BlockAction::Delete { block_id } => apply_delete(doc, block_id),
        BlockAction::Move { block_id, to_index } => apply_move(doc, block_id, to_index),
        BlockAction::ChangeType { block_id, new_kind } => {
            apply_change_type(doc, block_id, new_kind)
        }
        // UI-only — handled by the editor pane's action sink, not the model.
        BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditAttrs {
            block_id,
            new_attrs,
        } => apply_edit_attrs(doc, block_id, new_attrs),
        BlockAction::EditBlockBody { block_id, new_body } => {
            apply_edit_block_body(doc, block_id, new_body)
        }
    }
}

fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let old_attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();
    if let Some(meta) = block.plugin.as_mut() {
        meta.attrs = new_attrs.clone();
    }
    // Mirror `lang` from attrs into BlockKind::Code.lang so that subsequent
    // serialization (or any inspection of `block.kind` between edit and save)
    // sees the canonical lang. The list block has no equivalent mirror because
    // BlockKind::List carries `ordered`, which is already the source of truth
    // for the serializer's native arm; for code, attrs is the source of truth,
    // and kind.lang is the mirror.
    if let BlockKind::Code { .. } = &block.kind {
        if let Some(new_lang) = block
            .plugin
            .as_ref()
            .and_then(|m| m.attrs.get("lang"))
            .and_then(Value::as_str)
        {
            block.kind = BlockKind::Code {
                lang: new_lang.to_string(),
            };
        }
    }
    Some((
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs,
        },
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: old_attrs,
        },
    ))
}

fn find_idx(doc: &EditorDoc, id: BlockId) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

fn apply_split(
    doc: &mut EditorDoc,
    id: BlockId,
    byte_offset: usize,
    new_block_id: Option<BlockId>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get(idx)?;
    let kind = block.kind.clone();
    let body = block.body.clone();

    match body {
        BlockBody::Code(text) => {
            // Code "split" inserts a '\n' rather than producing a new
            // top-level block. Build the new Code body and route through
            // apply_edit_block_body so the inverse is recordable: an
            // EditBlockBody restoring the old Code text.
            let mut new_text = text;
            new_text.insert(byte_offset.min(new_text.len()), '\n');
            let (_inner_canonical, inverse) =
                apply_edit_block_body(doc, id, BlockBody::Code(new_text))?;
            Some((
                BlockAction::Split {
                    block_id: id,
                    byte_offset,
                    new_block_id: None,
                },
                inverse,
            ))
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
            let mut tail_block = match kind {
                BlockKind::Paragraph => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
                BlockKind::Heading(level) => {
                    EditorBlock::heading(level, vec![InlineRun::plain(tail)])
                }
                _ => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
            };
            let minted_id = if let Some(nid) = new_block_id {
                tail_block.id = nid;
                nid
            } else {
                tail_block.id
            };
            doc.blocks.insert(idx + 1, tail_block);
            Some((
                BlockAction::Split {
                    block_id: id,
                    byte_offset,
                    new_block_id: Some(minted_id),
                },
                BlockAction::MergeWithPrev {
                    block_id: minted_id,
                },
            ))
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
            // `items` here is our local clone (from `body = block.body.clone()`
            // above), so we can mutate it freely off-doc.
            let mut new_items = items;
            split_item_at_with_id(&mut new_items, pos, local, new_block_id);
            let minted_id = new_items.get(pos + 1)?.id;
            let (_inner_canonical, inverse) =
                apply_edit_block_body(doc, id, BlockBody::List(new_items))?;
            Some((
                BlockAction::Split {
                    block_id: id,
                    byte_offset,
                    new_block_id: Some(minted_id),
                },
                inverse,
            ))
        }
        BlockBody::Opaque(_) => None,
    }
}

fn apply_merge(doc: &mut EditorDoc, id: BlockId) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    if idx == 0 {
        return None;
    }
    let prev_idx = idx - 1;
    // A list block merges only its *first item* into the previous block; the
    // remaining items stay as a list. Merging the whole list away would
    // silently drop every item — this fires on Backspace at the start of the
    // first list item.
    if matches!(
        doc.blocks.get(idx).map(|b| &b.body),
        Some(BlockBody::List(_))
    ) {
        let prev_id = doc.blocks.get(prev_idx)?.id;
        let prev_flat_len: usize = match &doc.blocks.get(prev_idx)?.body {
            BlockBody::Inline(runs) => runs.iter().map(|r| r.text.len()).sum(),
            _ => 0,
        };
        let first_runs = match doc.blocks.get_mut(idx).map(|b| &mut b.body) {
            Some(BlockBody::List(items)) if !items.is_empty() => Some(items.remove(0).runs),
            _ => None,
        };
        if let Some(runs) = first_runs {
            if let Some(BlockBody::Inline(prev_runs)) =
                doc.blocks.get_mut(prev_idx).map(|b| &mut b.body)
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
        return Some((
            BlockAction::MergeWithPrev { block_id: id },
            BlockAction::Split {
                block_id: prev_id,
                byte_offset: prev_flat_len,
                new_block_id: None,
            },
        ));
    }
    let prev_id = doc.blocks.get(prev_idx)?.id;
    let prev_flat_len: usize = match &doc.blocks.get(prev_idx)?.body {
        BlockBody::Inline(runs) => runs.iter().map(|r| r.text.len()).sum(),
        _ => 0,
    };
    let cur_id = doc.blocks.get(idx)?.id;
    let cur = doc.blocks.remove(idx);
    let Some(prev) = doc.blocks.get_mut(prev_idx) else {
        doc.blocks.insert(idx, cur);
        return None;
    };
    if let (BlockBody::Inline(prev_runs), BlockBody::Inline(cur_runs)) = (&mut prev.body, cur.body)
    {
        prev_runs.extend(cur_runs);
    }
    Some((
        BlockAction::MergeWithPrev { block_id: id },
        BlockAction::Split {
            block_id: prev_id,
            byte_offset: prev_flat_len,
            new_block_id: Some(cur_id),
        },
    ))
}

fn apply_insert_after(
    doc: &mut EditorDoc,
    anchor: BlockId,
    new_block: EditorBlock,
) -> Option<(BlockAction, BlockAction)> {
    let pos = find_idx(doc, anchor)
        .map(|i| i + 1)
        .unwrap_or(doc.blocks.len());
    let inserted_id = new_block.id;
    // The canonical action and the doc each need an owned copy; clone once
    // for the doc, move the original into the canonical below.
    if pos > doc.blocks.len() {
        doc.blocks.push(new_block.clone());
    } else {
        doc.blocks.insert(pos, new_block.clone());
    }
    Some((
        BlockAction::InsertAfter { anchor, new_block },
        BlockAction::Delete {
            block_id: inserted_id,
        },
    ))
}

fn apply_delete(doc: &mut EditorDoc, id: BlockId) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let removed = doc.blocks.remove(idx);
    if doc.blocks.is_empty() {
        doc.blocks
            .push(EditorBlock::paragraph(vec![InlineRun::plain("")]));
    }
    // No predecessor anchor for the first block — return `None` to mark
    // the action as unrecordable (preserves current behavior).
    let anchor = idx
        .checked_sub(1)
        .and_then(|j| doc.blocks.get(j))
        .map(|b| b.id)?;
    Some((
        BlockAction::Delete { block_id: id },
        BlockAction::InsertAfter {
            anchor,
            new_block: removed,
        },
    ))
}

fn apply_move(
    doc: &mut EditorDoc,
    id: BlockId,
    to_index: usize,
) -> Option<(BlockAction, BlockAction)> {
    let from = find_idx(doc, id)?;
    let target_gap = to_index.min(doc.blocks.len());
    // Dropping into the gap immediately before or after self is a no-op.
    if target_gap == from || target_gap == from + 1 {
        return None;
    }
    let block = doc.blocks.remove(from);
    let insert_at = if target_gap > from {
        target_gap - 1
    } else {
        target_gap
    };
    doc.blocks.insert(insert_at, block);
    let inverse_to = if to_index > from { from } else { from + 1 };
    Some((
        BlockAction::Move {
            block_id: id,
            to_index,
        },
        BlockAction::Move {
            block_id: id,
            to_index: inverse_to,
        },
    ))
}

fn apply_change_type(
    doc: &mut EditorDoc,
    id: BlockId,
    new_kind: BlockKind,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let old_kind = block.kind.clone();
    match (&new_kind, &block.body) {
        // ── To Inline (Paragraph / Heading) ──────────────────────────────
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_runs)) => {
            // Body shape already matches — just update kind.
            block.kind = new_kind.clone();
        }
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Code(text)) => {
            block.kind = new_kind.clone();
            block.body = BlockBody::Inline(vec![InlineRun::plain(text.clone())]);
            block.plugin = None;
        }
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::List(items)) => {
            block.kind = new_kind.clone();
            // Flatten: join each item's runs text with '\n', wrap in a single
            // InlineRun.  Loses item boundaries — same lossy direction as the
            // inverse (undo restores via snapshot).
            let text: String = items
                .iter()
                .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            block.body = BlockBody::Inline(vec![InlineRun::plain(text)]);
            block.plugin = None;
        }

        // ── To Code ──────────────────────────────────────────────────────
        (BlockKind::Code { lang }, BlockBody::Inline(runs)) => {
            let text: String = runs.iter().map(|r| r.text.clone()).collect();
            block.kind = BlockKind::Code { lang: lang.clone() };
            block.body = BlockBody::Code(text);
            block.plugin = Some(PluginMeta::code(lang));
        }
        (BlockKind::Code { lang }, BlockBody::Code(_text)) => {
            // Only lang changes — update kind.lang and mirror into plugin.
            block.kind = BlockKind::Code { lang: lang.clone() };
            if let Some(meta) = block.plugin.as_mut() {
                meta.attrs.insert("lang".into(), Value::String(lang.clone()));
            }
        }
        (BlockKind::Code { lang }, BlockBody::List(items)) => {
            let text: String = items
                .iter()
                .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            block.kind = BlockKind::Code { lang: lang.clone() };
            block.body = BlockBody::Code(text);
            block.plugin = Some(PluginMeta::code(lang));
        }

        // ── To List ──────────────────────────────────────────────────────
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
        (BlockKind::List { ordered }, BlockBody::Code(text)) => {
            // Split text on '\n', one ListItem per line.
            let items: Vec<ListItem> = text
                .split('\n')
                .map(|line| ListItem {
                    id: BlockId::new(),
                    runs: vec![InlineRun::plain(line.to_string())],
                })
                .collect();
            block.kind = BlockKind::List { ordered: *ordered };
            block.body = BlockBody::List(items);
            block.plugin = Some(PluginMeta::list(*ordered));
        }
        (BlockKind::List { ordered }, BlockBody::List(_items)) => {
            // Items already match — just update the ordered flag and mirror.
            block.kind = BlockKind::List { ordered: *ordered };
            if let Some(meta) = block.plugin.as_mut() {
                meta.attrs.insert("ordered".into(), Value::Bool(*ordered));
            }
        }

        // ── Opaque / fallback ────────────────────────────────────────────
        _ => {
            block.kind = new_kind.clone();
        }
    }
    // NOTE: this inverse restores `kind` only, not `body`. Body conversions
    // (Inline→Code stringifies runs; Inline→List wraps into a single item)
    // are lossy on undo — the original body is not snapshot here. This
    // matches the pre-refactor behavior. Stage 3's `EditBlockBody` collapse
    // makes ChangeType fully reversible by snapshotting body alongside kind.
    // See
    // `docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`
    // Section 3 — "Shift B".
    Some((
        BlockAction::ChangeType {
            block_id: id,
            new_kind,
        },
        BlockAction::ChangeType {
            block_id: id,
            new_kind: old_kind,
        },
    ))
}

/// Split `items[pos]` at `byte_offset` into its flat text. The head stays in
/// place; the tail becomes a `ListItem` inserted at `pos + 1` with the
/// provided id when `new_item_id` is `Some`, or a freshly minted id when
/// `None`. Styling is dropped on both sides (the split produces plain runs).
///
/// Visible to `crate::ui::blocks::list` so the list widget can construct
/// post-split list bodies for its `EditBlockBody` emissions in stage 4.
pub(crate) fn split_item_at_with_id(
    items: &mut Vec<ListItem>,
    pos: usize,
    byte_offset: usize,
    new_item_id: Option<BlockId>,
) {
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
    let new_id = new_item_id.unwrap_or_default();
    items.insert(
        pos + 1,
        ListItem {
            id: new_id,
            runs: vec![InlineRun::plain(tail)],
        },
    );
}

/// Replace the body of `id` with `new_body`. Returns the (canonical action,
/// inverse action) pair: the inverse is another `EditBlockBody` carrying
/// the old body. Works for any body shape — the helper is shape-agnostic.
///
/// Returns `None` when `new_body` already equals the block's current body —
/// a no-op edit records nothing on the undo stack. This lets callers emit a
/// "commit the live editor buffer" `EditBlockBody` unconditionally (e.g.
/// before an undo, or before a structural list edit) without producing a
/// spurious empty undo entry when there was no pending change.
///
/// Both the incoming body and the stored body are compared in canonical
/// form ([`canonicalize_body`]): a body collected from the live editors and
/// the structurally-identical body stored in the model can differ only in
/// run splitting (a styled span vs. a styled span plus a typed plain tail)
/// or empty runs. Comparing canonically recognises those as no-ops, and the
/// stored/recorded body is the canonical one so the model stays canonical.
fn apply_edit_block_body(
    doc: &mut EditorDoc,
    id: BlockId,
    new_body: BlockBody,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let new_body = canonicalize_body(&new_body);
    if canonicalize_body(&block.body) == new_body {
        return None;
    }
    let old_body = std::mem::replace(&mut block.body, new_body.clone());
    Some((
        BlockAction::EditBlockBody {
            block_id: id,
            new_body,
        },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: old_body,
        },
    ))
}

//! `BlockAction` and the `apply` chokepoint.
//!
//! Every block-tree mutation goes through `apply(doc, action)`. Inline-edit
//! actions (`EditInline`, `EditCode`) are also routed here so the document
//! model stays the single source of truth for persistence — even though
//! per-block widgets keep reactive copies for live editing.

use crate::model::sync::canonicalize_body;
use crate::model::types::{
    Align, BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
use serde_json::Value;
use std::rc::Rc;

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
        new_attrs: Box<serde_json::Map<String, serde_json::Value>>,
    },
    /// Replace `block_id`'s entire `body` with `new_body`. Generic content
    /// edit — works for any body shape (Inline, Code, List, Opaque). The
    /// inverse swaps the old body back. Used by widgets that construct the
    /// target body locally rather than declaring a per-shape intent.
    EditBlockBody {
        block_id: BlockId,
        new_body: Box<BlockBody>,
        /// True when this commit originates from a built-in editor widget
        /// (paragraph, heading, code, list) rather than plugin-originated input.
        /// Provenance metadata: surfaced in `apply_edit_block_body`'s
        /// coercion-invariant assertion. Both provenances are coerced to the
        /// block's kind, so neither can leave the block in an unrenderable shape.
        built_in: bool,
    },
    /// Insert `new_block` immediately after `anchor`. If `anchor` is missing,
    /// appends to the end.
    InsertAfter {
        anchor: BlockId,
        new_block: Box<EditorBlock>,
    },
    /// Replace the document's front matter with `new_front_matter`. Used by
    /// the inspector to make front-matter edits undoable. One action per
    /// commit (Title blur, Slug blur, Date validation success, etc.).
    /// Boxed to keep `BlockAction` within the 40-byte size guard — front
    /// matter is small (KB at most) but not small enough to fit inline.
    #[allow(clippy::large_enum_variant)]
    EditFrontMatter {
        new_front_matter: Box<lopress_core::FrontMatter>,
    },
    /// Insert an empty row at index `at` (0..=rows.len()). New cells match the
    /// current column count. `at == 0` would insert above the header — callers
    /// pass `at >= 1`; the apply clamps into the body region defensively.
    TableInsertRow {
        block_id: BlockId,
        at: usize,
    },
    /// Delete body row `row`. No-op (returns None) for the header row (0) or
    /// when it is the last remaining body row.
    TableDeleteRow {
        block_id: BlockId,
        row: usize,
    },
    /// Insert an empty column at index `at` (0..=col_count) across every row,
    /// with `Align::None`.
    TableInsertColumn {
        block_id: BlockId,
        at: usize,
    },
    /// Delete column `col` across every row. No-op when it is the last column.
    TableDeleteColumn {
        block_id: BlockId,
        col: usize,
    },
    /// Set column `col`'s alignment.
    TableSetAlign {
        block_id: BlockId,
        col: usize,
        align: Align,
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
            apply_insert_after(doc, anchor, *new_block)
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
        } => apply_edit_attrs(doc, block_id, *new_attrs),
        BlockAction::EditBlockBody {
            block_id,
            new_body,
            built_in,
        } => apply_edit_block_body(doc, block_id, *new_body, built_in),
        BlockAction::EditFrontMatter { new_front_matter } => {
            apply_edit_front_matter(doc, *new_front_matter)
        }
        BlockAction::TableInsertRow { block_id, at } => apply_table_insert_row(doc, block_id, at),
        BlockAction::TableDeleteRow { block_id, row } => apply_table_delete_row(doc, block_id, row),
        BlockAction::TableInsertColumn { block_id, at } => {
            apply_table_insert_column(doc, block_id, at)
        }
        BlockAction::TableDeleteColumn { block_id, col } => {
            apply_table_delete_column(doc, block_id, col)
        }
        BlockAction::TableSetAlign {
            block_id,
            col,
            align,
        } => apply_table_set_align(doc, block_id, col, align),
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
                lang: Rc::from(new_lang),
            };
        }
    }
    Some((
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: Box::new(new_attrs),
        },
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: Box::new(old_attrs),
        },
    ))
}

fn find_idx(doc: &EditorDoc, id: BlockId) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

/// True when `block` is the read-more marker (`lopress:more`).
fn is_read_more(block: &EditorBlock) -> bool {
    block
        .plugin
        .as_ref()
        .is_some_and(|m| &*m.block_type_name == "lopress:more")
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
                apply_edit_block_body(doc, id, BlockBody::Code(new_text), false)?;
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
                apply_edit_block_body(doc, id, BlockBody::List(new_items), false)?;
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
        BlockBody::Table(_) => None,
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
    // One read-more marker per post: refuse a second.
    if is_read_more(&new_block) && doc.blocks.iter().any(is_read_more) {
        return None;
    }
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
        BlockAction::InsertAfter {
            anchor,
            new_block: Box::new(new_block),
        },
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
            new_block: Box::new(removed),
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
    // Guard: an Opaque (unknown-plugin) body has no sensible conversion to
    // another kind. Changing only the kind would leave `{kind, Opaque}`, which
    // `to_core` cannot serialize — the block is silently dropped on save. The
    // fallback view routes Opaque blocks through a focusable card whose toolbar
    // can fire ChangeType, so guard it here at the model chokepoint: treat it as
    // a no-op. These blocks are recoverable via Delete only (the fallback's
    // warning says as much).
    if matches!(block.body, BlockBody::Opaque(_)) {
        return None;
    }
    // A table body has no sensible conversion to another kind, and the kind-
    // cycler toolbar buttons would otherwise leave a (Paragraph, Table)
    // mismatch that renders as an empty gap. Treat ChangeType on a table as a
    // no-op, exactly like the Opaque guard above.
    if matches!(block.body, BlockBody::Table(_)) {
        return None;
    }
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
                meta.attrs
                    .insert("lang".into(), Value::String(lang.to_string()));
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
fn apply_edit_front_matter(
    doc: &mut EditorDoc,
    new_fm: lopress_core::FrontMatter,
) -> Option<(BlockAction, BlockAction)> {
    if doc.front_matter == new_fm {
        return None;
    }
    let old_fm = std::mem::replace(&mut doc.front_matter, new_fm.clone());
    Some((
        BlockAction::EditFrontMatter {
            new_front_matter: Box::new(new_fm),
        },
        BlockAction::EditFrontMatter {
            new_front_matter: Box::new(old_fm),
        },
    ))
}

fn table_body_mut(doc: &mut EditorDoc, id: BlockId) -> Option<&mut crate::model::types::TableData> {
    let idx = find_idx(doc, id)?;
    match &mut doc.blocks.get_mut(idx)?.body {
        BlockBody::Table(data) => Some(data),
        _ => None,
    }
}

fn apply_table_insert_row(
    doc: &mut EditorDoc,
    id: BlockId,
    at: usize,
) -> Option<(BlockAction, BlockAction)> {
    use crate::model::types::{BlockId as Bid, TableCell, TableRow};
    let data = table_body_mut(doc, id)?;
    let cols = data.align.len();
    // Never insert above the header row.
    let at = at.clamp(1, data.rows.len());
    let new_row = TableRow {
        id: Bid::new(),
        cells: (0..cols)
            .map(|_| TableCell {
                id: Bid::new(),
                runs: vec![],
            })
            .collect(),
    };
    data.rows.insert(at, new_row);
    Some((
        BlockAction::TableInsertRow { block_id: id, at },
        BlockAction::TableDeleteRow {
            block_id: id,
            row: at,
        },
    ))
}

fn apply_table_delete_row(
    doc: &mut EditorDoc,
    id: BlockId,
    row: usize,
) -> Option<(BlockAction, BlockAction)> {
    let before = {
        let data = table_body_mut(doc, id)?;
        if row == 0 || row >= data.rows.len() || data.rows.len() <= 2 {
            return None;
        }
        BlockBody::Table(data.clone())
    };
    let data = table_body_mut(doc, id)?;
    data.rows.remove(row);
    Some((
        BlockAction::TableDeleteRow { block_id: id, row },
        // Inverse: reinsert the exact removed row at the same index. We model
        // it as a generic body restore so the cells/ids come back intact.
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(before),
            built_in: true,
        },
    ))
}

fn apply_table_insert_column(
    doc: &mut EditorDoc,
    id: BlockId,
    at: usize,
) -> Option<(BlockAction, BlockAction)> {
    use crate::model::types::{Align as A, BlockId as Bid, TableCell};
    let data = table_body_mut(doc, id)?;
    let at = at.min(data.align.len());
    data.align.insert(at, A::None);
    for row in &mut data.rows {
        let col = at.min(row.cells.len());
        row.cells.insert(
            col,
            TableCell {
                id: Bid::new(),
                runs: vec![],
            },
        );
    }
    Some((
        BlockAction::TableInsertColumn { block_id: id, at },
        BlockAction::TableDeleteColumn {
            block_id: id,
            col: at,
        },
    ))
}

fn apply_table_delete_column(
    doc: &mut EditorDoc,
    id: BlockId,
    col: usize,
) -> Option<(BlockAction, BlockAction)> {
    let before = {
        let data = table_body_mut(doc, id)?;
        if col >= data.align.len() || data.align.len() <= 1 {
            return None;
        }
        BlockBody::Table(data.clone())
    };
    let data = table_body_mut(doc, id)?;
    data.align.remove(col);
    for row in &mut data.rows {
        if col < row.cells.len() {
            row.cells.remove(col);
        }
    }
    Some((
        BlockAction::TableDeleteColumn { block_id: id, col },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(before),
            built_in: true,
        },
    ))
}

fn apply_table_set_align(
    doc: &mut EditorDoc,
    id: BlockId,
    col: usize,
    align: crate::model::types::Align,
) -> Option<(BlockAction, BlockAction)> {
    let data = table_body_mut(doc, id)?;
    let old = *data.align.get(col)?;
    if old == align {
        return None;
    }
    let slot = data.align.get_mut(col)?;
    *slot = align;
    Some((
        BlockAction::TableSetAlign {
            block_id: id,
            col,
            align,
        },
        BlockAction::TableSetAlign {
            block_id: id,
            col,
            align: old,
        },
    ))
}

/// True when `body` is the expected shape for `kind`. Used by the
/// debug_assert in apply_edit_block_body to distinguish valid from
/// mismatched commits.
fn body_matches_kind(kind: &BlockKind, body: &BlockBody) -> bool {
    matches!(
        (kind, body),
        (
            BlockKind::Paragraph | BlockKind::Heading(_),
            BlockBody::Inline(_)
        ) | (BlockKind::Code { .. }, BlockBody::Code(_))
            | (BlockKind::List { .. }, BlockBody::List(_))
            | (BlockKind::Table, BlockBody::Table(_))
            | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
    )
}

/// Flatten any body to its plain text. Mirrors the flattening that
/// `apply_change_type` performs: `Inline`/`List` runs are concatenated, list
/// items are joined with `\n`, `Code` is already flat, and `Opaque` has no
/// text. Shared between `apply_change_type` / `coerce_body_to_kind` and the
/// render-layer fallback view so every code path presents the same text.
pub fn body_to_flat_text(body: &BlockBody) -> String {
    match body {
        BlockBody::Inline(runs) => runs.iter().map(|r| r.text.as_str()).collect(),
        BlockBody::Code(text) => text.clone(),
        BlockBody::List(items) => items
            .iter()
            .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n"),
        BlockBody::Table(data) => data
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|c| c.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        BlockBody::Opaque(_) => String::new(),
    }
}

/// Coerce `body` into the shape required by `kind`.
///
/// `EditBlockBody` carries a body produced by the block's currently-mounted
/// editor widget, whose shape is fixed by `kind`. A mismatched shape means the
/// commit is *stale* — it was emitted by a widget that a `ChangeType` has since
/// swapped out (e.g. the paragraph editor's `FocusLost` commit firing during
/// the editor-pane rebuild that `ChangeType` triggers, landing *after* the
/// block already became Code). Letting that stale commit through would drop the
/// block into an unrenderable `(kind, body)` pair, which `block_view` draws as
/// an empty, uneditable gap. Instead, convert the body to the kind's shape
/// (preserving the text). Conversions mirror `apply_change_type`'s body arms.
fn coerce_body_to_kind(kind: &BlockKind, body: BlockBody) -> BlockBody {
    match (kind, &body) {
        // Shape already matches the kind — keep as-is (the common case; every
        // non-stale commit lands here, so this is a no-op in normal editing).
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_))
        | (BlockKind::Code { .. }, BlockBody::Code(_))
        | (BlockKind::List { .. }, BlockBody::List(_))
        | (BlockKind::Table, BlockBody::Table(_))
        | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
        | (BlockKind::Image, BlockBody::Opaque(_)) => body,

        // → Inline (Paragraph / Heading).
        (BlockKind::Paragraph | BlockKind::Heading(_), _) => {
            BlockBody::Inline(vec![InlineRun::plain(body_to_flat_text(&body))])
        }
        // → Code.
        (BlockKind::Code { .. }, _) => BlockBody::Code(body_to_flat_text(&body)),
        // → List: one item per line of the flattened text.
        (BlockKind::List { .. }, _) => BlockBody::List(
            body_to_flat_text(&body)
                .split('\n')
                .map(|line| ListItem {
                    id: BlockId::new(),
                    runs: vec![InlineRun::plain(line.to_string())],
                })
                .collect(),
        ),
        // → Opaque from a non-Opaque body: no editor widget commits into an
        // opaque block, so this is unreachable in practice — leave it untouched.
        (BlockKind::Opaque { .. }, _) => body,
        // Image: body is always Opaque(Null); any mismatch is a programming
        // error — return the body as-is rather than panic.
        (BlockKind::Image, _) => body,
        // Table: body is always Table; any mismatch is a programming error —
        // return the body as-is rather than panic. No widget commits a
        // non-Table body into a Table block.
        (BlockKind::Table, _) => body,
    }
}

fn apply_edit_block_body(
    doc: &mut EditorDoc,
    id: BlockId,
    new_body: BlockBody,
    built_in: bool,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    // Coerce the incoming body to the block's kind so a stale or out-of-order
    // commit can never leave the block in an unrenderable shape. See
    // `coerce_body_to_kind`.
    //
    // A *mismatched* incoming body is expected, not a bug: a built-in editor
    // (built_in: true) legitimately emits a stale body after a ChangeType swaps
    // the kind out from under a still-mounted editor — the FocusLost flush
    // races the editor-pane rebuild and lands after the kind changed. For
    // Code/List blocks this flush is in fact the only path that carries
    // freshly-typed text into the model (the toolbar can't pre-commit them), so
    // we must accept and coerce it, not reject it. (An earlier assertion keyed
    // on `built_in` panicked here on that legitimate flow.)
    let new_body = canonicalize_body(&coerce_body_to_kind(&block.kind, new_body));
    // Invariant: coercion always yields a body whose shape matches the block's
    // kind, so the stored (kind, body) pair is always renderable. This catches
    // bugs in `coerce_body_to_kind` itself rather than false-flagging the
    // (expected) stale commit above. `built_in` is surfaced for provenance.
    debug_assert!(
        body_matches_kind(&block.kind, &new_body),
        "coerced EditBlockBody still mismatches kind: block {:?} kind {:?}, body {:?}, built_in {}",
        id,
        block.kind,
        new_body,
        built_in
    );
    if canonicalize_body(&block.body) == new_body {
        return None;
    }
    let old_body = std::mem::replace(&mut block.body, new_body.clone());
    Some((
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(new_body),
            built_in: false, // Record/inverse: external provenance.
        },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(old_body),
            built_in: false, // Record/inverse: external provenance.
        },
    ))
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn block_action_size_is_compact() {
        // After boxing heavy variants, BlockAction should fit in
        // a discriminant + pointer (roughly 9 bytes on x64, padded
        // to 16 bytes due to alignment). The guard threshold is 40
        // bytes to leave room for future small variants.
        let size = std::mem::size_of::<BlockAction>();
        assert!(
            size <= 40,
            "BlockAction is {} bytes (expected <= 40); box heavier variants",
            size
        );
    }
}

#[cfg(test)]
// Test module exercises edit-in-place paths that leave trailing unreachable
// branches when the front-matter block is consumed early.
#[allow(unreachable_code)]
#[allow(clippy::unreachable)]
mod front_matter_tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn apply_edit_front_matter_records_inverse() {
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("body")])],
            front_matter: lopress_core::FrontMatter {
                title: Some("old".to_string()),
                ..Default::default()
            },
        };
        let new_fm = lopress_core::FrontMatter {
            title: Some("new".to_string()),
            ..Default::default()
        };
        let (canonical, inverse) =
            apply_edit_front_matter(&mut doc, new_fm.clone()).expect("recorded");
        assert!(matches!(canonical, BlockAction::EditFrontMatter { .. }));

        // Apply the inverse: the doc's title should return to "old".
        let BlockAction::EditFrontMatter { new_front_matter } = inverse else {
            unreachable!();
        };
        apply_edit_front_matter(&mut doc, *new_front_matter);
        assert_eq!(doc.front_matter.title.as_deref(), Some("old"));
    }

    #[test]
    fn apply_edit_front_matter_no_op_returns_none() {
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("body")])],
            front_matter: lopress_core::FrontMatter {
                title: Some("same".to_string()),
                ..Default::default()
            },
        };
        let same = lopress_core::FrontMatter {
            title: Some("same".to_string()),
            ..Default::default()
        };
        assert!(apply_edit_front_matter(&mut doc, same).is_none());
    }
}

#[cfg(test)]
mod body_to_flat_text_tests {
    use super::*;

    #[test]
    fn inline_runs_concatenate() {
        let body = BlockBody::Inline(vec![
            InlineRun::plain("hello "),
            InlineRun {
                text: "world".into(),
                bold: true,
                ..Default::default()
            },
        ]);
        assert_eq!(body_to_flat_text(&body), "hello world");
    }

    #[test]
    fn code_returns_text_as_is() {
        let body = BlockBody::Code("fn main() {}".to_string());
        assert_eq!(body_to_flat_text(&body), "fn main() {}");
    }

    #[test]
    fn list_joins_items_with_newlines() {
        let body = BlockBody::List(vec![
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("a")],
            },
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("b")],
            },
        ]);
        assert_eq!(body_to_flat_text(&body), "a\nb");
    }

    #[test]
    fn opaque_returns_empty_string() {
        let body = BlockBody::Opaque(serde_json::json!({ "foo": 42 }));
        assert_eq!(body_to_flat_text(&body), "");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod read_more_guard_tests {
    use super::*;

    fn doc_with_para() -> EditorDoc {
        EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("p")])],
            front_matter: lopress_core::FrontMatter::default(),
        }
    }

    #[test]
    fn first_marker_inserts_second_is_rejected() {
        let mut doc = doc_with_para();
        // The doc has exactly one paragraph block from `doc_with_para`.
        let anchor = doc.blocks.first().unwrap().id;

        let first = apply(
            &mut doc,
            BlockAction::InsertAfter {
                anchor,
                new_block: Box::new(EditorBlock::read_more()),
            },
        );
        assert!(first.is_some(), "first marker should insert");
        assert_eq!(doc.blocks.len(), 2);

        // After insertion, the first block is still the original paragraph.
        let anchor2 = doc.blocks.first().unwrap().id;
        let second = apply(
            &mut doc,
            BlockAction::InsertAfter {
                anchor: anchor2,
                new_block: Box::new(EditorBlock::read_more()),
            },
        );
        assert!(second.is_none(), "second marker must be rejected");
        assert_eq!(doc.blocks.len(), 2, "no second marker inserted");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod table_action_tests {
    use super::*;
    use crate::model::types::{Align, EditorBlock, EditorDoc};

    fn doc_with_table() -> EditorDoc {
        EditorDoc {
            blocks: vec![EditorBlock::table_default()], // 2x2
            front_matter: lopress_core::FrontMatter::default(),
        }
    }

    fn table_data(doc: &EditorDoc) -> crate::model::types::TableData {
        match &doc.blocks[0].body {
            BlockBody::Table(d) => d.clone(),
            _ => panic!("expected table body"),
        }
    }

    #[test]
    fn insert_row_appends_and_undoes() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        let (_c, inverse) = apply(
            &mut doc,
            BlockAction::TableInsertRow {
                block_id: id,
                at: 2,
            },
        )
        .unwrap();
        assert_eq!(table_data(&doc).rows.len(), 3);
        apply(&mut doc, inverse);
        assert_eq!(table_data(&doc).rows.len(), 2);
    }

    #[test]
    fn delete_row_refuses_header_and_last_body() {
        let mut doc = doc_with_table(); // header + 1 body row
        let id = doc.blocks[0].id;
        // Deleting the header (row 0) is refused.
        assert!(apply(
            &mut doc,
            BlockAction::TableDeleteRow {
                block_id: id,
                row: 0
            }
        )
        .is_none());
        // Deleting the only body row is refused (must keep >= 1 body row).
        assert!(apply(
            &mut doc,
            BlockAction::TableDeleteRow {
                block_id: id,
                row: 1
            }
        )
        .is_none());
        assert_eq!(table_data(&doc).rows.len(), 2);
    }

    #[test]
    fn insert_and_delete_column_roundtrip() {
        let mut doc = doc_with_table(); // 2 columns
        let id = doc.blocks[0].id;
        let (_c, inv) = apply(
            &mut doc,
            BlockAction::TableInsertColumn {
                block_id: id,
                at: 2,
            },
        )
        .unwrap();
        assert_eq!(table_data(&doc).align.len(), 3);
        assert!(table_data(&doc).rows.iter().all(|r| r.cells.len() == 3));
        apply(&mut doc, inv);
        assert_eq!(table_data(&doc).align.len(), 2);
        assert!(table_data(&doc).rows.iter().all(|r| r.cells.len() == 2));
    }

    #[test]
    fn delete_column_refuses_last() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        apply(
            &mut doc,
            BlockAction::TableDeleteColumn {
                block_id: id,
                col: 0,
            },
        )
        .unwrap();
        assert_eq!(table_data(&doc).align.len(), 1);
        // Refuse to delete the last remaining column.
        assert!(apply(
            &mut doc,
            BlockAction::TableDeleteColumn {
                block_id: id,
                col: 0
            }
        )
        .is_none());
    }

    #[test]
    fn set_align_and_undo() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        let (_c, inv) = apply(
            &mut doc,
            BlockAction::TableSetAlign {
                block_id: id,
                col: 1,
                align: Align::Center,
            },
        )
        .unwrap();
        assert_eq!(table_data(&doc).align[1], Align::Center);
        apply(&mut doc, inv);
        assert_eq!(table_data(&doc).align[1], Align::None);
    }
}

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, EditorDoc};

const MAX_UNDO: usize = 100;
const COALESCE_WINDOW: Duration = Duration::from_secs(1);

struct UndoEntry {
    action: BlockAction,  // original (for redo)
    inverse: BlockAction, // computed at push-time (for undo)
}

pub struct UndoStack {
    undo: VecDeque<UndoEntry>,
    redo: Vec<UndoEntry>,
    last_inline_edit: Option<(BlockId, Instant)>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            last_inline_edit: None,
        }
    }

    /// Push an action onto the undo stack before it is applied.
    /// `doc` is the pre-apply state used to compute the inverse.
    /// For `Split`, the inverse (`MergeWithPrev`) cannot be computed from
    /// pre-state (we don't know the new block's ID yet); call
    /// `fix_split_inverse` immediately after applying.
    /// Clears the redo stack for non-inline-edit actions, or when the
    /// coalesce window expires.
    pub fn push_before_apply(&mut self, doc: &EditorDoc, action: &BlockAction) {
        let Some(inverse) = compute_inverse(doc, action) else {
            // Split: push a placeholder; caller must call fix_split_inverse.
            // OpenSlashMenu: never recorded.
            if matches!(action, BlockAction::Split { .. }) {
                let placeholder = BlockAction::MergeWithPrev {
                    block_id: BlockId::new(), // replaced by fix_split_inverse
                };
                self.redo.clear();
                self.push_entry(UndoEntry {
                    action: action.clone(),
                    inverse: placeholder,
                });
            }
            return;
        };

        if let (
            BlockAction::EditInline { block_id, .. },
            BlockAction::EditInline {
                block_id: _inv_id,
                new_runs: _old_runs,
            },
        ) = (action, &inverse)
        {
            let now = Instant::now();
            if let Some((last_id, last_t)) = self.last_inline_edit {
                if last_id == *block_id
                    && now.duration_since(last_t) < COALESCE_WINDOW
                    && self.redo.is_empty()
                {
                    // Coalesce: keep the oldest old_runs (already stored in the
                    // existing entry's inverse), update the action to the latest.
                    if let Some(entry) = self.undo.back_mut() {
                        entry.action = action.clone();
                    }
                    self.last_inline_edit = Some((*block_id, now));
                    return;
                }
            }
            self.last_inline_edit = Some((*block_id, now));
        } else {
            self.last_inline_edit = None;
        }

        self.redo.clear();
        self.push_entry(UndoEntry {
            action: action.clone(),
            inverse,
        });
    }

    /// Replace the placeholder inverse for the most recent Split entry with
    /// the real `MergeWithPrev { block_id: new_block_id }`.
    pub fn fix_split_inverse(&mut self, new_block_id: BlockId) {
        if let Some(entry) = self.undo.back_mut() {
            if matches!(entry.action, BlockAction::Split { .. }) {
                entry.inverse = BlockAction::MergeWithPrev {
                    block_id: new_block_id,
                };
            }
        }
    }

    /// After an undo recreates a block via `Split` (undoing a
    /// `MergeWithPrev`), the recreated block has a fresh `BlockId`. Update
    /// the matching redo entry's `MergeWithPrev` action to target it, so a
    /// subsequent redo merges the right block.
    pub fn fix_merge_redo(&mut self, new_block_id: BlockId) {
        if let Some(entry) = self.redo.last_mut() {
            if let BlockAction::MergeWithPrev { block_id } = &mut entry.action {
                *block_id = new_block_id;
            }
        }
    }

    /// Pop the top undo entry's inverse action (to apply as undo).
    /// Pushes the original onto the redo stack.
    pub fn pop_undo(&mut self) -> Option<BlockAction> {
        let entry = self.undo.pop_back()?;
        let inverse = entry.inverse.clone();
        self.redo.push(entry);
        Some(inverse)
    }

    /// Pop the top redo entry's original action (to re-apply as redo).
    /// Pushes back onto the undo stack.
    pub fn pop_redo(&mut self) -> Option<BlockAction> {
        let entry = self.redo.pop()?;
        let action = entry.action.clone();
        self.undo.push_back(entry);
        Some(action)
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    fn push_entry(&mut self, entry: UndoEntry) {
        if self.undo.len() == MAX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute the inverse of `action` from the pre-apply document state.
/// Returns `None` for `Split` (use `fix_split_inverse` after apply),
/// `OpenSlashMenu` (UI-only, not recorded), and first-block `Delete`
/// (no predecessor anchor available).
pub fn compute_inverse(doc: &EditorDoc, action: &BlockAction) -> Option<BlockAction> {
    match action {
        BlockAction::EditInline { block_id, .. } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let old_runs = match &block.body {
                BlockBody::Inline(runs) => runs.clone(),
                _ => return None,
            };
            Some(BlockAction::EditInline {
                block_id: *block_id,
                new_runs: old_runs,
            })
        }
        BlockAction::EditCode { block_id, .. } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let old_text = match &block.body {
                BlockBody::Code(t) => t.clone(),
                _ => return None,
            };
            Some(BlockAction::EditCode {
                block_id: *block_id,
                new_text: old_text,
            })
        }
        BlockAction::Split { .. } => None, // post-state required; handled separately
        BlockAction::MergeWithPrev { block_id } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            let prev = doc.blocks.get(idx.checked_sub(1)?)?;
            let split_offset: usize = match &prev.body {
                BlockBody::Inline(runs) => runs.iter().map(|r| r.text.len()).sum(),
                _ => return None,
            };
            Some(BlockAction::Split {
                block_id: prev.id,
                byte_offset: split_offset,
            })
        }
        BlockAction::Delete { block_id } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            // No predecessor anchor for the first block — `checked_sub` yields
            // `None`, so this whole arm returns `None`.
            let anchor = doc.blocks.get(idx.checked_sub(1)?)?.id;
            let full_block = doc.blocks.get(idx)?.clone();
            Some(BlockAction::InsertAfter {
                anchor,
                new_block: full_block,
            })
        }
        BlockAction::InsertAfter { new_block, .. } => Some(BlockAction::Delete {
            block_id: new_block.id,
        }),
        BlockAction::Move { block_id, to_index } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            // `apply_move` reads `to_index` as a gap in pre-removal
            // coordinates. To return the block to its original index `idx`:
            // a forward move (original to_index > idx) undoes with gap
            // `idx`; a backward move undoes with gap `idx + 1`.
            let inverse_to = if *to_index > idx { idx } else { idx + 1 };
            Some(BlockAction::Move {
                block_id: *block_id,
                to_index: inverse_to,
            })
        }
        BlockAction::ChangeType { block_id, .. } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let old_kind = block.kind.clone();
            Some(BlockAction::ChangeType {
                block_id: *block_id,
                new_kind: old_kind,
            })
        }
        BlockAction::EditAttrs { block_id, .. } => {
            let block = doc.blocks.iter().find(|b| b.id == *block_id)?;
            let old_attrs = block.plugin.as_ref()?.attrs.clone();
            Some(BlockAction::EditAttrs {
                block_id: *block_id,
                new_attrs: old_attrs,
            })
        }
        BlockAction::OpenSlashMenu { .. } => None,
    }
}

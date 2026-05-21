use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::actions::BlockAction;
use crate::model::types::BlockId;

const MAX_UNDO: usize = 100;
const COALESCE_WINDOW: Duration = Duration::from_secs(1);

struct UndoEntry {
    /// Canonical action with any minted ids filled in. Re-apply for redo.
    action: BlockAction,
    /// The action that, applied to the post-state, restores the pre-state.
    inverse: BlockAction,
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

    /// Record a (canonical action, inverse action) pair that the caller just
    /// obtained from `actions::apply`'s return value. Successive
    /// `EditBlockBody` actions on the same block within `COALESCE_WINDOW`
    /// collapse into one undo entry (the oldest inverse is kept, the latest
    /// action is bumped forward) — so typing N characters into a block
    /// produces one undo entry per coalesce window, not N. Clears the redo
    /// stack for non-coalescing actions.
    pub fn push_after_apply(&mut self, action: BlockAction, inverse: BlockAction) {
        // Coalesce successive EditBlockBody actions on the same block within
        // the time window. The stored inverse keeps the OLDEST body
        // (already on the existing entry); only the action is bumped.
        if let BlockAction::EditBlockBody { block_id, .. } = &action {
            let edit_id = *block_id;
            let now = Instant::now();
            if let Some((last_id, last_t)) = self.last_inline_edit {
                if last_id == edit_id
                    && now.duration_since(last_t) < COALESCE_WINDOW
                    && self.redo.is_empty()
                {
                    if let Some(entry) = self.undo.back_mut() {
                        entry.action = action;
                    }
                    self.last_inline_edit = Some((edit_id, now));
                    return;
                }
            }
            self.last_inline_edit = Some((edit_id, now));
        } else {
            self.last_inline_edit = None;
        }

        self.redo.clear();
        self.push_entry(UndoEntry { action, inverse });
    }

    /// Pop the top undo entry's inverse (to apply as undo). Moves the entry
    /// onto the redo stack so a subsequent redo re-applies the canonical
    /// action.
    pub fn pop_undo(&mut self) -> Option<BlockAction> {
        let entry = self.undo.pop_back()?;
        let inverse = entry.inverse.clone();
        self.redo.push(entry);
        Some(inverse)
    }

    /// Pop the top redo entry's canonical action (to re-apply). Moves the
    /// entry back onto the undo stack.
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

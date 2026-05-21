use std::collections::VecDeque;

use crate::actions::BlockAction;

const MAX_UNDO: usize = 100;

struct UndoEntry {
    /// Canonical action with any minted ids filled in. Re-apply for redo.
    action: BlockAction,
    /// The action that, applied to the post-state, restores the pre-state.
    inverse: BlockAction,
}

pub struct UndoStack {
    undo: VecDeque<UndoEntry>,
    redo: Vec<UndoEntry>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }

    /// Record a (canonical action, inverse action) pair that the caller just
    /// obtained from `actions::apply`'s return value. Each call is its own
    /// undo entry — there is no coalescing.
    ///
    /// (Coalescing previously merged successive `EditBlockBody` on the same
    /// block within a time window. That made sense for the stage-1
    /// per-character `EditInline` action, but `EditBlockBody` is only
    /// emitted on commit/structural boundaries now — coalescing it merged
    /// genuinely distinct user actions, e.g. a typing-commit with the
    /// Enter-split that follows it, or two consecutive Enters. Each
    /// `EditBlockBody` is a deliberate edit and gets its own entry.)
    pub fn push_after_apply(&mut self, action: BlockAction, inverse: BlockAction) {
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

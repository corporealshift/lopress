# Editor Assessment Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the nine issues from the 2026-05-15 editor assessment: scroll-to-cursor, ctrl API save routing, undo/redo, slash menu keyboard trigger, link URL input, inspector description field + title/H1 warning, navigation shortcuts, and H4–H6 toolbar buttons.

**Architecture:** All changes are in `crates/lopress-editor`. Undo is a new `src/undo.rs` module. Most other fixes are surgical edits to existing files (`inline_editor.rs`, `ui/mod.rs`, `toolbar.rs`, `inspector.rs`). No new crates; no changes to `lopress-core`.

**Tech Stack:** Rust, Floem reactive UI, `floem::reactive::RwSignal`, `lapce-xi-rope::Rope`.

**Spec:** `docs/superpowers/specs/2026-05-15-editor-assessment-fixes-design.md`

---

### Task 1: Scroll-to-cursor

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs:161-169`

- [ ] **Step 1: Add `scroll_into_view` call to the focus effect**

In `inline_editor.rs`, locate the `create_effect` that calls `view_id.request_focus()` (around line 162). Add one line immediately after:

```rust
create_effect(move |_| {
    if focus_target.get() == Some(block_id) {
        editor_sig.with_untracked(|ed| {
            if let Some(view_id) = ed.editor_view_id.get_untracked() {
                view_id.request_focus();
                view_id.scroll_into_view();  // <-- add this line
            }
        });
        focus_target.set(None);
    }
});
```

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual test**

Open a long document (20+ blocks), use Ctrl API or arrow-key down through many blocks, verify the view scrolls to keep the focused block visible.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "fix(editor): scroll view to cursor on focus change"
```

---

### Task 2: Ctrl API Save Fix

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs:380-409`

The ctrl effect currently calls `crate::actions::apply(doc, block_action)` directly, bypassing `on_action`. This means `mark_dirty()` never fires and `focus_target` isn't updated after structural actions.

- [ ] **Step 1: Clone on_action for the ctrl effect**

In `editing_view` (in `ui/mod.rs`), `on_action` is defined around line 215 and the ctrl block starts around line 382. Before the `#[cfg(debug_assertions)]` ctrl block, add:

```rust
#[cfg(debug_assertions)]
let on_action_for_ctrl = on_action.clone();
```

- [ ] **Step 2: Replace the direct apply call with on_action routing**

Replace the `create_effect` inside the ctrl block (lines ~398-408):

```rust
// BEFORE:
let action_read = create_signal_from_channel(ctrl_action_rx);
create_effect(move |_| {
    if let Some(action) = action_read.get() {
        current_doc.update(|maybe| {
            if let Some(doc) = maybe {
                if let Some(block_action) = action.into_block_action(doc) {
                    crate::actions::apply(doc, block_action);
                }
            }
        });
    }
});

// AFTER:
let action_read = create_signal_from_channel(ctrl_action_rx);
create_effect(move |_| {
    if let Some(ctrl_action) = action_read.get() {
        let block_action = current_doc
            .with_untracked(|d| ctrl_action.into_block_action(d.as_ref()?));
        if let Some(action) = block_action {
            on_action_for_ctrl(action);
        }
    }
});
```

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 4: Manual test**

With the app running, POST to `http://127.0.0.1:7878/action` with an `EditInline` action. Edit, wait 1 s, reopen the file in a text editor — the change should be persisted. Also test `Split` and `ChangeType` to verify focus moves correctly.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/mod.rs
git commit -m "fix(ctrl): route API actions through on_action so save debounce fires"
```

---

### Task 3: UndoStack — Data Structure and Inverse Computation

**Files:**
- Create: `crates/lopress-editor/src/undo.rs`
- Modify: `crates/lopress-editor/src/lib.rs` (add `pub mod undo;`)
- Create: `crates/lopress-editor/tests/undo_tests.rs`

- [ ] **Step 1: Write failing tests for compute_inverse**

Create `crates/lopress-editor/tests/undo_tests.rs`:

```rust
#![allow(clippy::unwrap_used)]

use lopress_editor::actions::BlockAction;
use lopress_editor::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun,
};
use lopress_editor::undo::compute_inverse;

fn doc_with(blocks: Vec<EditorBlock>) -> EditorDoc {
    EditorDoc { blocks, front_matter: lopress_core::FrontMatter::default() }
}

fn para(text: &str) -> EditorBlock {
    EditorBlock::paragraph(vec![InlineRun::plain(text)])
}

#[test]
fn inverse_of_edit_inline_is_old_runs() {
    let old = para("before");
    let id = old.id;
    let doc = doc_with(vec![old]);
    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("after")],
    };
    let inv = compute_inverse(&doc, &action).unwrap();
    match inv {
        BlockAction::EditInline { block_id, new_runs } => {
            assert_eq!(block_id, id);
            assert_eq!(new_runs, vec![InlineRun::plain("before")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_merge_with_prev_is_split_at_join_point() {
    let a = para("hello ");
    let b = para("world");
    let prev_id = a.id;
    let cur_id = b.id;
    let doc = doc_with(vec![a, b]);
    // "hello " is 6 bytes
    let inv = compute_inverse(&doc, &BlockAction::MergeWithPrev { block_id: cur_id }).unwrap();
    match inv {
        BlockAction::Split { block_id, byte_offset } => {
            assert_eq!(block_id, prev_id);
            assert_eq!(byte_offset, 6);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_change_type_is_change_type_with_old_kind() {
    let b = para("text");
    let id = b.id;
    let doc = doc_with(vec![b]);
    let inv = compute_inverse(
        &doc,
        &BlockAction::ChangeType { block_id: id, new_kind: BlockKind::Heading(2) },
    )
    .unwrap();
    match inv {
        BlockAction::ChangeType { block_id, new_kind } => {
            assert_eq!(block_id, id);
            assert_eq!(new_kind, BlockKind::Paragraph);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_delete_is_insert_after_with_predecessor() {
    let a = para("anchor");
    let b = para("victim");
    let anchor_id = a.id;
    let victim_id = b.id;
    let doc = doc_with(vec![a, b]);
    let inv = compute_inverse(&doc, &BlockAction::Delete { block_id: victim_id }).unwrap();
    match inv {
        BlockAction::InsertAfter { anchor, new_block } => {
            assert_eq!(anchor, anchor_id);
            assert_eq!(new_block.id, victim_id);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn inverse_of_insert_after_is_delete_new_block() {
    let a = para("anchor");
    let new_b = para("inserted");
    let new_id = new_b.id;
    let anchor_id = a.id;
    let doc = doc_with(vec![a]);
    let inv = compute_inverse(
        &doc,
        &BlockAction::InsertAfter { anchor: anchor_id, new_block: new_b },
    )
    .unwrap();
    match inv {
        BlockAction::Delete { block_id } => assert_eq!(block_id, new_id),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn undo_stack_push_and_pop() {
    use lopress_editor::undo::UndoStack;
    let a = para("text");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("edited")],
    };
    stack.push_before_apply(&doc, &action);
    lopress_editor::actions::apply(&mut doc, action);

    let undo_action = stack.pop_undo().unwrap();
    match undo_action {
        BlockAction::EditInline { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("text")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn undo_stack_redo_available_after_undo() {
    use lopress_editor::undo::UndoStack;
    let a = para("original");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let action = BlockAction::EditInline {
        block_id: id,
        new_runs: vec![InlineRun::plain("edited")],
    };
    stack.push_before_apply(&doc, &action.clone());
    lopress_editor::actions::apply(&mut doc, action);

    stack.pop_undo().unwrap();
    let redo_action = stack.pop_redo().unwrap();
    match redo_action {
        BlockAction::EditInline { new_runs, .. } => {
            assert_eq!(new_runs, vec![InlineRun::plain("edited")]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn edit_inline_within_one_second_coalesces() {
    use lopress_editor::undo::UndoStack;
    let a = para("a");
    let id = a.id;
    let mut doc = doc_with(vec![a]);
    let mut stack = UndoStack::new();

    let a1 = BlockAction::EditInline { block_id: id, new_runs: vec![InlineRun::plain("ab")] };
    stack.push_before_apply(&doc, &a1);
    lopress_editor::actions::apply(&mut doc, a1);

    let a2 = BlockAction::EditInline { block_id: id, new_runs: vec![InlineRun::plain("abc")] };
    stack.push_before_apply(&doc, &a2);
    lopress_editor::actions::apply(&mut doc, a2);

    // Should have only ONE undo entry (coalesced)
    assert_eq!(stack.undo_depth(), 1);
    let undo = stack.pop_undo().unwrap();
    match undo {
        BlockAction::EditInline { new_runs, .. } => {
            // Restores to original "a", not to intermediate "ab"
            assert_eq!(new_runs, vec![InlineRun::plain("a")]);
        }
        _ => panic!("wrong variant"),
    }
}
```

- [ ] **Step 2: Run tests — verify they fail (module doesn't exist)**

```
cargo test -p lopress-editor --test undo_tests 2>&1 | head -20
```

Expected: compile error — `module 'undo' not found`.

- [ ] **Step 3: Create `crates/lopress-editor/src/undo.rs`**

```rust
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, EditorDoc};

const MAX_UNDO: usize = 100;
const COALESCE_WINDOW: Duration = Duration::from_secs(1);

struct UndoEntry {
    action: BlockAction,   // original (for redo)
    inverse: BlockAction,  // computed at push-time (for undo)
}

pub struct UndoStack {
    undo: VecDeque<UndoEntry>,
    redo: Vec<UndoEntry>,
    last_inline_edit: Option<(BlockId, Instant)>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { undo: VecDeque::new(), redo: Vec::new(), last_inline_edit: None }
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
                self.push_entry(UndoEntry { action: action.clone(), inverse: placeholder });
            }
            return;
        };

        if let (
            BlockAction::EditInline { block_id, .. },
            BlockAction::EditInline { block_id: inv_id, new_runs: old_runs },
        ) = (action, &inverse)
        {
            let now = Instant::now();
            if let Some((last_id, last_t)) = self.last_inline_edit {
                if last_id == *block_id && now.duration_since(last_t) < COALESCE_WINDOW {
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
        self.push_entry(UndoEntry { action: action.clone(), inverse });
    }

    /// Replace the placeholder inverse for the most recent Split entry with
    /// the real `MergeWithPrev { block_id: new_block_id }`.
    pub fn fix_split_inverse(&mut self, new_block_id: BlockId) {
        if let Some(entry) = self.undo.back_mut() {
            if matches!(entry.action, BlockAction::Split { .. }) {
                entry.inverse = BlockAction::MergeWithPrev { block_id: new_block_id };
            }
        }
    }

    /// Pop the top undo entry's inverse action (to apply as undo).
    /// Pushes the original onto the redo stack.
    pub fn pop_undo(&mut self) -> Option<BlockAction> {
        let entry = self.undo.pop_back()?;
        self.redo.push(entry);
        Some(self.redo.last().unwrap().inverse.clone())
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

/// Compute the inverse of `action` from the pre-apply document state.
/// Returns `None` for `Split` (use `fix_split_inverse` after apply),
/// `OpenSlashMenu` (UI-only, not recorded), and first-block `Delete`
/// (no predecessor anchor available).
pub fn compute_inverse(doc: &EditorDoc, action: &BlockAction) -> Option<BlockAction> {
    match action {
        BlockAction::EditInline { block_id, .. } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            let old_runs = match &doc.blocks[idx].body {
                BlockBody::Inline(runs) => runs.clone(),
                _ => return None,
            };
            Some(BlockAction::EditInline { block_id: *block_id, new_runs: old_runs })
        }
        BlockAction::EditCode { block_id, .. } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            let old_text = match &doc.blocks[idx].body {
                BlockBody::Code(t) => t.clone(),
                _ => return None,
            };
            Some(BlockAction::EditCode { block_id: *block_id, new_text: old_text })
        }
        BlockAction::Split { .. } => None, // post-state required; handled separately
        BlockAction::MergeWithPrev { block_id } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            if idx == 0 {
                return None;
            }
            let prev = &doc.blocks[idx - 1];
            let split_offset: usize = match &prev.body {
                BlockBody::Inline(runs) => runs.iter().map(|r| r.text.len()).sum(),
                _ => return None,
            };
            Some(BlockAction::Split { block_id: prev.id, byte_offset: split_offset })
        }
        BlockAction::Delete { block_id } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            if idx == 0 {
                return None; // no predecessor anchor
            }
            let anchor = doc.blocks[idx - 1].id;
            let full_block = doc.blocks[idx].clone();
            Some(BlockAction::InsertAfter { anchor, new_block: full_block })
        }
        BlockAction::InsertAfter { new_block, .. } => {
            Some(BlockAction::Delete { block_id: new_block.id })
        }
        BlockAction::Move { block_id, .. } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            Some(BlockAction::Move { block_id: *block_id, to_index: idx })
        }
        BlockAction::ChangeType { block_id, .. } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            let old_kind = doc.blocks[idx].kind.clone();
            Some(BlockAction::ChangeType { block_id: *block_id, new_kind: old_kind })
        }
        BlockAction::EditAttrs { block_id, .. } => {
            let idx = doc.blocks.iter().position(|b| b.id == *block_id)?;
            let old_attrs = doc.blocks[idx].plugin.as_ref()?.attrs.clone();
            Some(BlockAction::EditAttrs { block_id: *block_id, new_attrs: old_attrs })
        }
        BlockAction::OpenSlashMenu { .. } => None,
    }
}
```

- [ ] **Step 4: Register the module in `lib.rs`**

In `crates/lopress-editor/src/lib.rs`, add after the existing `pub mod` lines:

```rust
pub mod undo;
```

- [ ] **Step 5: Run tests — verify they pass**

```
cargo test -p lopress-editor --test undo_tests
```

Expected: all 7 tests pass.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/undo.rs crates/lopress-editor/src/lib.rs crates/lopress-editor/tests/undo_tests.rs
git commit -m "feat(undo): add UndoStack and compute_inverse with tests"
```

---

### Task 4: Undo/Redo Full Wiring

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs` (wire stack into on_action; create on_undo/on_redo closures)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (thread on_undo/on_redo)
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (thread on_undo/on_redo)
- Modify: `crates/lopress-editor/src/ui/blocks/paragraph.rs` (thread on_undo/on_redo)
- Modify: `crates/lopress-editor/src/ui/blocks/heading.rs` (thread on_undo/on_redo)
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (add on_undo/on_redo params, intercept Ctrl+Z/Y)

No unit tests for this task — the wiring is UI-only. Verify manually.

- [ ] **Step 1: Add UndoStack signal and on_undo/on_redo closures to `editing_view` in `ui/mod.rs`**

After the `let mark_dirty: Rc<dyn Fn()> = ...` block (around line 207), add:

```rust
use crate::undo::UndoStack;
let undo_stack: RwSignal<UndoStack> = RwSignal::new(UndoStack::new());
```

In the `on_open` callback (around line 161), clear the undo stack when a new document opens. Add at the end of the closure body:

```rust
undo_stack.update(|s| *s = UndoStack::new());
```

- [ ] **Step 2: Push to undo stack in `on_action`**

Inside the `on_action` closure (starting around line 215), add undo tracking. The `on_action` closure currently:
1. Handles `OpenSlashMenu` early-return
2. Computes `pre_focus`
3. Calls `current_doc.update(...)` to apply
4. Computes `post_focus` + `change_type_focus`
5. Schedules focus
6. Calls `on_action_mark_dirty()`

Insert undo push **between steps 1 and 2**, and a `fix_split_inverse` call **between steps 3 and 4**:

```rust
let on_action: ActionSink = Rc::new(move |action: BlockAction| {
    if let BlockAction::OpenSlashMenu { block_id } = action {
        slash_menu_open.set(Some(block_id));
        return;
    }
    if slash_menu_open.get_untracked().is_some() {
        slash_menu_open.set(None);
    }

    // ── Undo push (pre-apply) ────────────────────────────────────────────
    undo_stack.update(|s| s.push_before_apply(
        &current_doc.with_untracked(|d| d.clone()).unwrap_or_default_doc(),
        &action,
    ));
    // Note: unwrap_or_default_doc doesn't exist — use the pattern below.

    // ... (keep existing pre_focus, apply, post_focus logic)

    // ── Fix Split inverse (post-apply) ───────────────────────────────────
    if matches!(&action, BlockAction::Split { .. }) {
        // After apply, find the new block (the one after block_id in doc).
        // ... (see full code below)
    }
```

**Full replacement for the `on_action` closure body** — copy this exactly, replacing the existing closure body:

```rust
let on_action: ActionSink = Rc::new(move |action: BlockAction| {
    if let BlockAction::OpenSlashMenu { block_id } = action {
        slash_menu_open.set(Some(block_id));
        return;
    }
    if slash_menu_open.get_untracked().is_some() {
        slash_menu_open.set(None);
    }

    // Push to undo stack before apply (using pre-state).
    let pre_doc_snapshot = current_doc.with_untracked(|d| d.clone());
    if let Some(ref doc) = pre_doc_snapshot {
        undo_stack.update(|s| s.push_before_apply(doc, &action));
    }

    let pre_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
        (BlockAction::MergeWithPrev { block_id }, Some(d)) => d
            .blocks
            .iter()
            .position(|b| b.id == *block_id)
            .filter(|&i| i > 0)
            .and_then(|i| d.blocks.get(i - 1))
            .map(|b| b.id),
        _ => None,
    });
    let action_for_apply = action.clone();
    current_doc.update(|maybe| {
        if let Some(d) = maybe {
            apply(d, action_for_apply);
        }
    });

    // Fix Split inverse now that post-state has the new block id.
    if let BlockAction::Split { block_id, .. } = &action {
        let new_id = current_doc.with_untracked(|maybe| {
            let d = maybe.as_ref()?;
            let i = d.blocks.iter().position(|b| b.id == *block_id)?;
            d.blocks.get(i + 1).map(|b| b.id)
        });
        if let Some(new_id) = new_id {
            undo_stack.update(|s| s.fix_split_inverse(new_id));
        }
    }

    let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
        (BlockAction::Split { block_id, .. }, Some(d)) => d
            .blocks
            .iter()
            .position(|b| b.id == *block_id)
            .and_then(|i| d.blocks.get(i + 1))
            .map(|b| b.id),
        _ => None,
    });
    let change_type_focus = match &action {
        BlockAction::ChangeType { block_id, .. } => Some(*block_id),
        _ => None,
    };
    if let Some(id) = pre_focus.or(post_focus).or(change_type_focus) {
        floem::action::exec_after(Duration::from_millis(0), move |_| {
            focus_target.set(Some(id));
        });
    }
    on_action_mark_dirty();
});
```

- [ ] **Step 3: Create on_undo and on_redo closures in `editing_view`**

After the `on_action` definition, add:

```rust
let on_undo: Rc<dyn Fn()> = {
    let current_doc = current_doc;
    let undo_stack = undo_stack;
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let inv = undo_stack.with_untracked(|s| s.undo_depth() > 0)
            .then(|| undo_stack.update_returning(|s| s.pop_undo()))
            .flatten();
        // Note: RwSignal doesn't have update_returning; use a local:
        if let Some(inv_action) = undo_stack.try_update(|s| s.pop_undo()).flatten() {
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    apply(d, inv_action);
                }
            });
            mark_dirty();
        }
    })
};
```

Wait — `RwSignal::try_update` returns `Option<O>` where `O` is the closure return type. Use it as:

```rust
let on_undo: Rc<dyn Fn()> = {
    let current_doc = current_doc;
    let undo_stack = undo_stack;
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let inv_action = undo_stack.try_update(|s| s.pop_undo()).flatten();
        if let Some(action) = inv_action {
            current_doc.update(|maybe| {
                if let Some(d) = maybe { apply(d, action); }
            });
            mark_dirty();
        }
    })
};

let on_redo: Rc<dyn Fn()> = {
    let current_doc = current_doc;
    let undo_stack = undo_stack;
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let action = undo_stack.try_update(|s| s.pop_redo()).flatten();
        if let Some(action) = action {
            current_doc.update(|maybe| {
                if let Some(d) = maybe { apply(d, action); }
            });
            mark_dirty();
        }
    })
};
```

- [ ] **Step 4: Thread on_undo/on_redo through editor_pane**

In `editor_pane.rs`, update the `editor_pane` function signature and forward the callbacks to `block_view`:

```rust
pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,   // <-- add
    on_redo: Rc<dyn Fn()>,   // <-- add
) -> impl IntoView {
```

In the loop that calls `block_view`, add the new params:

```rust
rows.push(block_view(
    b,
    on_action.clone(),
    focus_target,
    focus_pub,
    dnd,
    current_doc,
    Rc::clone(&on_undo),  // <-- add
    Rc::clone(&on_redo),  // <-- add
));
```

Update the call site in `ui/mod.rs` (the `dyn_container` pane_key block) to pass `on_undo.clone(), on_redo.clone()` as the last two arguments to `editor_pane::editor_pane(...)`.

Add `use std::rc::Rc;` to `editor_pane.rs` imports if not present.

- [ ] **Step 5: Thread on_undo/on_redo through block_view**

In `blocks/mod.rs`, update `block_view` signature:

```rust
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,   // <-- add
    on_redo: Rc<dyn Fn()>,   // <-- add
) -> AnyView {
```

Forward to `render_paragraph_editable` and `render_heading_editable`:

```rust
(BlockKind::Paragraph, BlockBody::Inline(runs)) => {
    paragraph::render_paragraph_editable(
        runs, block.id, on_action.clone(), focus_target,
        focus_pub, current_doc,
        Rc::clone(&on_undo), Rc::clone(&on_redo),  // <-- add
    )
    .style(|s| s.padding_vert(6.))
    .into_any()
}
(BlockKind::Heading(level), BlockBody::Inline(runs)) => {
    heading::render_heading_editable(
        *level, runs, block.id, on_action.clone(), focus_target,
        focus_pub, current_doc,
        Rc::clone(&on_undo), Rc::clone(&on_redo),  // <-- add
    )
    .into_any()
}
```

Add `use std::rc::Rc;` to imports in `mod.rs`.

- [ ] **Step 6: Thread on_undo/on_redo through paragraph and heading**

In `paragraph.rs`, update `render_paragraph_editable`:

```rust
pub fn render_paragraph_editable(
    runs: &[InlineRun],
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,   // <-- add
    on_redo: Rc<dyn Fn()>,   // <-- add
) -> impl IntoView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, BODY_FONT_SIZE as usize);
    editable_inline(state, block_id, on_action, focus_target, focus_pub,
                    current_doc, true, on_undo, on_redo)
}
```

In `heading.rs`, do the same for `render_heading_editable` (look at the existing signature and add `on_undo: Rc<dyn Fn()>` and `on_redo: Rc<dyn Fn()>` as the last two params, forward them to `editable_inline`).

- [ ] **Step 7: Add on_undo/on_redo params to editable_inline and handle_key**

In `inline_editor.rs`, update `editable_inline`:

```rust
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    _slash_eligible: bool,
    on_undo: Rc<dyn Fn()>,   // <-- add
    on_redo: Rc<dyn Fn()>,   // <-- add
) -> impl IntoView {
```

Thread them into `handle_key`:

```rust
let result = handle_key(
    kp, ms, editor_sig, spans_sig, style_rev,
    block_id, &on_action_for_key, focus_target, current_doc,
    &on_undo, &on_redo,  // <-- add
);
```

Update `handle_key` signature:

```rust
fn handle_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: &Rc<dyn Fn()>,   // <-- add
    on_redo: &Rc<dyn Fn()>,   // <-- add
) -> CommandExecuted {
```

In `handle_key`, inside the `if ctrl_or_cmd` block, add undo/redo intercepts **before** the `match s.as_str()` on B/I/E/K:

```rust
if ctrl_or_cmd {
    if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
        match s.as_str() {
            "z" | "Z" if !ms.shift() => { on_undo(); return CommandExecuted::Yes; }
            "y" | "Y" => { on_redo(); return CommandExecuted::Yes; }
            "z" | "Z" if ms.shift() => { on_redo(); return CommandExecuted::Yes; }
            "b" | "B" => { apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Bold); return CommandExecuted::Yes; }
            "i" | "I" => { apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Italic); return CommandExecuted::Yes; }
            "e" | "E" => { apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Code); return CommandExecuted::Yes; }
            "k" | "K" => { apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Link); return CommandExecuted::Yes; }
            _ => {}
        }
    }
    return CommandExecuted::No;
}
```

Note: `"z" | "Z" if !ms.shift()` — Rust `match` guards apply to each arm, and `|` patterns share the guard. Write as two separate arms to be safe:

```rust
"z" | "Z" => {
    if ms.shift() { on_redo(); } else { on_undo(); }
    return CommandExecuted::Yes;
}
"y" | "Y" => { on_redo(); return CommandExecuted::Yes; }
```

- [ ] **Step 8: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 9: Manual test**

1. Open a document, type some text, press Ctrl+Z — text should revert.
2. Press Ctrl+Y / Ctrl+Shift+Z — text re-applies.
3. Split a block with Enter, then Ctrl+Z — the split should be undone (blocks merge).
4. Delete a block, Ctrl+Z — block reappears.

- [ ] **Step 10: Commit**

```
git add crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/src/ui/editor_pane.rs \
        crates/lopress-editor/src/ui/blocks/mod.rs \
        crates/lopress-editor/src/ui/blocks/paragraph.rs \
        crates/lopress-editor/src/ui/blocks/heading.rs \
        crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "feat(undo): wire Ctrl+Z/Y undo-redo through the editor"
```

---

### Task 5: Slash Menu Keyboard Trigger

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

The `_slash_eligible` parameter is already threaded to `editable_inline` (it was passed as `true` for Paragraph). The `_` prefix blocks its use. This task wires the `/` key interception.

- [ ] **Step 1: Remove the underscore prefix from the parameter**

In `editable_inline`, the parameter is currently `_slash_eligible: bool`. Change it to `slash_eligible: bool`.

The call to `handle_key` currently passes the block-level state. Thread `slash_eligible` into it.

Update `handle_key` signature to add:

```rust
slash_eligible: bool,
```

Update the call site in `editable_inline`:

```rust
let result = handle_key(
    kp, ms, editor_sig, spans_sig, style_rev,
    block_id, &on_action_for_key, focus_target, current_doc,
    &on_undo, &on_redo, slash_eligible,
);
```

- [ ] **Step 2: Add the `/` intercept in handle_key**

In `handle_key`, after the `if ctrl_or_cmd { ... return CommandExecuted::No; }` block, and before the `match &kp.key {` block, add:

```rust
// Slash command trigger: `/` on an empty Paragraph block.
if !ctrl_or_cmd && !ms.shift() {
    if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
        if s == "/" && slash_eligible {
            let is_empty =
                editor_sig.with_untracked(|ed| ed.doc().text().len() == 0);
            if is_empty {
                on_action(BlockAction::OpenSlashMenu { block_id });
                return CommandExecuted::Yes;
            }
        }
    }
}
```

- [ ] **Step 3: Set slash_eligible correctly for Headings**

In `heading.rs`, `render_heading_editable` passes `slash_eligible` to `editable_inline`. Heading blocks should pass `false`:

```rust
editable_inline(state, block_id, on_action, focus_target, focus_pub,
                current_doc, false, on_undo, on_redo)
//                           ^^^^^ was true
```

Paragraph stays `true` (already the case).

- [ ] **Step 4: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 5: Manual test**

1. Focus an empty Paragraph block, press `/` — the slash menu should appear.
2. Focus a non-empty Paragraph block, press `/` — character is typed normally.
3. Focus a Heading block, press `/` on empty content — character typed (not menu).

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs \
        crates/lopress-editor/src/ui/blocks/heading.rs
git commit -m "feat(editor): wire slash-menu keyboard trigger for empty paragraph blocks"
```

---

### Task 6: Link URL Input Row

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (add `link_url_sig` to `BlockEditorState`)
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (pass `link_url_sig` to toolbar slot)
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (add URL row below the button row)
- Modify: `crates/lopress-editor/src/model/style_span.rs` (inspect for link URL from selection)

`BlockEditorState` needs a `link_url_sig: RwSignal<Option<String>>` that the toolbar can read to show/hide the URL input. The Ctrl+K handler sets the URL to `Some("")` when toggling a link on.

- [ ] **Step 1: Add link_url_sig to BlockEditorState**

In `inline_editor.rs`, update `BlockEditorState`:

```rust
pub struct BlockEditorState {
    pub editor_sig: RwSignal<Editor>,
    pub spans_sig: RwSignal<Vec<StyleSpan>>,
    pub style_rev: RwSignal<u64>,
    pub text_sig: RwSignal<String>,
    pub link_url_sig: RwSignal<Option<String>>,  // <-- add
}
```

In `build_block_editor`, add:

```rust
let link_url_sig = cx.create_rw_signal(None::<String>);
// ... at the end of the function, include in struct:
BlockEditorState {
    editor_sig,
    spans_sig,
    style_rev,
    text_sig,
    link_url_sig,  // <-- add
}
```

- [ ] **Step 2: Add link_url_sig to FocusPublisher**

`FocusPublisher.editor_and_spans` currently holds `Option<(RwSignal<Editor>, RwSignal<Vec<StyleSpan>>, RwSignal<u64>)>`. Extend it to include `link_url_sig`:

```rust
pub editor_and_spans:
    RwSignal<Option<(RwSignal<Editor>, RwSignal<Vec<StyleSpan>>, RwSignal<u64>, RwSignal<Option<String>>)>>,
```

Update the publish effect in `editable_inline`:

```rust
focus_pub.editor_and_spans.set(Some((
    editor_sig,
    spans_sig,
    style_rev,
    state.link_url_sig,  // <-- add (capture state before moving)
)));
```

Capture `link_url_sig` before the effect via `let link_url_sig = state.link_url_sig;` at the top of `editable_inline`.

Update every destructure of `editor_and_spans` in `toolbar.rs` to add the fourth element.

- [ ] **Step 3: Update Ctrl+K handler to populate link_url_sig**

In `handle_key`, the `"k" | "K"` arm currently calls `apply_style_toggle(..., InlineFlag::Link)`. After toggling, determine if a link span is now active and set `link_url_sig`:

```rust
"k" | "K" => {
    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Link);
    // After toggle: check if selection now has a link; if so, open URL row.
    let has_link = selection_has_link(editor_sig, spans_sig);
    link_url_sig.set(if has_link { Some(String::new()) } else { None });
    return CommandExecuted::Yes;
}
```

Add helper function at the bottom of `inline_editor.rs`:

```rust
fn selection_has_link(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
) -> bool {
    use floem::views::editor::core::cursor::CursorMode;
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(o) => (*o, *o),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    spans_sig.with_untracked(|spans| {
        spans.iter().any(|s| {
            let lo = s.start.max(sel_start);
            let hi = s.end.min(sel_end);
            lo < hi && s.link.is_some()
        })
    })
}
```

- [ ] **Step 4: Add URL row to the toolbar**

In `toolbar.rs`, update `block_toolbar_for` to take `on_action` (already present) and return a `v_stack` of two rows instead of a single `h_stack`.

Update the function to return `impl IntoView` that is:

```rust
let button_row = h_stack_from_iter(buttons).style(|s| {
    s.padding_horiz(6.)
        .padding_vert(4.)
        .gap(4.)
        .background(Color::rgb8(245, 245, 248))
        .border(1.)
        .border_color(Color::rgb8(220, 220, 226))
        .border_radius(4.)
        .margin_bottom(4.)
});

let url_row = dyn_container(
    move || focus_pub.editor_and_spans.get().and_then(|(_, _, _, url)| url.get()),
    move |maybe_url| match maybe_url {
        None => empty().into_any(),
        Some(current_url) => {
            // Local buffer for the text input
            let url_buf: RwSignal<String> = RwSignal::new(current_url);
            let on_action_commit = on_action.clone();
            let commit = move || {
                let url = url_buf.get_untracked();
                // Write the URL into all link spans in the selection.
                if let Some((editor_sig, spans_sig, _, url_sig)) =
                    focus_pub.editor_and_spans.get_untracked()
                {
                    // Update link spans in the current selection to have the typed URL.
                    write_url_to_selection(editor_sig, spans_sig, &url);
                    // Commit the updated runs to the doc.
                    let text = editor_sig
                        .with_untracked(|ed| String::from(&ed.doc().text()));
                    let spans = spans_sig.get_untracked();
                    let rope = lapce_xi_rope::Rope::from(text.as_str());
                    let new_runs =
                        crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                    on_action_commit(BlockAction::EditInline { block_id, new_runs });
                    url_sig.set(None); // close the URL row
                }
            };
            let commit2 = commit.clone();
            let on_action_remove = on_action.clone();
            let remove = move || {
                if let Some((editor_sig, spans_sig, style_rev, url_sig)) =
                    focus_pub.editor_and_spans.get_untracked()
                {
                    crate::ui::blocks::inline_editor::apply_style_toggle(
                        editor_sig, spans_sig, style_rev, InlineFlag::Link,
                    );
                    url_sig.set(None);
                    // Commit updated runs
                    let text = editor_sig
                        .with_untracked(|ed| String::from(&ed.doc().text()));
                    let spans = spans_sig.get_untracked();
                    let rope = lapce_xi_rope::Rope::from(text.as_str());
                    let new_runs =
                        crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                    on_action_remove(BlockAction::EditInline { block_id, new_runs });
                }
            };
            use floem::views::{button, text_input, h_stack, label};
            h_stack((
                text_input(url_buf)
                    .placeholder("https://…")
                    .on_event_stop(
                        floem::event::EventListener::KeyDown,
                        move |e: &floem::event::Event| {
                            use floem::keyboard::{Key, NamedKey};
                            if let floem::event::Event::KeyDown(k) = e {
                                if matches!(k.key.logical_key, Key::Named(NamedKey::Enter)) {
                                    commit();
                                }
                            }
                        },
                    )
                    .style(|s| s.flex_grow(1.0).font_size(13.)),
                button(label(|| "Remove".to_string())).action(move || remove()),
            ))
            .style(|s| s.gap(4.).width_full().padding_vert(4.))
            .into_any()
        }
    },
)
.style(|s| s.width_full());

floem::views::v_stack((button_row, url_row))
    .style(|s| s.width_full())
```

Add helper in `toolbar.rs`:

```rust
fn write_url_to_selection(
    editor_sig: RwSignal<floem::views::editor::Editor>,
    spans_sig: RwSignal<Vec<crate::model::style_span::StyleSpan>>,
    url: &str,
) {
    use floem::views::editor::core::cursor::CursorMode;
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(o) => (*o, *o),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    let url_owned = url.to_owned();
    spans_sig.update(|spans| {
        for span in spans.iter_mut() {
            let lo = span.start.max(sel_start);
            let hi = span.end.min(sel_end);
            if lo < hi && span.link.is_some() {
                span.link = Some(url_owned.clone());
            }
        }
    });
}
```

- [ ] **Step 5: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors. Fix any type mismatches in the `editor_and_spans` destructures.

- [ ] **Step 6: Manual test**

1. Select some text, press Ctrl+K — URL row should appear below the toolbar.
2. Type a URL, press Enter — URL row closes, link is saved.
3. Click into the linked text — URL row should not re-appear automatically (it only opens on Ctrl+K).
4. Select linked text, Ctrl+K again to toggle off — link is removed, URL row closes.

- [ ] **Step 7: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs \
        crates/lopress-editor/src/ui/blocks/mod.rs \
        crates/lopress-editor/src/ui/toolbar.rs
git commit -m "feat(editor): add link URL input row to toolbar"
```

---

### Task 7: Inspector — Description Field and Title/H1 Divergence Warning

**Files:**
- Modify: `crates/lopress-editor/src/ui/inspector.rs`

`FrontMatter` in `lopress-core` already has `description: Option<String>`. No changes needed to `lopress-core`.

- [ ] **Step 1: Write a failing test for the inspector form — description round-trip**

These are UI components with no unit-testable logic separate from the reactive system. Skip automated tests; verify manually in Step 5.

- [ ] **Step 2: Add description field buffer to `form` in `inspector.rs`**

In the `form` function, after `let draft_sig`:

```rust
let desc_buf: RwSignal<String> =
    RwSignal::new(fm.description.clone().unwrap_or_default());
```

Add the write-back effect (same pattern as other fields):

```rust
let md = mark_dirty.clone();
create_effect(move |_| {
    let new_desc = desc_buf.get();
    let mut changed = false;
    current_doc.update(|maybe| {
        if let Some(d) = maybe {
            let next = if new_desc.is_empty() { None } else { Some(new_desc.clone()) };
            if d.front_matter.description != next {
                d.front_matter.description = next;
                changed = true;
            }
        }
    });
    if changed { md(); }
});
```

- [ ] **Step 3: Add title/H1 divergence derived signal**

After the effect blocks, add:

```rust
let h1_text: floem::reactive::Memo<Option<String>> =
    floem::reactive::create_memo(move |_| {
        current_doc.with(|maybe| {
            let d = maybe.as_ref()?;
            let h1 = d.blocks.iter().find(|b| b.kind == crate::model::types::BlockKind::Heading(1))?;
            match &h1.body {
                crate::model::types::BlockBody::Inline(runs) => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                }
                _ => None,
            }
        })
    });

let title_h1_mismatch = floem::reactive::create_memo(move |_| {
    let title = current_doc.with(|d| {
        d.as_ref().and_then(|d| d.front_matter.title.clone())
    });
    let h1 = h1_text.get();
    matches!((title, h1), (Some(t), Some(h)) if t != h)
});
```

- [ ] **Step 4: Add field widgets**

In the `v_stack((...))` at the end of `form`, add after `title_field`:

```rust
// Title/H1 divergence warning
let h1_text_for_sync = h1_text;
let on_action_for_sync = mark_dirty.clone();
let current_doc_for_sync = current_doc;
let warning_row = dyn_container(
    move || title_h1_mismatch.get(),
    move |mismatch| {
        if !mismatch {
            return empty().into_any();
        }
        let on_sync = {
            let current_doc = current_doc_for_sync;
            let mark_dirty = on_action_for_sync.clone();
            let title_buf_for_sync = title_buf;
            move || {
                if let Some(text) = h1_text_for_sync.get_untracked() {
                    title_buf_for_sync.set(text.clone());
                    current_doc.update(|maybe| {
                        if let Some(d) = maybe {
                            d.front_matter.title = Some(text.clone());
                        }
                    });
                    mark_dirty();
                }
            }
        };
        h_stack((
            label(|| "⚠ Title differs from H1".to_string())
                .style(|s| s.font_size(11.).color(ERR_FG).flex_grow(1.0)),
            button(label(|| "Sync from H1".to_string()))
                .action(on_sync)
                .style(|s| s.font_size(11.).padding_horiz(6.).padding_vert(2.)),
        ))
        .style(|s| s.gap(4.).width_full())
        .into_any()
    },
)
.style(|s| s.width_full());

// Description field
let desc_field = field_row(
    "Description",
    text_input(desc_buf)
        .placeholder("Short excerpt or summary")
        .style(input_style)
        .into_any(),
);
```

Update the final `v_stack`:

```rust
v_stack((
    label(|| "Front matter".to_string()).style(|s| { ... }),
    title_field,
    warning_row,   // <-- add after title_field
    slug_field,
    date_field,
    tags_field,
    draft_field,
    desc_field,    // <-- add after draft_field
))
```

Add `use floem::views::{button, h_stack, ...}` if not already imported. Also add `use floem::reactive::create_memo;`.

- [ ] **Step 5: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 6: Manual test**

1. Open a document that has both a front-matter title and an H1 block with different text — the `⚠ Title differs from H1` warning should appear.
2. Click "Sync from H1" — the title field updates, warning disappears.
3. Type in the Description field, save, reopen — description is persisted in front-matter.

- [ ] **Step 7: Commit**

```
git add crates/lopress-editor/src/ui/inspector.rs
git commit -m "feat(inspector): add description field and title/H1 divergence warning"
```

---

### Task 8: Navigation Shortcuts

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

Add `Ctrl+Home`, `Ctrl+End`, `Page Up`, `Page Down` handling in `handle_key`. All four commit the current block's runs first, then jump focus via `focus_target`.

- [ ] **Step 1: Add navigation cases to handle_key**

In `handle_key`, inside the `if ctrl_or_cmd { ... }` block, add before the trailing `return CommandExecuted::No;`:

```rust
if let KeyInput::Keyboard(Key::Named(NamedKey::Home), _) = kp.key {
    // Ctrl+Home: jump to first block
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let first_id = current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
    if let Some(id) = first_id {
        focus_target.set(Some(id));
    }
    return CommandExecuted::Yes;
}
if let KeyInput::Keyboard(Key::Named(NamedKey::End), _) = kp.key {
    // Ctrl+End: jump to last block
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let last_id = current_doc.with_untracked(|d| d.as_ref()?.blocks.last().map(|b| b.id));
    if let Some(id) = last_id {
        focus_target.set(Some(id));
    }
    return CommandExecuted::Yes;
}
```

Outside the `if ctrl_or_cmd` block (so these fire without modifiers), add two more arms to the `match &kp.key` block:

```rust
// Page Up: jump 10 blocks back (clamped to 0)
KeyInput::Keyboard(Key::Named(NamedKey::PageUp), _) => {
    let target_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        let j = i.saturating_sub(10);
        d.blocks.get(j).map(|b| b.id)
    });
    if let Some(id) = target_id {
        commit_from_editor(editor_sig, spans_sig, block_id, on_action);
        focus_target.set(Some(id));
    }
    CommandExecuted::Yes
}

// Page Down: jump 10 blocks forward (clamped to last)
KeyInput::Keyboard(Key::Named(NamedKey::PageDown), _) => {
    let target_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        let j = (i + 10).min(d.blocks.len().saturating_sub(1));
        d.blocks.get(j).map(|b| b.id)
    });
    if let Some(id) = target_id {
        commit_from_editor(editor_sig, spans_sig, block_id, on_action);
        focus_target.set(Some(id));
    }
    CommandExecuted::Yes
}
```

- [ ] **Step 2: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual test**

1. Open a long document (20+ blocks), focus block 5, press `Page Down` — focus jumps to block 15.
2. Press `Ctrl+End` — focus jumps to the last block.
3. Press `Ctrl+Home` — focus jumps to the first block.
4. Press `Page Up` from block 5 — jumps to block 0 (clamped).

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "feat(editor): add Ctrl+Home/End and Page Up/Down navigation shortcuts"
```

---

### Task 9: H4–H6 Toolbar Buttons

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`

- [ ] **Step 1: Append H4, H5, H6 to the kinds vec**

In `block_toolbar_for`, the `kinds` vec currently ends with `("OL", BlockKind::List { ordered: true })`. Append:

```rust
let kinds: Vec<(&'static str, BlockKind)> = vec![
    ("P", BlockKind::Paragraph),
    ("H1", BlockKind::Heading(1)),
    ("H2", BlockKind::Heading(2)),
    ("H3", BlockKind::Heading(3)),
    (
        "Code",
        BlockKind::Code { lang: String::new() },
    ),
    ("UL", BlockKind::List { ordered: false }),
    ("OL", BlockKind::List { ordered: true }),
    ("H4", BlockKind::Heading(4)),   // <-- add
    ("H5", BlockKind::Heading(5)),   // <-- add
    ("H6", BlockKind::Heading(6)),   // <-- add
];
```

- [ ] **Step 2: Compile**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Manual test**

Open a document, focus a block — the toolbar should show H4, H5, H6 buttons. Click H4 — block changes to Heading level 4.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "feat(toolbar): expose H4, H5, H6 block-type buttons"
```

---

## Self-Review

**Spec coverage:**

| Spec item | Covered by |
|---|---|
| #1 Scroll-to-cursor | Task 1 |
| #2 Ctrl API save fix | Task 2 |
| #3 Slash menu keyboard trigger | Task 5 |
| #4 Link URL tooltip | Task 6 |
| #6 Undo/redo | Tasks 3 + 4 |
| #7 Navigation shortcuts | Task 8 |
| #8 Title/H1 divergence warning | Task 7 |
| #9 Description field | Task 7 |
| #10 H4–H6 toolbar | Task 9 |

**Implementation order** matches the spec: 1 → 2 → 6 (undo) → 3 (slash) → 4 (link) → 8+9 (inspector) → 7 (nav) → 10 (H4-H6).

**Type consistency notes:**
- `BlockEditorState.link_url_sig` added in Task 6 Step 1; used in Task 6 Steps 2–4.
- `FocusPublisher.editor_and_spans` extended to 4-tuple in Task 6 Step 2; all destructures in `toolbar.rs` must be updated.
- `on_undo`/`on_redo` params added in Task 4 and forwarded consistently through the chain.
- `UndoStack::pop_undo` and `pop_redo` return `Option<BlockAction>` (not the entry); callers must not touch the entry directly.

**Placeholder scan:** No TBDs found. The `write_url_to_selection` helper and `selection_has_link` helper are fully defined in their respective tasks.

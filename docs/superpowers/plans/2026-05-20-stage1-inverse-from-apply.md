# Stage 1: Inverse-from-Apply Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move undo-inverse computation from `undo::compute_inverse` into the apply arms themselves, so `apply` returns the inverse it just performed. Eliminate the four post-apply fix-up methods by adding `new_block_id: Option<BlockId>` to `Split` and `SplitListItem` for id-stable undo↔redo.

**Architecture:** `apply(doc, action) -> Option<(BlockAction /* canonical */, BlockAction /* inverse */)>`. Each apply arm reads pre-state, mutates, and returns both the canonicalized action (with newly-minted ids filled in for `Split`/`SplitListItem`) and the action that would restore the prior state. The undo stack stores the (canonical_action, inverse) pair; undo and redo just shuttle entries between stacks, applying the relevant half. `compute_inverse`, `fix_split_inverse`, `fix_split_list_item_inverse`, `fix_merge_redo`, and `fix_merge_list_item_redo` all disappear.

**Tech Stack:** Rust, Floem reactive signals, existing `lopress-editor` crate.

**Spec:** [`docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`](../specs/2026-05-20-list-editor-unification-and-generic-undo-design.md) (Stage 1 = "Shift A" in section 3).

**Scope:** This plan covers **stage 1 only**. Stages 2–6 (`EditBlockBody` collapse, paragraph/heading/code migration, list unification + caret/data-loss/Ctrl-shortcut fixes, ctrl API translation, cleanup) get their own plans after stage 1 lands cleanly. Behavior is preserved end-to-end; only internals change.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/lopress-editor/src/actions.rs` | `BlockAction` enum + `apply`. After this stage: `Split` and `SplitListItem` carry `new_block_id: Option<BlockId>`; `apply` returns `Option<(BlockAction, BlockAction)>`; each arm computes its own inverse inline. |
| `crates/lopress-editor/src/undo.rs` | `UndoStack`. After this stage: `compute_inverse` deleted; `push_before_apply` renamed to `push_after_apply(action, inverse)`; the four `fix_*` methods deleted; placeholder-inverse handling deleted. |
| `crates/lopress-editor/src/ui/mod.rs` | Chokepoint (`on_action`), `on_undo`, `on_redo`. After this stage: chokepoint calls `apply` first, captures `(canonical, inverse)`, then calls `push_after_apply`. The fix-up call blocks in all three places are deleted. |
| `crates/lopress-editor/tests/actions_tests.rs` | Existing apply-behavior tests. Updated for the new `Split.new_block_id` field; new tests for `apply`'s return value across each variant. |
| `crates/lopress-editor/tests/undo_tests.rs` | Existing undo-stack tests. Updated for `push_after_apply`; new id-stability tests for `Split`/`SplitListItem` undo↔redo round-trips. |
| `crates/lopress-editor/tests/list_action_tests.rs` | List-specific action tests. Updated for the new `SplitListItem.new_block_id` field; updated for new `apply` signature. |

No new files. No new public API. No serialization-format change (`BlockAction` is not serialized to disk; it's an in-memory enum on the action channel).

---

## Task 1: Add `new_block_id: Option<BlockId>` to `Split`

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs:14-90` (enum), `crates/lopress-editor/src/actions.rs:152-212` (`apply_split`)
- Modify: `crates/lopress-editor/src/ui/mod.rs` (all `BlockAction::Split { ... }` emit sites)
- Modify: `crates/lopress-editor/src/undo.rs` (the `Split` arm in `compute_inverse` and the `fix_split_inverse` placeholder construction)
- Modify: `crates/lopress-editor/src/ctrl/mod.rs:75-80` (`CtrlAction::Split` → `BlockAction::Split` translation)
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (where the Enter handler builds `BlockAction::Split`)
- Test: `crates/lopress-editor/tests/actions_tests.rs`

- [ ] **Step 1: Write a failing test for `Split { new_block_id: Some(known_id) }`**

Add to `crates/lopress-editor/tests/actions_tests.rs`:

```rust
#[test]
fn split_with_new_block_id_uses_provided_id() {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::{BlockId, EditorBlock, EditorDoc, InlineRun};

    let target_id = BlockId::new();
    let mut doc = EditorDoc::default();
    let a_id = BlockId::new();
    doc.blocks.push({
        let mut b = EditorBlock::paragraph(vec![InlineRun::plain("hello world")]);
        b.id = a_id;
        b
    });

    apply(
        &mut doc,
        BlockAction::Split {
            block_id: a_id,
            byte_offset: 5,
            new_block_id: Some(target_id),
        },
    );

    assert_eq!(doc.blocks.len(), 2, "split must produce two blocks");
    assert_eq!(
        doc.blocks[1].id, target_id,
        "the new block must use the provided id, not a freshly minted one"
    );
}

#[test]
fn split_with_new_block_id_none_mints_fresh_id() {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::{BlockId, EditorBlock, EditorDoc, InlineRun};

    let mut doc = EditorDoc::default();
    let a_id = BlockId::new();
    doc.blocks.push({
        let mut b = EditorBlock::paragraph(vec![InlineRun::plain("hello world")]);
        b.id = a_id;
        b
    });

    apply(
        &mut doc,
        BlockAction::Split {
            block_id: a_id,
            byte_offset: 5,
            new_block_id: None,
        },
    );

    assert_eq!(doc.blocks.len(), 2);
    assert_ne!(doc.blocks[1].id, a_id, "fresh id must differ from the original block");
}
```

- [ ] **Step 2: Run the new tests; verify they fail**

Run: `cargo test -p lopress-editor --test actions_tests split_with_new_block_id`
Expected: FAIL with a compile error — `BlockAction::Split` has no field `new_block_id`.

- [ ] **Step 3: Add `new_block_id: Option<BlockId>` to the `Split` variant**

In `crates/lopress-editor/src/actions.rs`, edit the `Split` arm of the `BlockAction` enum:

```rust
    /// Split the block at `byte_offset` into the block's flat text. The
    /// trailing portion becomes a new block of the same kind directly after
    /// the original. `new_block_id`: `None` mints a fresh id; `Some(id)`
    /// uses the provided id so undo↔redo round-trips are id-stable.
    Split {
        block_id: BlockId,
        byte_offset: usize,
        new_block_id: Option<BlockId>,
    },
```

- [ ] **Step 4: Update every `BlockAction::Split { ... }` construction site to pass `new_block_id: None`**

Run: `cargo build -p lopress-editor 2>&1 | head -40` and let the compiler enumerate the call sites. Add `new_block_id: None,` to each. Expected sites (verify against compile errors):

- `crates/lopress-editor/src/ui/mod.rs` — `pre_focus`/`post_focus` pattern matches; the actions::Split construction (if any local). Pattern matches that destructure `Split { block_id, .. }` continue to work because they use `..`.
- `crates/lopress-editor/src/ui/blocks/inline_editor.rs` — line ~394–402, the Enter handler's `BlockAction::Split { block_id, byte_offset }` construction.
- `crates/lopress-editor/src/undo.rs` — `compute_inverse`'s `BlockAction::MergeWithPrev` → `BlockAction::Split { ... }` translation (line ~228); also the `Split` pattern match for placeholder, which uses `..` and is unaffected.
- `crates/lopress-editor/src/ctrl/mod.rs:75-80` — `CtrlAction::Split { ... } → BlockAction::Split { ... }`.
- `crates/lopress-editor/tests/actions_tests.rs`, `tests/undo_tests.rs`, `tests/list_action_tests.rs` — every existing `BlockAction::Split { ... }`.

Pattern: each construction adds one line. Example:

```rust
BlockAction::Split {
    block_id,
    byte_offset,
    new_block_id: None,
}
```

- [ ] **Step 5: Honor `new_block_id` inside `apply_split`**

In `crates/lopress-editor/src/actions.rs`, edit `apply_split` (line ~152):

```rust
fn apply_split(doc: &mut EditorDoc, id: BlockId, byte_offset: usize, new_block_id: Option<BlockId>) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get(idx) else {
        return;
    };
    let kind = block.kind.clone();
    let body = block.body.clone();

    // Helper: stamp `new_block_id` (if provided) onto the freshly-constructed
    // tail block before inserting it.
    let assign_id = |mut b: EditorBlock| -> EditorBlock {
        if let Some(id) = new_block_id {
            b.id = id;
        }
        b
    };

    match body {
        BlockBody::Code(text) => {
            let mut new_text = text;
            new_text.insert(byte_offset.min(new_text.len()), '\n');
            apply_edit_code(doc, id, new_text);
            // Code blocks split by inserting a '\n' rather than producing a
            // distinct second block; `new_block_id` is unused on this path.
        }
        BlockBody::Inline(runs) => {
            let flat: String = runs.iter().map(|r| r.text.as_str()).collect();
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
            doc.blocks.insert(idx + 1, assign_id(tail_block));
        }
        BlockBody::List(items) => {
            let mut cumulative = 0usize;
            let mut target: Option<(usize, usize)> = None;
            for (i, it) in items.iter().enumerate() {
                let item_len: usize = it.runs.iter().map(|r| r.text.len()).sum();
                if byte_offset <= cumulative + item_len {
                    target = Some((i, byte_offset - cumulative));
                    break;
                }
                cumulative += item_len + 1;
            }
            let (pos, local) = target.unwrap_or((items.len().saturating_sub(1), 0));
            if let Some(b) = doc.blocks.get_mut(idx) {
                if let BlockBody::List(list) = &mut b.body {
                    split_item_at_with_id(list, pos, local, new_block_id);
                }
            }
        }
        BlockBody::Opaque(_) => {}
    }
}
```

Then update the `apply` dispatcher to forward `new_block_id`:

```rust
        BlockAction::Split {
            block_id,
            byte_offset,
            new_block_id,
        } => apply_split(doc, block_id, byte_offset, new_block_id),
```

And add a helper next to `split_item_at` that takes an optional id:

```rust
/// Like `split_item_at`, but uses the provided id for the new item when
/// `new_item_id` is `Some`. `None` mints a fresh id.
fn split_item_at_with_id(
    items: &mut Vec<ListItem>,
    pos: usize,
    byte_offset: usize,
    new_item_id: Option<BlockId>,
) {
    split_item_at(items, pos, byte_offset);
    if let Some(id) = new_item_id {
        if let Some(it) = items.get_mut(pos + 1) {
            it.id = id;
        }
    }
}
```

- [ ] **Step 6: Run the new tests; verify they pass**

Run: `cargo test -p lopress-editor --test actions_tests split_with_new_block_id`
Expected: PASS (both tests).

- [ ] **Step 7: Run the full test suite to confirm no regressions**

Run: `cargo test -p lopress-editor`
Expected: all tests pass. `cargo clippy -p lopress-editor --all-targets -- -D warnings` clean. `cargo fmt --all`.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/src/ui/blocks/inline_editor.rs \
        crates/lopress-editor/src/undo.rs \
        crates/lopress-editor/src/ctrl/mod.rs \
        crates/lopress-editor/tests/actions_tests.rs \
        crates/lopress-editor/tests/undo_tests.rs \
        crates/lopress-editor/tests/list_action_tests.rs
git commit -m "feat(editor): add Split.new_block_id for id-stable undo↔redo"
```

---

## Task 2: Add `new_block_id: Option<BlockId>` to `SplitListItem`

Same shape as Task 1, applied to the list-item variant.

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (enum variant, `apply_split_list_item`)
- Modify: `crates/lopress-editor/src/ui/mod.rs` (every `BlockAction::SplitListItem { ... }` construction)
- Modify: `crates/lopress-editor/src/undo.rs` (the `MergeListItemWithPrev` → `SplitListItem` translation, the placeholder construction in `push_before_apply`)
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs` (the Enter handler's emit site)
- Test: `crates/lopress-editor/tests/list_action_tests.rs`

- [ ] **Step 1: Write a failing test**

Add to `crates/lopress-editor/tests/list_action_tests.rs`:

```rust
#[test]
fn split_list_item_with_new_item_id_uses_provided_id() {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::{BlockBody, BlockId, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta};

    let item_a = BlockId::new();
    let item_b = BlockId::new();
    let block_id = BlockId::new();
    let target_new_id = BlockId::new();

    let mut doc = EditorDoc::default();
    let mut block = EditorBlock {
        id: block_id,
        kind: lopress_editor::model::types::BlockKind::List { ordered: false },
        body: BlockBody::List(vec![
            ListItem { id: item_a, runs: vec![InlineRun::plain("first item")] },
            ListItem { id: item_b, runs: vec![InlineRun::plain("second")] },
        ]),
        plugin: Some(PluginMeta::list(false)),
    };
    doc.blocks.push(block);

    apply(
        &mut doc,
        BlockAction::SplitListItem {
            block_id,
            item_id: item_a,
            byte_offset: 5,
            new_block_id: Some(target_new_id),
        },
    );

    let BlockBody::List(items) = &doc.blocks[0].body else { panic!("expected list body"); };
    assert_eq!(items.len(), 3, "split must produce one extra item");
    assert_eq!(items[1].id, target_new_id, "new item must use the provided id");
}
```

- [ ] **Step 2: Run; verify it fails to compile**

Run: `cargo test -p lopress-editor --test list_action_tests split_list_item_with_new_item_id`
Expected: FAIL — no `new_block_id` field on `SplitListItem`.

- [ ] **Step 3: Add the field to the enum**

In `crates/lopress-editor/src/actions.rs`:

```rust
    /// Split a list item at `byte_offset` into the item's flat text. The
    /// trailing portion becomes a new `ListItem` directly after it.
    /// `new_block_id`: `None` mints a fresh item id; `Some(id)` uses it so
    /// undo↔redo round-trips are id-stable.
    SplitListItem {
        block_id: BlockId,
        item_id: BlockId,
        byte_offset: usize,
        new_block_id: Option<BlockId>,
    },
```

- [ ] **Step 4: Update every `BlockAction::SplitListItem { ... }` construction site to pass `new_block_id: None`**

Run: `cargo build -p lopress-editor 2>&1 | head -40` and add `new_block_id: None,` to each construction. Sites:

- `crates/lopress-editor/src/ui/blocks/list.rs` — line ~227–236, the Enter-key handler.
- `crates/lopress-editor/src/undo.rs` — the `MergeListItemWithPrev` arm of `compute_inverse` (line ~298–302); also the placeholder in `push_before_apply` (line ~52–62) — that one uses `BlockId::new()` for the inner `item_id` placeholder, which is fine to leave as-is for now; just add `new_block_id: None` to the outer struct.
- `crates/lopress-editor/tests/list_action_tests.rs`, `tests/undo_tests.rs` — every existing site.

- [ ] **Step 5: Honor `new_block_id` in `apply_split_list_item`**

In `crates/lopress-editor/src/actions.rs`:

```rust
fn apply_split_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    byte_offset: usize,
    new_item_id: Option<BlockId>,
) {
    let Some(idx) = find_idx(doc, block_id) else {
        return;
    };
    let Some(block) = doc.blocks.get_mut(idx) else {
        return;
    };
    if let BlockBody::List(items) = &mut block.body {
        if let Some(pos) = items.iter().position(|it| it.id == item_id) {
            split_item_at_with_id(items, pos, byte_offset, new_item_id);
        }
    }
}
```

And update the dispatcher arm in `apply`:

```rust
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset,
            new_block_id,
        } => apply_split_list_item(doc, block_id, item_id, byte_offset, new_block_id),
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p lopress-editor --test list_action_tests split_list_item_with_new_item_id`
Expected: PASS.

- [ ] **Step 7: Full suite**

Run: `cargo test -p lopress-editor && cargo clippy -p lopress-editor --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/src/ui/blocks/list.rs \
        crates/lopress-editor/src/undo.rs \
        crates/lopress-editor/tests/list_action_tests.rs \
        crates/lopress-editor/tests/undo_tests.rs
git commit -m "feat(editor): add SplitListItem.new_block_id for id-stable undo↔redo"
```

---

## Task 3: Make `apply` return `Option<(BlockAction, BlockAction)>` and move every inverse into the apply arm

This task is the heart of the stage. Each arm of `apply` is rewritten to (a) capture pre-state before mutating, (b) mutate, (c) build the (canonical action, inverse action) pair, and (d) return it. The dispatcher returns whatever the arm returned, or `None` for `OpenSlashMenu` (unrecorded).

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` — all of `apply` and its helpers.
- Tests: `crates/lopress-editor/tests/actions_tests.rs` and `crates/lopress-editor/tests/list_action_tests.rs` — add inverse-symmetry tests.

The other callers of `apply` (`ui/mod.rs`, `undo.rs`) are not touched in this task — they still call `apply` and ignore its return value. Task 4 wires the return value through.

- [ ] **Step 1: Write failing tests for inverse symmetry**

Add to `crates/lopress-editor/tests/actions_tests.rs`:

```rust
/// For every recordable action, `apply` must return the inverse action that
/// would restore the doc. Applying that inverse to the post-state must
/// reproduce the original pre-state.
mod inverse_symmetry {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::*;

    fn paragraph_doc() -> (EditorDoc, BlockId) {
        let mut doc = EditorDoc::default();
        let id = BlockId::new();
        let mut b = EditorBlock::paragraph(vec![InlineRun::plain("hello world")]);
        b.id = id;
        doc.blocks.push(b);
        (doc, id)
    }

    #[test]
    fn edit_inline_round_trip() {
        let (mut doc, id) = paragraph_doc();
        let before = doc.clone();
        let action = BlockAction::EditInline {
            block_id: id,
            new_runs: vec![InlineRun::plain("changed")],
        };
        let (_canonical, inverse) =
            apply(&mut doc, action).expect("EditInline must record an inverse");
        // Sanity: doc actually changed.
        assert_ne!(doc.blocks[0].body, before.blocks[0].body);
        // Apply the inverse; the doc must match `before` byte-for-byte.
        let (_canon2, _inv2) = apply(&mut doc, inverse).expect("inverse must also record");
        assert_eq!(doc.blocks[0].body, before.blocks[0].body);
    }

    #[test]
    fn split_round_trip_id_stable() {
        let (mut doc, id) = paragraph_doc();
        let before = doc.clone();
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 5,
                new_block_id: None,
            },
        )
        .expect("Split must record an inverse");

        // Canonical must carry the minted id.
        let minted_id = match &canonical {
            BlockAction::Split { new_block_id: Some(nid), .. } => *nid,
            _ => panic!("canonical Split must have a concrete new_block_id"),
        };
        assert_eq!(doc.blocks[1].id, minted_id);

        // Apply the inverse; doc must match `before`.
        let _ = apply(&mut doc, inverse).expect("inverse must record");
        assert_eq!(doc.blocks.len(), before.blocks.len());
        assert_eq!(doc.blocks[0].body, before.blocks[0].body);
    }

    #[test]
    fn split_redo_uses_same_id() {
        // Apply Split → undo → re-apply the canonical Split → the new block
        // must have the SAME id as the first time, because canonical carries
        // it. This proves the four fix_* methods are no longer needed.
        let (mut doc, id) = paragraph_doc();
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 5,
                new_block_id: None,
            },
        )
        .expect("Split must record");
        let original_new_id = doc.blocks[1].id;

        // Undo.
        let _ = apply(&mut doc, inverse).expect("inverse must record");
        assert_eq!(doc.blocks.len(), 1);

        // Redo the canonical form.
        let _ = apply(&mut doc, canonical).expect("canonical re-apply must record");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(
            doc.blocks[1].id, original_new_id,
            "redo must reuse the original new_block_id"
        );
    }

    #[test]
    fn open_slash_menu_returns_none() {
        let (mut doc, id) = paragraph_doc();
        let result = apply(&mut doc, BlockAction::OpenSlashMenu { block_id: id });
        assert!(result.is_none(), "OpenSlashMenu is UI-only, unrecorded");
    }
}
```

And in `crates/lopress-editor/tests/list_action_tests.rs`, add a list-symmetry test:

```rust
#[test]
fn split_list_item_round_trip_id_stable() {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::*;

    let item_a = BlockId::new();
    let item_b = BlockId::new();
    let block_id = BlockId::new();

    let mut doc = EditorDoc::default();
    let block = EditorBlock {
        id: block_id,
        kind: BlockKind::List { ordered: false },
        body: BlockBody::List(vec![
            ListItem { id: item_a, runs: vec![InlineRun::plain("alpha")] },
            ListItem { id: item_b, runs: vec![InlineRun::plain("beta")] },
        ]),
        plugin: Some(PluginMeta::list(false)),
    };
    doc.blocks.push(block);
    let before = doc.clone();

    let (canonical, inverse) = apply(
        &mut doc,
        BlockAction::SplitListItem {
            block_id,
            item_id: item_a,
            byte_offset: 3,
            new_block_id: None,
        },
    )
    .expect("SplitListItem must record");

    let minted_item_id = match &canonical {
        BlockAction::SplitListItem { new_block_id: Some(nid), .. } => *nid,
        _ => panic!("canonical must carry concrete new_block_id"),
    };
    let BlockBody::List(items) = &doc.blocks[0].body else { panic!() };
    assert_eq!(items[1].id, minted_item_id);

    // Undo.
    let _ = apply(&mut doc, inverse).expect("inverse must record");
    assert_eq!(doc.blocks, before.blocks);
}
```

- [ ] **Step 2: Run; verify the new tests fail to compile**

Run: `cargo test -p lopress-editor --test actions_tests inverse_symmetry`
Expected: FAIL — `apply` returns `()`, not a tuple; `.expect` not callable.

- [ ] **Step 3: Change `apply`'s signature and dispatcher**

In `crates/lopress-editor/src/actions.rs`, rewrite `apply`:

```rust
/// Apply one `BlockAction` to the document.
///
/// Returns `Some((canonical_action, inverse_action))` for any recordable
/// action — the action that, when applied to the post-state, restores the
/// pre-state. `canonical_action` differs from the input only for variants
/// that mint ids (`Split`, `SplitListItem`): the returned form has
/// `new_block_id: Some(...)` filled in, so a future redo reuses the same
/// id and undo↔redo stays id-stable without post-apply patching.
///
/// Returns `None` for UI-only actions (`OpenSlashMenu`) and for actions
/// whose target doesn't exist (no-op — nothing to undo).
pub fn apply(
    doc: &mut EditorDoc,
    action: BlockAction,
) -> Option<(BlockAction, BlockAction)> {
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
            new_block_id,
        } => apply_split_list_item(doc, block_id, item_id, byte_offset, new_block_id),
        BlockAction::MergeListItemWithPrev { block_id, item_id } => {
            apply_merge_list_item(doc, block_id, item_id)
        }
        BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditAttrs {
            block_id,
            new_attrs,
        } => apply_edit_attrs(doc, block_id, new_attrs),
    }
}
```

Each helper now returns `Option<(BlockAction, BlockAction)>`.

- [ ] **Step 4: Rewrite each helper to return its inverse**

Replace each helper in `crates/lopress-editor/src/actions.rs`. The pattern is consistent: bail with `None` if the target is missing; otherwise observe pre-state, mutate, return `Some((canonical, inverse))`.

```rust
fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let meta = block.plugin.as_mut()?;
    let old_attrs = std::mem::replace(&mut meta.attrs, new_attrs.clone());
    Some((
        BlockAction::EditAttrs { block_id: id, new_attrs },
        BlockAction::EditAttrs { block_id: id, new_attrs: old_attrs },
    ))
}

fn apply_edit_inline(
    doc: &mut EditorDoc,
    id: BlockId,
    new_runs: Vec<InlineRun>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    if !matches!(block.body, BlockBody::Inline(_)) {
        return None;
    }
    let old_body = std::mem::replace(&mut block.body, BlockBody::Inline(new_runs.clone()));
    let BlockBody::Inline(old_runs) = old_body else {
        unreachable!("checked above")
    };
    Some((
        BlockAction::EditInline { block_id: id, new_runs },
        BlockAction::EditInline { block_id: id, new_runs: old_runs },
    ))
}

fn apply_edit_code(
    doc: &mut EditorDoc,
    id: BlockId,
    new_text: String,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let BlockBody::Code(_) = &block.body else {
        return None;
    };
    let old_text = match std::mem::replace(&mut block.body, BlockBody::Code(new_text.clone())) {
        BlockBody::Code(t) => t,
        _ => unreachable!("checked above"),
    };
    Some((
        BlockAction::EditCode { block_id: id, new_text },
        BlockAction::EditCode { block_id: id, new_text: old_text },
    ))
}

fn apply_edit_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    new_runs: Vec<InlineRun>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, block_id)?;
    let block = doc.blocks.get_mut(idx)?;
    let BlockBody::List(items) = &mut block.body else {
        return None;
    };
    let item = items.iter_mut().find(|it| it.id == item_id)?;
    let old_runs = std::mem::replace(&mut item.runs, new_runs.clone());
    Some((
        BlockAction::EditListItem { block_id, item_id, new_runs },
        BlockAction::EditListItem { block_id, item_id, new_runs: old_runs },
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
    // Replicate the existing change-type body conversion logic exactly.
    match (&new_kind, &block.body) {
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_)) => {
            block.kind = new_kind.clone();
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
            block.plugin = Some(PluginMeta::list(*ordered));
        }
        _ => {
            block.kind = new_kind.clone();
        }
    }
    Some((
        BlockAction::ChangeType { block_id: id, new_kind },
        BlockAction::ChangeType { block_id: id, new_kind: old_kind },
    ))
    // Note: this inverse only fully restores the body if the original
    // ChangeType did NOT convert the body shape. Body conversions (e.g.
    // Inline→Code) are lossy on undo — the old body shape is not snapshot
    // here. This matches the current `compute_inverse` behavior. Improving
    // it is out of scope for this stage; the EditBlockBody collapse in
    // stage 3 will make ChangeType inverses fully reversible by snapshotting
    // body alongside kind.
}

fn apply_insert_after(
    doc: &mut EditorDoc,
    anchor: BlockId,
    new_block: EditorBlock,
) -> Option<(BlockAction, BlockAction)> {
    let pos = find_idx(doc, anchor).map(|i| i + 1).unwrap_or(doc.blocks.len());
    let inserted_id = new_block.id;
    if pos > doc.blocks.len() {
        doc.blocks.push(new_block.clone());
    } else {
        doc.blocks.insert(pos, new_block.clone());
    }
    Some((
        BlockAction::InsertAfter { anchor, new_block },
        BlockAction::Delete { block_id: inserted_id },
    ))
}

fn apply_delete(
    doc: &mut EditorDoc,
    id: BlockId,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    // First-block delete has no predecessor anchor — matches the current
    // `compute_inverse` return-None behavior. Keep the mutation but skip
    // the inverse (caller will treat as unrecordable).
    let anchor_id = idx.checked_sub(1).and_then(|j| doc.blocks.get(j)).map(|b| b.id);
    let removed = doc.blocks.remove(idx);
    if doc.blocks.is_empty() {
        doc.blocks.push(EditorBlock::paragraph(vec![InlineRun::plain("")]));
    }
    let anchor = anchor_id?;
    Some((
        BlockAction::Delete { block_id: id },
        BlockAction::InsertAfter { anchor, new_block: removed },
    ))
}

fn apply_move(
    doc: &mut EditorDoc,
    id: BlockId,
    to_index: usize,
) -> Option<(BlockAction, BlockAction)> {
    let from = find_idx(doc, id)?;
    let target_gap = to_index.min(doc.blocks.len());
    if target_gap == from || target_gap == from + 1 {
        return None; // no-op move; not recorded
    }
    let block = doc.blocks.remove(from);
    let insert_at = if target_gap > from { target_gap - 1 } else { target_gap };
    doc.blocks.insert(insert_at, block);
    // Inverse: same as the current `compute_inverse` logic. `apply_move`
    // reads `to_index` as a pre-removal gap; the inverse gap that returns
    // the block to `from` is `from` for a forward move and `from + 1` for
    // a backward move.
    let inverse_to = if to_index > from { from } else { from + 1 };
    Some((
        BlockAction::Move { block_id: id, to_index },
        BlockAction::Move { block_id: id, to_index: inverse_to },
    ))
}

fn apply_merge(
    doc: &mut EditorDoc,
    id: BlockId,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    if idx == 0 {
        return None;
    }
    // Capture the split offset BEFORE mutation. The split offset is the
    // byte-length of prev's flat text (or, for a list block being merged
    // into an inline-bodied prev, prev's flat text length too — the new
    // content tacks onto the end).
    let prev_id = doc.blocks.get(idx - 1)?.id;
    let split_offset: usize = match &doc.blocks[idx - 1].body {
        BlockBody::Inline(runs) => runs.iter().map(|r| r.text.len()).sum(),
        _ => return None, // unmergeable prev: skip recording inverse
    };

    // Preserve the existing merge logic exactly — list-into-inline merges
    // a single first item.
    if matches!(doc.blocks.get(idx).map(|b| &b.body), Some(BlockBody::List(_))) {
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
        let empty = matches!(
            doc.blocks.get(idx).map(|b| &b.body),
            Some(BlockBody::List(items)) if items.is_empty()
        );
        if empty {
            doc.blocks.remove(idx);
        }
        // The inverse for the list-into-inline merge is the same shape
        // (Split at prev's end). Note: the recreated first item gets a
        // fresh id, since we don't snapshot it; this matches current
        // behavior and will be improved by EditBlockBody in stage 3.
        return Some((
            BlockAction::MergeWithPrev { block_id: id },
            BlockAction::Split {
                block_id: prev_id,
                byte_offset: split_offset,
                new_block_id: None,
            },
        ));
    }

    let cur = doc.blocks.remove(idx);
    let cur_id = cur.id;
    let prev = doc.blocks.get_mut(idx - 1)?;
    if let (BlockBody::Inline(prev_runs), BlockBody::Inline(cur_runs)) =
        (&mut prev.body, cur.body)
    {
        prev_runs.extend(cur_runs);
    }
    // Inverse: Split prev at the offset that produced it. Stamp `cur_id`
    // as the new block id so undo↔redo is id-stable.
    Some((
        BlockAction::MergeWithPrev { block_id: id },
        BlockAction::Split {
            block_id: prev_id,
            byte_offset: split_offset,
            new_block_id: Some(cur_id),
        },
    ))
}

fn apply_merge_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, block_id)?;
    let block = doc.blocks.get_mut(idx)?;
    let BlockBody::List(items) = &mut block.body else {
        return None;
    };
    let pos = items.iter().position(|it| it.id == item_id)?;
    if pos == 0 {
        return None;
    }
    let prev_id = items.get(pos - 1)?.id;
    let split_offset: usize = items[pos - 1].runs.iter().map(|r| r.text.len()).sum();
    let cur_item_id = item_id;

    let cur = items.remove(pos);
    if let Some(prev) = items.get_mut(pos - 1) {
        prev.runs.extend(cur.runs);
    }
    Some((
        BlockAction::MergeListItemWithPrev { block_id, item_id },
        BlockAction::SplitListItem {
            block_id,
            item_id: prev_id,
            byte_offset: split_offset,
            new_block_id: Some(cur_item_id),
        },
    ))
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

    let assign_id = |mut b: EditorBlock| -> EditorBlock {
        if let Some(id) = new_block_id {
            b.id = id;
        }
        b
    };

    match body {
        BlockBody::Code(_) => {
            // Splits on Code insert a newline and produce no second block;
            // the inverse is to remove that newline. Implementing that as a
            // recordable inverse here requires storing the inserted offset.
            // The current `compute_inverse` returned `None` for Split — we
            // preserve that and skip recording here too.
            apply_split_internal_code(doc, id, byte_offset);
            None
        }
        BlockBody::Inline(_) => {
            apply_split_internal_inline(doc, idx, byte_offset, &kind, assign_id)
                .map(|new_id| (
                    BlockAction::Split { block_id: id, byte_offset, new_block_id: Some(new_id) },
                    BlockAction::MergeWithPrev { block_id: new_id },
                ))
        }
        BlockBody::List(_) => {
            // Splitting a list block at a byte offset finds the item
            // containing that offset and splits the item there. The
            // current `compute_inverse` returned None for Split — we
            // preserve that to keep behavior bit-for-bit identical to
            // pre-stage. Stage 3's EditBlockBody collapse makes this path
            // fully reversible.
            let _ = apply_split_internal_list(doc, idx, byte_offset, new_block_id);
            None
        }
        BlockBody::Opaque(_) => None,
    }
}

// Three internal helpers extracted from the previous monolithic apply_split
// to keep the outer match arm readable. Bodies copy/paste from the existing
// function — minimal logic change, only the return shape.
fn apply_split_internal_code(doc: &mut EditorDoc, id: BlockId, byte_offset: usize) {
    let Some(idx) = find_idx(doc, id) else { return };
    let Some(block) = doc.blocks.get(idx) else { return };
    let BlockBody::Code(text) = block.body.clone() else { return };
    let mut new_text = text;
    new_text.insert(byte_offset.min(new_text.len()), '\n');
    let _ = apply_edit_code(doc, id, new_text);
}

fn apply_split_internal_inline(
    doc: &mut EditorDoc,
    idx: usize,
    byte_offset: usize,
    kind: &BlockKind,
    assign_id: impl FnOnce(EditorBlock) -> EditorBlock,
) -> Option<BlockId> {
    let BlockBody::Inline(runs) = doc.blocks.get(idx)?.body.clone() else { return None };
    let flat: String = runs.iter().map(|r| r.text.as_str()).collect();
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
        BlockKind::Heading(level) => EditorBlock::heading(*level, vec![InlineRun::plain(tail)]),
        _ => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
    };
    let stamped = assign_id(tail_block);
    let new_id = stamped.id;
    doc.blocks.insert(idx + 1, stamped);
    Some(new_id)
}

fn apply_split_internal_list(
    doc: &mut EditorDoc,
    idx: usize,
    byte_offset: usize,
    new_block_id: Option<BlockId>,
) -> Option<BlockId> {
    let BlockBody::List(items) = doc.blocks.get(idx)?.body.clone() else { return None };
    let mut cumulative = 0usize;
    let mut target: Option<(usize, usize)> = None;
    for (i, it) in items.iter().enumerate() {
        let item_len: usize = it.runs.iter().map(|r| r.text.len()).sum();
        if byte_offset <= cumulative + item_len {
            target = Some((i, byte_offset - cumulative));
            break;
        }
        cumulative += item_len + 1;
    }
    let (pos, local) = target.unwrap_or((items.len().saturating_sub(1), 0));
    let block = doc.blocks.get_mut(idx)?;
    let BlockBody::List(list) = &mut block.body else { return None };
    split_item_at_with_id(list, pos, local, new_block_id);
    list.get(pos + 1).map(|it| it.id)
}

fn apply_split_list_item(
    doc: &mut EditorDoc,
    block_id: BlockId,
    item_id: BlockId,
    byte_offset: usize,
    new_item_id: Option<BlockId>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, block_id)?;
    let block = doc.blocks.get_mut(idx)?;
    let BlockBody::List(items) = &mut block.body else {
        return None;
    };
    let pos = items.iter().position(|it| it.id == item_id)?;
    split_item_at_with_id(items, pos, byte_offset, new_item_id);
    let minted_id = items.get(pos + 1)?.id;
    Some((
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset,
            new_block_id: Some(minted_id),
        },
        BlockAction::MergeListItemWithPrev {
            block_id,
            item_id: minted_id,
        },
    ))
}
```

(Note on `apply_split` for `BlockBody::List`: the current `compute_inverse` returns `None` for `Split`; the new code also returns `None` for that path to preserve behavior bit-for-bit. The `EditBlockBody` collapse in stage 3 makes this inverse fully reversible. Same note applies to `Code` splits.)

- [ ] **Step 5: Update existing callers of `apply` that ignore its return value**

`apply` is called from `crates/lopress-editor/src/ui/mod.rs` (lines 277-281, 354-358, 405-409). Each call currently looks like:

```rust
current_doc.update(|maybe| {
    if let Some(d) = maybe {
        apply(d, action_for_apply);
    }
});
```

Change each to capture the return value but ignore it for now (Task 4 wires it in):

```rust
current_doc.update(|maybe| {
    if let Some(d) = maybe {
        let _ = apply(d, action_for_apply);
    }
});
```

The internal call `apply_edit_code(doc, id, new_text)` inside `apply_split` for `BlockBody::Code` already ignores the return via `let _ = ...` in the helper.

- [ ] **Step 6: Run the inverse-symmetry tests**

Run: `cargo test -p lopress-editor --test actions_tests inverse_symmetry`
Expected: PASS (four tests).

Run: `cargo test -p lopress-editor --test list_action_tests split_list_item_round_trip`
Expected: PASS.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test -p lopress-editor && cargo clippy -p lopress-editor --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all green. If any existing test breaks (e.g. assertion on `apply`'s unit return), update it to ignore the new tuple.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/tests/actions_tests.rs \
        crates/lopress-editor/tests/list_action_tests.rs
git commit -m "refactor(editor): apply returns (canonical, inverse); inverse logic moves into apply arms"
```

---

## Task 4: Wire the new `apply` return into the undo stack; delete `compute_inverse` and the four `fix_*` methods

This task connects `apply`'s now-returned `(canonical, inverse)` pair to the undo stack and deletes all the indirect-inverse-construction machinery in `undo.rs`. After this task, the stage is complete: behavior is preserved, the codebase is smaller, and adding new block types no longer requires touching `undo.rs`.

**Files:**
- Modify: `crates/lopress-editor/src/undo.rs` — rename `push_before_apply` → `push_after_apply`; new signature `(action, inverse)`; delete `compute_inverse`, `fix_split_inverse`, `fix_split_list_item_inverse`, `fix_merge_redo`, `fix_merge_list_item_redo`, and the placeholder-handling branch in the old push.
- Modify: `crates/lopress-editor/src/ui/mod.rs` — chokepoint, `on_undo`, `on_redo` rewrites.
- Modify: `crates/lopress-editor/tests/undo_tests.rs` — update for the new API; add an explicit `Split` undo↔redo round-trip test that confirms ids stay stable across multiple cycles.

- [ ] **Step 1: Write a failing test for id-stable undo↔redo via the UndoStack**

In `crates/lopress-editor/tests/undo_tests.rs`, add:

```rust
#[test]
fn split_undo_redo_round_trip_preserves_block_id() {
    use lopress_editor::actions::{apply, BlockAction};
    use lopress_editor::model::types::{BlockId, EditorBlock, EditorDoc, InlineRun};
    use lopress_editor::undo::UndoStack;

    let mut doc = EditorDoc::default();
    let a_id = BlockId::new();
    let mut b = EditorBlock::paragraph(vec![InlineRun::plain("hello world")]);
    b.id = a_id;
    doc.blocks.push(b);

    let mut undo = UndoStack::new();

    // Apply Split.
    let action = BlockAction::Split {
        block_id: a_id,
        byte_offset: 5,
        new_block_id: None,
    };
    let (canonical, inverse) = apply(&mut doc, action).unwrap();
    undo.push_after_apply(canonical, inverse);
    let original_new_id = doc.blocks[1].id;

    // Undo.
    let undo_action = undo.pop_undo().unwrap();
    let _ = apply(&mut doc, undo_action).unwrap();
    assert_eq!(doc.blocks.len(), 1);

    // Redo.
    let redo_action = undo.pop_redo().unwrap();
    let _ = apply(&mut doc, redo_action).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[1].id, original_new_id, "redo must preserve the id");

    // Undo again.
    let undo_action_2 = undo.pop_undo().unwrap();
    let _ = apply(&mut doc, undo_action_2).unwrap();
    assert_eq!(doc.blocks.len(), 1);

    // Redo again — id still stable.
    let redo_action_2 = undo.pop_redo().unwrap();
    let _ = apply(&mut doc, redo_action_2).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[1].id, original_new_id, "second redo must also preserve the id");
}
```

- [ ] **Step 2: Run; verify it fails**

Run: `cargo test -p lopress-editor --test undo_tests split_undo_redo_round_trip_preserves_block_id`
Expected: FAIL — `push_after_apply` doesn't exist; `push_before_apply` takes `(doc, &action)`.

- [ ] **Step 3: Rewrite `UndoStack` to use `push_after_apply`**

Replace `crates/lopress-editor/src/undo.rs` contents:

```rust
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::actions::BlockAction;
use crate::model::types::BlockId;

const MAX_UNDO: usize = 100;
const COALESCE_WINDOW: Duration = Duration::from_secs(1);

struct UndoEntry {
    /// Canonical action (with any minted ids filled in). Re-apply for redo.
    action: BlockAction,
    /// The action that undoes `action` when applied to the post-state.
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

    /// Record an action that has just been applied, along with its inverse.
    /// The caller obtains both from `actions::apply`'s return value. Clears
    /// the redo stack for non-coalescing actions or when the coalesce window
    /// expires.
    pub fn push_after_apply(&mut self, action: BlockAction, inverse: BlockAction) {
        // Coalesce successive EditInline actions on the same block within
        // the time window. The stored inverse keeps the OLDEST old_runs
        // (from the existing entry), only the action is bumped forward.
        if let BlockAction::EditInline { block_id, .. } = &action {
            let now = Instant::now();
            if let Some((last_id, last_t)) = self.last_inline_edit {
                if last_id == *block_id
                    && now.duration_since(last_t) < COALESCE_WINDOW
                    && self.redo.is_empty()
                {
                    if let Some(entry) = self.undo.back_mut() {
                        entry.action = action;
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
        self.push_entry(UndoEntry { action, inverse });
    }

    /// Pop the top undo entry's inverse action (to apply as undo).
    /// Pushes the entry onto the redo stack so a subsequent redo
    /// re-applies its canonical action.
    pub fn pop_undo(&mut self) -> Option<BlockAction> {
        let entry = self.undo.pop_back()?;
        let inverse = entry.inverse.clone();
        self.redo.push(entry);
        Some(inverse)
    }

    /// Pop the top redo entry's canonical action (to re-apply as redo).
    /// Pushes the entry back onto the undo stack.
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
```

The whole `compute_inverse` function, the four `fix_*` methods, and the placeholder-inverse branch in the old `push_before_apply` are deleted. The file shrinks significantly.

- [ ] **Step 4: Rewrite the chokepoint in `ui/mod.rs`**

Replace the `on_action` body section that today calls `push_before_apply` + apply + fix-up methods (lines ~256-305) with a simpler flow:

```rust
        // Push to undo stack AFTER apply, using the inverse apply returns.
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
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
```

Wait — `pre_focus` reads pre-apply state to find the previous block before merge. The new flow applies first, then pre_focus would observe post-state. Fix by computing `pre_focus` BEFORE the apply call:

```rust
        // Compute pre-focus (the block that gains focus after MergeWithPrev)
        // BEFORE apply mutates the doc.
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

        // Apply + record.
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
        }

        // Delete the entire Split fix-up block (was lines ~283-293).
        // Delete the entire SplitListItem fix-up block (was lines ~295-305).
        // post_focus below still works because it observes post-state.

        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            (
                BlockAction::SplitListItem {
                    block_id, item_id, ..
                },
                Some(d),
            ) => list_item_after(d, *block_id, *item_id),
            _ => None,
        });
```

In `on_undo` (around lines 344-394), simplify: apply returns the inverse-of-the-inverse (i.e. the action), which the entry has already stored as `action`. The fix-up calls (`fix_merge_redo`, `fix_merge_list_item_redo`) are deleted.

```rust
    let on_undo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_undo();
            });
            if let Some(action) = popped {
                let focus_id = focus_block_for(&action);
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };
```

Same simplification for `on_redo` — the entire fix-up block disappears:

```rust
    let on_redo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_redo();
            });
            if let Some(action) = popped {
                let focus_id = focus_block_for(&action);
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };
```

- [ ] **Step 5: Update test helper imports + sites that used the old API**

Search `crates/lopress-editor/tests/undo_tests.rs` for `push_before_apply` and `compute_inverse` and `fix_split_inverse` and `fix_split_list_item_inverse` and `fix_merge_redo` and `fix_merge_list_item_redo`. For each:

- `push_before_apply(doc, &action)` → first call `let (canonical, inverse) = apply(doc, action.clone()).unwrap();` then `stack.push_after_apply(canonical, inverse);`. Note: the action MUST be applied to the doc the test is asserting against, so this requires re-shaping the test order if it pushed before applying.
- `compute_inverse(doc, &action)` (if used directly) → call `apply(&mut doc.clone(), action.clone())` and take the inverse from the tuple. (Tests that called `compute_inverse` without applying are likely rare; if any exists, restructure to apply on a clone and inspect the inverse.)
- `stack.fix_split_inverse(id)`, `stack.fix_split_list_item_inverse(id)`, `stack.fix_merge_redo(id)`, `stack.fix_merge_list_item_redo(id)` → delete these calls entirely. With the new flow they are unnecessary.

Each test that was structured around the old "push before apply, then fix up after" pattern collapses to "apply, push after apply, done." Tests that exercised the fix-up paths specifically (verifying ids get patched in) can be replaced with the id-stability test added in Step 1.

- [ ] **Step 6: Run the new id-stable round-trip test**

Run: `cargo test -p lopress-editor --test undo_tests split_undo_redo_round_trip_preserves_block_id`
Expected: PASS.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test -p lopress-editor`
Expected: all tests pass.

Run: `cargo clippy -p lopress-editor --all-targets -- -D warnings`
Expected: clean. If unused-imports warnings appear in `ui/mod.rs` (e.g. `list_item_after` not used because the SplitListItem fix-up was deleted), check whether that helper has other callers and remove the import or the helper as appropriate.

Run: `cargo fmt --all -- --check`
Expected: no diff.

- [ ] **Step 8: Run the workspace-level test suite**

Run: `cargo test --workspace`
Expected: all green. This is the gate before commit — stage 1 must not regress any integration test.

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-editor/src/undo.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/tests/undo_tests.rs
git commit -m "refactor(editor): wire apply's returned inverse into UndoStack; delete compute_inverse and fix_* helpers"
```

---

## Stage 1 done-when

After Task 4 commits:

- `crates/lopress-editor/src/actions.rs` exports `pub fn apply(doc, action) -> Option<(BlockAction, BlockAction)>`.
- `crates/lopress-editor/src/undo.rs` no longer contains `compute_inverse`, `fix_split_inverse`, `fix_split_list_item_inverse`, `fix_merge_redo`, or `fix_merge_list_item_redo`. `push_after_apply(action, inverse)` is the only push method.
- `BlockAction::Split` and `BlockAction::SplitListItem` both carry `new_block_id: Option<BlockId>`.
- `cargo test --workspace`, `cargo clippy -p lopress-editor --all-targets -- -D warnings`, `cargo fmt --all -- --check` all clean.
- Undo↔redo round-trips on `Split` and `SplitListItem` are id-stable (covered by the new tests in Tasks 3 and 4).
- No user-facing behavior change: typing, structural edits, undo, redo all feel identical to the pre-stage build.

Stage 2 (introduce `EditBlockBody`, additive) gets its own plan once this lands.

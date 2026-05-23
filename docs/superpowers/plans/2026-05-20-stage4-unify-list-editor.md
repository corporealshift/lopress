# Stage 4: Unify the List Editor onto `mount_block_editor` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate list items onto the same shared editor mount path as paragraphs and headings, simultaneously fixing the **caret-disappears-on-mouse-release** bug, the **uncommitted-list-item-text-loss** data-loss bug, and the **missing Ctrl-shortcuts in list items** gap — all three are downstream of the divergent mount path. List blocks become keyboard-isolated per the spec's section 2 behavior table.

**Architecture:** Extract `mount_block_editor` from `editable_inline` (the paragraph mount). It takes a `structural_key` callback and a `commit` closure. Paragraphs pass `structural_key = |_,_| None` and behave identically to today. List items pass a thin structural-key callback that intercepts Enter / Backspace-at-0 / arrows at vline boundaries and builds a complete new `BlockBody::List` for every list mutation, emitting `EditBlockBody` for the whole list block. After the migration, deletion of `EditListItem`, `SplitListItem`, `MergeListItemWithPrev` cleans up.

**Tech Stack:** Rust, Floem, existing `lopress-editor` crate.

**Spec:** [`docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`](../specs/2026-05-20-list-editor-unification-and-generic-undo-design.md) — Sections 1 (shared mount), 2 (list structural-key table).

**Prior stages:** Stages 1-3 merged. `EditBlockBody` is the generic content-edit action; `EditInline` and `EditCode` are deleted; list-specific variants are the next to go.

**Scope:** This plan covers stage 4 only. After it, stage 5 handles the ctrl HTTP API translation, and stage 6 is cleanup.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | New `mount_block_editor` function with the shared mount logic + key dispatch. `editable_inline` becomes a thin paragraph wrapper. The default-key-handling portion of `handle_key` is folded into `mount_block_editor` (shared by all block types); the Ctrl-shortcut helpers (style toggles, undo/redo, link-URL) stay in this file. |
| `crates/lopress-editor/src/ui/blocks/list.rs` | `list_item_editor` rewritten on top of `mount_block_editor` with a list-specific `structural_key` callback and a batched commit closure that constructs a complete new `BlockBody::List` and emits `EditBlockBody`. Old `handle_list_item_key` and `commit_list_item` are deleted. |
| `crates/lopress-editor/src/ui/blocks/editor_registry.rs` | `editable_list_view` signature gains `on_undo` + `on_redo`; the registry adapter `list_editor_widget` passes them through. |
| `crates/lopress-editor/src/actions.rs` | Deletes `EditListItem`, `SplitListItem`, `MergeListItemWithPrev` variants; their dispatcher arms; `apply_edit_list_item`, `apply_split_list_item`, `apply_merge_list_item`, `split_item_at_with_id` helpers. |
| `crates/lopress-editor/src/ui/mod.rs` | `focus_block_for`'s list-arm gets the deleted variants removed. |
| `crates/lopress-editor/src/ctrl/mod.rs` | Any ctrl translations that produced the deleted variants are updated or removed. |
| `crates/lopress-editor/tests/list_action_tests.rs`, `actions_tests.rs`, `undo_tests.rs` | Tests that constructed or destructured the deleted variants are rewritten or removed. The data-loss regression test and the caret-on-focus test (live verification) are described in the spec's section 4 test strategy — the formal Rust tests focus on the action layer; live verification covers UI feel. |

---

## Task 1: Extract `mount_block_editor` from `editable_inline`

This is a pure refactor — no behavior change. Paragraphs keep behaving identically; tests stay green. The extraction is the architectural cornerstone of the rest of the stage.

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` — add `mount_block_editor` function; rewrite `editable_inline` as a thin paragraph wrapper that calls `mount_block_editor`. The Ctrl-shortcut handling + slash trigger + default Enter/Backspace/arrows/PageUp/PageDown move into the shared mount. The paragraph-specific commit closure (`commit_from_editor`) is passed in.

### Shared mount surface

```rust
type CommitClosure = std::rc::Rc<dyn Fn()>;
type StructuralKey =
    std::rc::Rc<dyn Fn(&KeyPress, floem::keyboard::Modifiers) -> Option<CommandExecuted>>;

#[allow(clippy::too_many_arguments)]
fn mount_block_editor(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: std::rc::Rc<dyn Fn()>,
    on_redo: std::rc::Rc<dyn Fn()>,
    commit: CommitClosure,
    structural_key: StructuralKey,
    slash_eligible: bool,
) -> impl IntoView
```

Internally:

1. Build the `focused: RwSignal<bool>`, the `editor_view`, register the `editor_view_id`.
2. Wire pointer events (down/move/up) and Focus events (gain/lose).
3. `KeyDown` handler order: call `structural_key(kp, ms)` first; if `Some(Yes)`, character insertion is skipped and we stop. Otherwise call the shared key handler that handles:
   - Ctrl/Cmd shortcuts (Z/Y/B/I/E/K, Home/End)
   - Slash trigger when `slash_eligible` and the block is empty
   - Shift+Enter → soft line break
   - Enter → call `commit()` then `Split { block_id, byte_offset, new_block_id: None }`
   - Backspace at offset 0 → call `commit()` then `MergeWithPrev { block_id }`
   - ↑ on first vline → call `commit()` then cross-block nav (focus_target = prev block)
   - ↓ on last vline → call `commit()` then cross-block nav (focus_target = next block)
   - PageUp / PageDown → 10-block jump (calls `commit()` first)
   - Default editor handler (cursor movement within the block) for everything else
4. `focus_pub` publish effect (gates on `ed.active`, same as before — for toolbar focus tracking).
5. `focus_target` programmatic-focus effect.
6. Height-from-visual-lines styling.

### Paragraph wrapper

`editable_inline` becomes:

```rust
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    slash_eligible: bool,
    on_undo: std::rc::Rc<dyn Fn()>,
    on_redo: std::rc::Rc<dyn Fn()>,
) -> impl IntoView {
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let on_action_for_commit = on_action.clone();
    let commit: CommitClosure = std::rc::Rc::new(move || {
        commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
    });
    let structural_key: StructuralKey = std::rc::Rc::new(|_, _| None);
    mount_block_editor(
        state,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
        commit,
        structural_key,
        slash_eligible,
    )
}
```

`commit_from_editor` stays as-is (it already emits `EditBlockBody { new_body: Inline(...) }` from stage 3).

The current monolithic `handle_key` is split: its Ctrl-shortcut portion + default Enter/Backspace/arrows/PageUp/PageDown body moves into `mount_block_editor`'s `KeyDown` handler. `commit_from_editor`, `commit_and_jump_prev`, `commit_and_jump_next`, `apply_style_toggle`, `selection_has_link` stay where they are (they're shared helpers; the mount calls them via the `commit` closure or directly).

### Steps

- [ ] **Step 1: Read the current `editable_inline` and `handle_key` carefully**

Look at `crates/lopress-editor/src/ui/blocks/inline_editor.rs` lines 114-493. Understand which parts are shared logic (Ctrl-shortcuts, character insertion, focus tracking, mount setup) and which are paragraph-specific structural responses (Enter→Split, Backspace→MergeWithPrev, ↑↓→cross-block).

- [ ] **Step 2: Add the `mount_block_editor` function**

Add the new function above `editable_inline`. It contains everything from the current `editable_inline` body plus the body of `handle_key`, restructured so the `structural_key` callback is invoked before the shared logic. The paragraph-specific Enter/Backspace/arrow handlers stay — they're the *shared* default behavior in the new model. The thing that's customisable is `structural_key`.

- [ ] **Step 3: Rewrite `editable_inline` as a thin wrapper**

Shape per the snippet above. Construct the paragraph `commit` and a no-op `structural_key`, call `mount_block_editor`.

- [ ] **Step 4: Verify build + tests + clippy + fmt**

```bash
cargo test --workspace 2>&1 | grep "test result: " | awk '{ok+=$4; failed+=$6} END { print "passed:" ok " failed:" failed }'
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: all green. Behavior unchanged because `editable_inline` is still the only caller of `mount_block_editor`, the paragraph's structural_key returns None, and the shared mount runs the same logic.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "refactor(editor): extract mount_block_editor from editable_inline

Pure refactor. mount_block_editor owns the shared editor mount (focused
RwSignal, editor_view, pointer/focus listeners, KeyDown dispatch, height
styling) plus the shared block-level key handling (Ctrl shortcuts, slash
trigger, Enter/Backspace/arrow/PageUp-Down defaults). It takes a
structural_key callback (Some(Yes) = handled, None = fall through to
shared) and a commit closure.

editable_inline becomes a thin paragraph wrapper that passes
structural_key = |_,_| None. Behavior identical to today.

Stage 4 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Thread `on_undo` + `on_redo` through `editable_list_view`

Preparation for Task 3 — list items need access to undo/redo for Ctrl+Z/Y to work, which means `editable_list_view` needs the closures and the editor registry's `list_editor_widget` needs to pass them through.

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs` — `editable_list_view` signature gains `on_undo` + `on_redo`.
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` — `EditorContext` already carries `on_undo` and `on_redo`. The `list_editor_widget` adapter forwards them.

- [ ] **Step 1: Add `on_undo` / `on_redo` parameters to `editable_list_view`**

In `crates/lopress-editor/src/ui/blocks/list.rs`, update `editable_list_view`'s signature:

```rust
pub fn editable_list_view(
    items: &[ListItem],
    block_id: BlockId,
    ordered: bool,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: std::rc::Rc<dyn Fn()>,
    on_redo: std::rc::Rc<dyn Fn()>,
) -> AnyView { ... }
```

For Task 2 the parameters are accepted but unused (the existing `list_item_editor` doesn't take them yet). Mark with `_` prefix to silence the warning if needed: `_on_undo: ..., _on_redo: ...`. They'll be used in Task 3.

- [ ] **Step 2: Pass them through the registry adapter**

In `crates/lopress-editor/src/ui/blocks/editor_registry.rs`, the `list_editor_widget` function (around line 45) builds the args for `editable_list_view`. Add `on_undo` and `on_redo` from `ctx`:

```rust
fn list_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::List(items) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
    let ordered = ctx
        .block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("ordered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    list::editable_list_view(
        items,
        ctx.block.id,
        ordered,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
        std::rc::Rc::clone(&ctx.on_undo),
        std::rc::Rc::clone(&ctx.on_redo),
    )
}
```

- [ ] **Step 3: Verify build / tests / clippy / fmt**

Same commands as Task 1. Behavior unchanged.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/list.rs \
        crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "refactor(editor): thread on_undo and on_redo through editable_list_view

Preparation for migrating list items onto mount_block_editor in the next
commit, which requires undo/redo closures so list items get Ctrl+Z/Y for
free.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Migrate `list_item_editor` onto `mount_block_editor`

The core of stage 4. Rewrites `list_item_editor` to use the shared mount and implements the structural-key callback per the spec's section 2. After this commit, the **caret bug, data-loss bug, and missing Ctrl-shortcuts in list items are all fixed**.

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs` — heavy rewrite of `list_item_editor`. Old `handle_list_item_key` and `commit_list_item` deleted. New `list_structural_key` builds the structural callback. A new batched-commit closure builds the full new `BlockBody::List` from every item's live buffer.

### Key design point: collecting per-item editor handles

`editable_list_view` needs to give every item's structural-key callback access to *every other item's* current buffer so the batched commit can flush the whole list. We do this with an `Rc<RefCell<Vec<(BlockId, RwSignal<Editor>, RwSignal<Vec<StyleSpan>>)>>>` created once per call to `editable_list_view` and filled in as each `list_item_editor` runs.

```rust
type ItemHandles = std::rc::Rc<std::cell::RefCell<Vec<(
    BlockId,
    RwSignal<Editor>,
    RwSignal<Vec<crate::model::style_span::StyleSpan>>,
)>>>;
```

Each `list_item_editor(...)` pushes its `(item_id, editor_sig, spans_sig)` into the shared `ItemHandles` at construction. The batched commit closure walks the vec, calls `rope_and_spans_to_runs` for each item, builds a fresh `Vec<ListItem>` preserving original ids, and the structural callback packages that vec into `BlockBody::List` for the `EditBlockBody` emit.

### Structural-key behavior table (from spec section 2)

| Key (no Ctrl/Cmd) | Condition | Action |
|---|---|---|
| `Shift+Enter` | always | fall through (`None`) |
| `Enter` | always | build new list body with item split → emit `EditBlockBody`; focus_target = new item id |
| `Backspace` at offset > 0 | any | fall through |
| `Backspace` at offset 0 | `item_index > 0` | build new list body with this item merged into prev → emit `EditBlockBody`; focus_target = prev item id |
| `Backspace` at offset 0 | `item_index == 0`, item empty, list has ≥ 2 items | new list body with this item removed → emit `EditBlockBody`; focus_target = new first item id |
| `Backspace` at offset 0 | `item_index == 0`, item empty, list has 1 item | emit `Delete { block_id }` |
| `Backspace` at offset 0 | `item_index == 0`, item non-empty | consume, no-op |
| `↑` not on first vline | any | fall through |
| `↑` on first vline | `item_index > 0` | flush list + focus prev item |
| `↑` on first vline | `item_index == 0` | consume, no-op (keyboard-isolated) |
| `↓` not on last vline | any | fall through |
| `↓` on last vline | `item_index + 1 < count` | flush list + focus next item |
| `↓` on last vline | last item | consume, no-op (keyboard-isolated) |

All "fall through" cases return `None` from `structural_key`. All handled cases that need to flush the list call the batched commit first (which emits `EditBlockBody` with the full live state), then the structural action (which is also an `EditBlockBody` carrying the post-mutation list body or a `Delete`).

Wait — that would mean two `EditBlockBody` actions per Enter. We can combine: the structural mutation already needs the live buffers from every item to construct the new body, so the commit happens *as part of* building the new body. Concretely:

```rust
// Pseudocode for "Enter" handler inside structural_key:
let new_items = collect_items_from_buffers(item_handles); // reads every item's live buffer
let split_pos = item_index;
let local_offset = byte_offset_in_focused_item;
let (head, tail) = split_runs_at(&new_items[split_pos].runs, local_offset);
let new_item_id = BlockId::new();
let new_list = build_list_with_split(new_items, split_pos, head, tail, new_item_id);
on_action(BlockAction::EditBlockBody {
    block_id: list_block_id,
    new_body: BlockBody::List(new_list),
});
focus_target.set(Some(new_item_id));
```

One emission, full state, atomically. No data loss possible.

### Steps

- [ ] **Step 1: Write a regression test for the data-loss bug**

Append to `crates/lopress-editor/tests/list_action_tests.rs`:

```rust
#[test]
fn editing_multiple_items_then_splitting_one_preserves_all_edits() {
    // Regression for the uncommitted-list-item-edit-loss bug (idea doc
    // 2026-05-18-list-item-uncommitted-edit-loss.md).
    //
    // Simulate the situation by hand: build the post-edit, pre-split list
    // body and emit a single EditBlockBody that contains everyone's edits
    // *plus* the structural split. The action-layer fix relies on the UI
    // building the body this way; this test verifies the action handles
    // it correctly.
    use lopress_editor::model::types::ListItem;
    let it0 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("item zero original")],
    };
    let it1 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("item one original")],
    };
    let it2 = ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain("item two original")],
    };
    let ids = vec![it0.id, it1.id, it2.id];
    let list = EditorBlock::list(false, vec![it0, it1, it2]);
    let block_id = list.id;
    let mut doc = list_doc(vec![]);
    doc.blocks[0] = list;

    // The user typed into items 0 and 2 but didn't commit. Now they press
    // Enter in item 1 to split it. The UI builds a new body that captures
    // everyone's live buffer state + the split.
    let new_item_after_split = BlockId::new();
    let new_body = BlockBody::List(vec![
        ListItem {
            id: ids[0],
            runs: vec![InlineRun::plain("item zero edited")],
        },
        ListItem {
            id: ids[1],
            runs: vec![InlineRun::plain("item one ed")],
        },
        ListItem {
            id: new_item_after_split,
            runs: vec![InlineRun::plain("ited")],
        },
        ListItem {
            id: ids[2],
            runs: vec![InlineRun::plain("item two edited")],
        },
    ]);
    let (_canonical, inverse) = apply(
        &mut doc,
        BlockAction::EditBlockBody { block_id, new_body },
    )
    .unwrap();

    // All four items present with the right text — no edit was lost.
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!("expected list body");
    };
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].runs[0].text, "item zero edited");
    assert_eq!(items[1].runs[0].text, "item one ed");
    assert_eq!(items[2].runs[0].text, "ited");
    assert_eq!(items[2].id, new_item_after_split);
    assert_eq!(items[3].runs[0].text, "item two edited");

    // Inverse round-trip restores the original three items with original ids.
    let _ = apply(&mut doc, inverse).unwrap();
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!();
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items.iter().map(|it| it.id).collect::<Vec<_>>(), ids);
}
```

- [ ] **Step 2: Verify the test passes (it's an action-layer test, action already supports this)**

Run: `cargo test -p lopress-editor --test list_action_tests editing_multiple_items 2>&1 | tail -5`
Expected: PASS. The test verifies the action layer is correct; the UI plumbing in steps 3-N makes the editor build this body shape on every list mutation.

- [ ] **Step 3: Refactor `editable_list_view` to collect item handles**

In `crates/lopress-editor/src/ui/blocks/list.rs`, at the top of `editable_list_view`, create an `ItemHandles` shared structure:

```rust
let item_handles: ItemHandles = std::rc::Rc::new(std::cell::RefCell::new(Vec::with_capacity(items.len())));
```

Pass `item_handles.clone()` into each `list_item_editor` call so each item registers itself there at construction.

- [ ] **Step 4: Rewrite `list_item_editor` to call `mount_block_editor`**

Delete the old body of `list_item_editor` (the `editor_container_view` mount + `handle_list_item_key` + `commit_list_item` + everything else). Replace with:

1. `build_block_editor` for this item's runs (unchanged).
2. Register `(item_id, editor_sig, spans_sig)` into `item_handles`.
3. Build the `commit` closure: walks `item_handles`, builds new `Vec<ListItem>` with each item's current buffer-derived runs (using each item's `editor_sig.with_untracked(|ed| ed.doc().text())` and that item's `spans_sig.get_untracked()`), wraps as `BlockBody::List`, emits `EditBlockBody { block_id: list_block_id, new_body }`.
4. Build the `structural_key` closure per the section 2 table:
   - For Enter / Backspace / arrows, the closure builds a new `Vec<ListItem>` via the same walk as `commit`, then mutates the cloned vec (insert split item, remove item, merge into prev) before emitting `EditBlockBody`.
   - Each handled branch returns `Some(CommandExecuted::Yes)`; unhandled branches return `None`.
5. Call `mount_block_editor(state, list_block_id, on_action, focus_target, focus_pub, current_doc, on_undo, on_redo, commit, structural_key, /* slash_eligible */ false)`. Use `list_block_id` (NOT `item_id`) for the structural callback's emit target — the action is on the whole list block, not the item.

Subtlety: `mount_block_editor` uses `block_id` to set up the `focus_target` programmatic-focus effect. For list items, focus_target may be set to the item's id (during cross-item navigation) or the list block's id (when navigation lands on the list as a whole). The current code handles both cases. Preserve that — easiest is to pass `item_id` for the focus-target plumbing and `list_block_id` for the on_action emits. This may require two `block_id` arguments OR a separate `focus_id` arg to `mount_block_editor`.

After studying it: pass `block_id = item_id` to mount_block_editor (focus / focus_pub use it). The structural_key and commit closures capture `list_block_id` separately and use it for the on_action calls. Adjust the focus_pub publish to set the *list* block id (since the toolbar slot is owned by the list block, not the item) — preserve the existing behavior with a small tweak.

- [ ] **Step 5: Verify build + tests + clippy + fmt**

Likely some errors and warnings to chase down. Expected: all green at the end. The `EditListItem`, `SplitListItem`, `MergeListItemWithPrev` action variants still exist; nothing emits them anymore, but their tests still pass.

- [ ] **Step 6: Live verification of the three bug fixes**

Build a release binary and exercise:
- **Caret-on-focus**: open a list, click into an item. The caret blinks and stays visible after the mouse releases. Press Tab away and back — caret reappears.
- **Data-loss**: open a list with multiple items, type into items 1 and 3 without pressing Enter/arrows, then click into item 2 and press Enter. Items 1 and 3 retain their typed text. (Verify via `127.0.0.1:7878/state` if needed.)
- **Ctrl-shortcuts**: in a list item, Ctrl+B/I/E work; Ctrl+Z/Y undo/redo work; Ctrl+Home/End jump to first/last block.

Document the results inline in the commit message.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/list.rs \
        crates/lopress-editor/tests/list_action_tests.rs
git commit -m "feat(editor): list items use the unified mount_block_editor path

Fixes three bugs by unification:
1. Caret stays visible when the mouse releases (was: vanished — list items
   used editor_container_view with is_active gated on ed.active, the Floem
   pointer-down/up flag rather than focus).
2. Typing into multiple list items without committing then pressing Enter
   no longer loses uncommitted text in non-focused items — every list
   mutation now builds a full BlockBody::List from each item's live buffer
   and emits a single EditBlockBody.
3. Ctrl+B/I/E/K/Z/Y all work inside list items (was: handle_list_item_key
   short-circuited on Ctrl and the default handler did not know them).

Keyboard isolation per spec section 2: arrows at list boundaries do
nothing; Enter never closes the list (always inserts a new item); empty-
first-item Backspace removes the item or, if it's the only item, deletes
the list block.

Stage 4 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Delete the list-specific action variants and helpers

After Task 3 nothing emits `EditListItem`, `SplitListItem`, or `MergeListItemWithPrev`. Delete them.

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` — delete the three variants, their dispatcher arms, and the `apply_edit_list_item`, `apply_split_list_item`, `apply_merge_list_item`, `split_item_at_with_id` helpers.
- Modify: `crates/lopress-editor/src/ui/mod.rs` — `focus_block_for`'s list arm trimmed.
- Modify: `crates/lopress-editor/src/ctrl/mod.rs` — `CtrlAction` translations for the deleted variants (if any) get removed or rerouted. (Likely only `CtrlAction::EditListItem` exists; it can be removed from the ctrl API alongside.)
- Modify: tests in `list_action_tests.rs`, `actions_tests.rs`, `undo_tests.rs` — drop tests that constructed/destructured the deleted variants. The `EditBlockBody` round-trip tests + the data-loss regression test from Task 3 are the new coverage.

### Steps

- [ ] **Step 1: Delete the variants from `BlockAction`**

In `crates/lopress-editor/src/actions.rs`, remove `EditListItem`, `SplitListItem`, `MergeListItemWithPrev` enum variants.

- [ ] **Step 2: Delete the dispatcher arms**

Remove the matching arms from `apply`'s match. Note `SplitListItem` is also used internally by other helpers — that's fine, those helpers go away in Step 3.

- [ ] **Step 3: Delete the helper functions**

Remove `apply_edit_list_item`, `apply_split_list_item`, `apply_merge_list_item`, and `split_item_at_with_id`.

- [ ] **Step 4: Trim `focus_block_for`**

In `crates/lopress-editor/src/ui/mod.rs`, remove the deleted variants from the list-arm of `focus_block_for`. The arm becomes:

```rust
        BlockAction::EditBlockBody { block_id, .. } => Some(*block_id),
```

- [ ] **Step 5: Update `ctrl/mod.rs`**

If `CtrlAction` has variants that produced any of the deleted `BlockAction`s, drop those translations. (Most likely only `CtrlAction::EditListItem` was present; we drop it.) Note any breaking ctrl-API change in the commit message.

- [ ] **Step 6: Drop dead tests**

Run `grep -rn "EditListItem\|SplitListItem\|MergeListItemWithPrev" crates/lopress-editor/` and remove every remaining reference. Where a test asserted the inverse-shape of e.g. `MergeListItemWithPrev`, the equivalent property is covered by the round-trip tests in `mod inverse_symmetry` and the data-loss regression test from Task 3.

- [ ] **Step 7: Verify build / tests / clippy / fmt**

All green.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(editor): delete list-specific action variants

EditListItem, SplitListItem, MergeListItemWithPrev are no longer emitted —
list items go through EditBlockBody for every mutation. Drop the variants,
their dispatcher arms, the apply_edit_list_item / apply_split_list_item /
apply_merge_list_item / split_item_at_with_id helpers, and the
focus_block_for arms. Drop ctrl-API translations for the same.

Tests that asserted shapes of the deleted inverses are removed — coverage
moves to the EditBlockBody round-trip tests and the new data-loss
regression test.

Closes stage 4 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Stage 4 done-when

- `mount_block_editor` exists; `editable_inline` is a thin wrapper that passes `structural_key = |_,_| None`.
- `list_item_editor` is mounted on `mount_block_editor` with the list structural-key callback.
- Every list mutation (typing, Enter-to-split, Backspace-to-merge, item-removal, ↑/↓ at boundaries) emits a single `EditBlockBody` carrying the full new list body — uncommitted text in other items is folded in.
- Caret stays visible in list items when the mouse releases.
- Ctrl+B/I/E/K/Z/Y all work in list items.
- Arrows at list boundaries are no-ops (keyboard-isolated); Enter never closes the list.
- `BlockAction` no longer has `EditListItem`, `SplitListItem`, `MergeListItemWithPrev`.
- `cargo test --workspace`, `cargo clippy -p lopress-editor --all-targets -- -D warnings`, `cargo fmt --all -- --check` all clean.

Stage 5 (ctrl HTTP API translation cleanup) and stage 6 (cleanup pass) get their own plans next.

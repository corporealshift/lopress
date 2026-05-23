# Stage 3: Migrate Paragraph / Heading / Code to `EditBlockBody` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate every emit site of `BlockAction::EditInline` and `BlockAction::EditCode` to emit `BlockAction::EditBlockBody { new_body: BlockBody::Inline(...) }` / `EditBlockBody { new_body: BlockBody::Code(...) }`. Make `apply_split`'s Code-body and List-body paths recordable (they go through `apply_edit_block_body` now). Then delete the now-orphaned `EditInline` and `EditCode` variants and their helpers. Generalize `undo.rs` coalescing to key on `EditBlockBody`.

**Architecture:** Four small, sequential commits. Each leaves the editor green. After this stage, `BlockAction` no longer carries the per-shape content variants for paragraphs/headings/code — only `EditBlockBody` plus the still-present list-specific variants (`EditListItem` / `SplitListItem` / `MergeListItemWithPrev`, deleted in stage 4).

**Tech Stack:** Rust, existing `lopress-editor` crate.

**Spec:** [`docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`](../specs/2026-05-20-list-editor-unification-and-generic-undo-design.md) — Section 4 stage 3.

**Prior stages:** Stage 1 (apply returns inverse) merged. Stage 2 (`EditBlockBody` added) merged.

**Scope:** This plan covers stage 3 only. List-specific variants stay until stage 4. The list widget (`ui/blocks/list.rs`) is not touched. The ctrl HTTP API's public verb shapes (`CtrlAction::EditInline` / `EditCode`) stay — the internal translation to `BlockAction` changes.

---

## File Structure

| File | Responsibility | Tasks |
|------|----------------|-------|
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | `commit_from_editor` builds an `Inline` body and emits `EditBlockBody`. | Task 1 |
| `crates/lopress-editor/src/ui/toolbar.rs` | Three sites (ChangeType-commit-current, link-URL commit, link-URL remove) build `Inline` bodies and emit `EditBlockBody`. | Task 1 |
| `crates/lopress-editor/src/ctrl/mod.rs` | `CtrlAction::EditInline` and `CtrlAction::EditCode` translate to `BlockAction::EditBlockBody`. | Task 2 |
| `crates/lopress-editor/src/actions.rs` | `apply_split`'s Code and List branches route through `apply_edit_block_body` and become recordable. Then `EditInline` / `EditCode` variants, `apply_edit_inline`, `apply_edit_code` deleted. Dispatcher arms removed. | Tasks 3, 4 |
| `crates/lopress-editor/src/undo.rs` | Coalescing match changes from `EditInline` to `EditBlockBody`. | Task 4 |
| `crates/lopress-editor/src/ui/mod.rs` | `focus_block_for` drops the `EditInline | EditCode` arm. | Task 4 |
| `crates/lopress-editor/tests/actions_tests.rs`, `undo_tests.rs` | All tests that constructed `EditInline` / `EditCode` (for assertions or fixtures) rewritten to use `EditBlockBody`. Inverse-shape tests for those variants are removed (the `EditBlockBody` round-trip tests already cover the semantics). | Task 4 |

---

## Task 1: Migrate UI emit sites for `EditInline` → `EditBlockBody`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs:547-560` (`commit_from_editor`)
- Modify: `crates/lopress-editor/src/ui/toolbar.rs:89, 170, 191` (three emit sites)

- [ ] **Step 1: Update `commit_from_editor` in `inline_editor.rs`**

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, change `commit_from_editor` (around line 547):

```rust
fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: crate::model::types::BlockBody::Inline(new_runs),
    });
}
```

If `BlockBody` is not already imported in this file's `use` block, add `crate::model::types::BlockBody` to the existing imports.

- [ ] **Step 2: Update toolbar.rs ChangeType-commit-current site (line ~89)**

In `crates/lopress-editor/src/ui/toolbar.rs`, find the type-selector button's action closure (around line 80-95). Replace:

```rust
                    on_action_for_btn(BlockAction::EditInline { block_id, new_runs });
```

with:

```rust
                    on_action_for_btn(BlockAction::EditBlockBody {
                        block_id,
                        new_body: crate::model::types::BlockBody::Inline(new_runs),
                    });
```

- [ ] **Step 3: Update toolbar.rs link-URL commit site (line ~170)**

In the same file, find the `commit` closure in the link-URL row (around line 160-173). Replace:

```rust
                        on_action_commit(BlockAction::EditInline { block_id, new_runs });
```

with:

```rust
                        on_action_commit(BlockAction::EditBlockBody {
                            block_id,
                            new_body: crate::model::types::BlockBody::Inline(new_runs),
                        });
```

- [ ] **Step 4: Update toolbar.rs link-URL remove site (line ~191)**

Same file, the `remove` closure (around line 176-193). Replace:

```rust
                        on_action_remove(BlockAction::EditInline { block_id, new_runs });
```

with:

```rust
                        on_action_remove(BlockAction::EditBlockBody {
                            block_id,
                            new_body: crate::model::types::BlockBody::Inline(new_runs),
                        });
```

- [ ] **Step 5: Verify build, tests, clippy, fmt**

Run:

```bash
cargo build -p lopress-editor 2>&1 | tail -5
cargo test -p lopress-editor 2>&1 | grep "test result: "
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: build clean, all tests pass, clippy clean, fmt clean. Behavior unchanged — `EditBlockBody` with an `Inline` body produces the same end state as `EditInline` did.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs \
        crates/lopress-editor/src/ui/toolbar.rs
git commit -m "refactor(editor): UI emit sites use EditBlockBody for inline content

commit_from_editor and the three toolbar emit sites (ChangeType commit,
link-URL commit, link-URL remove) now construct BlockBody::Inline locally
and emit EditBlockBody { new_body } instead of EditInline { new_runs }.
Behavior unchanged.

Stage 3 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Migrate ctrl translation for `EditInline` + `EditCode` → `EditBlockBody`

**Files:**
- Modify: `crates/lopress-editor/src/ctrl/mod.rs:100-110` (translation of two ctrl actions)

- [ ] **Step 1: Update `CtrlAction::EditInline` translation**

In `crates/lopress-editor/src/ctrl/mod.rs`, find the `into_block_action` method's match arm for `CtrlAction::EditInline` (around line 102). Replace:

```rust
            CtrlAction::EditInline { block_id, new_runs } => BlockAction::EditInline {
                block_id: find(doc, block_id)?,
                new_runs,
            },
```

with:

```rust
            CtrlAction::EditInline { block_id, new_runs } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: crate::model::types::BlockBody::Inline(new_runs),
            },
```

- [ ] **Step 2: Update `CtrlAction::EditCode` translation (line ~106)**

In the same file, replace:

```rust
            CtrlAction::EditCode { block_id, new_text } => BlockAction::EditCode {
                block_id: find(doc, block_id)?,
                new_text,
            },
```

with:

```rust
            CtrlAction::EditCode { block_id, new_text } => BlockAction::EditBlockBody {
                block_id: find(doc, block_id)?,
                new_body: crate::model::types::BlockBody::Code(new_text),
            },
```

The public `CtrlAction::EditInline` / `EditCode` verb shapes stay — only the internal translation changes.

- [ ] **Step 3: Verify build, tests, clippy, fmt**

```bash
cargo build -p lopress-editor 2>&1 | tail -5
cargo test --workspace 2>&1 | grep "test result: " | awk '{ok+=$4; failed+=$6} END { print "passed:" ok " failed:" failed }'
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: build clean, all workspace tests pass, clippy clean, fmt clean.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ctrl/mod.rs
git commit -m "refactor(editor): ctrl API translates EditInline/EditCode to EditBlockBody

The public CtrlAction::EditInline and CtrlAction::EditCode verb shapes are
unchanged. Their internal translation to BlockAction now constructs an
EditBlockBody with the appropriate BlockBody shape. Stage 5's ctrl API
work will further translate other verbs into the unified enum.

Stage 3 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Make `apply_split`'s Code and List paths recordable via `EditBlockBody`

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` — `apply_split`'s `BlockBody::Code` and `BlockBody::List` branches (around lines 194-265)
- Modify: `crates/lopress-editor/tests/actions_tests.rs` — strengthen / add round-trip tests for Code-split and List-split now that they are recordable
- Modify: docstring on `apply` (around line 105-127) — remove "Code/List splits" and "first-block Delete" from the stage-1-unrecordable bucket where they no longer apply

- [ ] **Step 1: Write failing tests for recordable Code-split and List-split**

Append to `crates/lopress-editor/tests/actions_tests.rs` inside `mod inverse_symmetry`:

```rust
    #[test]
    fn split_code_block_is_now_recordable() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("foobar".to_string());
        block.kind = BlockKind::Code {
            lang: String::new(),
        };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        assert_round_trip(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 3,
                new_block_id: None,
            },
        );
        // After undo, the Code body should be restored to "foobar".
        match &doc.blocks[0].body {
            BlockBody::Code(text) => assert_eq!(text, "foobar"),
            _ => panic!("expected Code body"),
        }
    }

    #[test]
    fn split_list_block_is_now_recordable() {
        use lopress_editor::model::types::ListItem;
        let it0 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("ab")],
        };
        let it1 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("cd")],
        };
        let original_item_ids = vec![it0.id, it1.id];
        let list = EditorBlock::list(false, vec![it0, it1]);
        let block_id = list.id;
        let mut doc = doc_with(vec![list]);
        // Top-level Split on the list at flat-offset 4: lands inside item 1
        // at local-offset 1 (item 0 has 2 chars + 1 newline = 3, so offset 4
        // is in item 1 at position 1).
        assert_round_trip(
            &mut doc,
            BlockAction::Split {
                block_id,
                byte_offset: 4,
                new_block_id: None,
            },
        );
        // After undo, the list should have its original two items with their
        // original ids.
        match &doc.blocks[0].body {
            BlockBody::List(items) => {
                let ids: Vec<_> = items.iter().map(|it| it.id).collect();
                assert_eq!(ids, original_item_ids, "undo must restore the original item ids");
            }
            _ => panic!("expected List body"),
        }
    }
```

- [ ] **Step 2: Run; verify they fail**

Run: `cargo test -p lopress-editor --test actions_tests inverse_symmetry::split_code_block_is_now_recordable 2>&1 | tail -10`
Expected: FAIL with the panic from `assert_round_trip` — the test reaches `apply(&mut doc, action).expect("action must record")` and that returns `None` for Code/List splits today, so `.expect` panics.

- [ ] **Step 3: Refactor `apply_split`'s Code branch to use `apply_edit_block_body`**

In `crates/lopress-editor/src/actions.rs`, replace the `BlockBody::Code(text) => { … None }` arm of `apply_split` (around lines 194-201) with:

```rust
        BlockBody::Code(text) => {
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
```

The outer canonical action stays as `BlockAction::Split` so a redo replays the same flow; the inverse is the `EditBlockBody` that restores the old Code text.

- [ ] **Step 4: Refactor `apply_split`'s List branch to use `apply_edit_block_body`**

Replace the `BlockBody::List(items) => { … None }` arm of `apply_split` (around lines 242-263) with:

```rust
        BlockBody::List(items) => {
            // The ctrl API's `Split` command treats a list as the flat text of
            // its items joined by '\n'. Walk cumulative byte offsets to find
            // the item containing `byte_offset` and split it there.
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
            // Build the new list body off-doc (we already own a clone of
            // `items` from the outer `body = block.body.clone()` snapshot).
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
```

The canonical action stamps `new_block_id: Some(minted_id)` so redos are id-stable for list splits, matching the inline-body Split path's behavior from stage 1.

- [ ] **Step 5: Update the `apply` docstring to drop the stage-1-unrecordable bucket**

Replace the docstring's category-3 bullet (around lines 119-127) — the `Returns None when ...` paragraph — with:

```rust
/// Returns `None` when the action does not produce a recordable inverse.
/// Two cases:
/// 1. **UI-only actions** (`OpenSlashMenu`) — never touch the model.
/// 2. **No-op actions** — target block id not found, `Move` with a
///    same-position gap, `MergeWithPrev` on the first block, or `Delete`
///    of the first block (no predecessor anchor exists for the
///    `InsertAfter` inverse). The model may be unchanged or, for
///    first-block `Delete`, mutated in a way that cannot be undone via
///    the current action enum. (First-block `Delete` is the lone
///    intentionally-unrecordable mutation remaining.)
```

- [ ] **Step 6: Run the new tests; verify they pass**

Run: `cargo test -p lopress-editor --test actions_tests inverse_symmetry 2>&1 | tail -20`
Expected: all 18 tests pass (was 16 — +2 new).

- [ ] **Step 7: Verify the full suite, clippy, fmt**

```bash
cargo test --workspace 2>&1 | grep "test result: " | awk '{ok+=$4; failed+=$6} END { print "passed:" ok " failed:" failed }'
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/tests/actions_tests.rs
git commit -m "feat(editor): Code and List splits are now recordable via EditBlockBody

apply_split's Code branch and List branch now construct the new body and
route through apply_edit_block_body, returning a recordable (Split, inverse)
pair where the inverse is an EditBlockBody restoring the old body.

For list splits, the canonical Split action stamps new_block_id: Some(...)
with the newly-minted item id, matching the inline-body path's id stability.

The apply docstring drops these two cases from the unrecordable bucket;
only first-block Delete remains intentionally unrecordable.

Stage 3 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Delete `EditInline` / `EditCode` variants and helpers; generalize undo coalescing

This is the cleanup task. After Tasks 1-3, nothing emits `EditInline` or `EditCode` (UI uses `EditBlockBody`, ctrl translates to `EditBlockBody`, `apply_split` routes through `apply_edit_block_body`). The variants are dead. Delete them, delete the helpers, update the few remaining references in tests and `focus_block_for`, and widen `undo.rs` coalescing to key on `EditBlockBody`.

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` — delete `EditInline` and `EditCode` enum variants, their dispatcher arms, and the `apply_edit_inline` / `apply_edit_code` helpers.
- Modify: `crates/lopress-editor/src/undo.rs` — change the coalescing match from `BlockAction::EditInline` to `BlockAction::EditBlockBody`.
- Modify: `crates/lopress-editor/src/ui/mod.rs` — drop the `EditInline | EditCode` arm of `focus_block_for`.
- Modify: `crates/lopress-editor/tests/actions_tests.rs`, `crates/lopress-editor/tests/undo_tests.rs` — rewrite every test that constructs or destructures `EditInline` / `EditCode` to use `EditBlockBody`. The old inverse-shape tests (`inverse_of_edit_inline_is_old_runs`, the equivalent for code) are dropped — the `EditBlockBody` round-trip tests already cover the semantics.

- [ ] **Step 1: Delete `EditInline` and `EditCode` enum variants in `actions.rs`**

In `crates/lopress-editor/src/actions.rs`, remove these two variants from the `BlockAction` enum (around lines 51-60):

```rust
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
```

- [ ] **Step 2: Delete the dispatcher arms**

In the same file's `apply` function, remove these two arms (around lines 144-147):

```rust
        BlockAction::EditInline { block_id, new_runs } => {
            apply_edit_inline(doc, block_id, new_runs)
        }
        BlockAction::EditCode { block_id, new_text } => apply_edit_code(doc, block_id, new_text),
```

- [ ] **Step 3: Delete the `apply_edit_inline` and `apply_edit_code` helper functions**

Remove the two helper bodies entirely (around lines 488-540 in the current file). They have no remaining callers — `apply_split`'s Code branch went through `apply_edit_block_body` in Task 3, and the dispatcher arms are gone in Step 2.

- [ ] **Step 4: Verify `actions.rs` compiles**

Run: `cargo build -p lopress-editor 2>&1 | head -30`
Expected: compile errors in `undo.rs`, `ui/mod.rs`, and tests that reference the deleted variants — those are fixed in the next steps.

- [ ] **Step 5: Update `undo.rs` coalescing to key on `EditBlockBody`**

In `crates/lopress-editor/src/undo.rs`, find the coalescing block in `push_after_apply` (around line 41). Replace:

```rust
        if let BlockAction::EditInline { block_id, .. } = &action {
```

with:

```rust
        if let BlockAction::EditBlockBody { block_id, .. } = &action {
```

The rest of the coalescing logic is unchanged — successive `EditBlockBody` actions on the same block within the 1-second window collapse into one undo entry, with the oldest inverse preserved and the latest action stored. The renamed local `edit_id` from stage 1 stays.

- [ ] **Step 6: Update `focus_block_for` in `ui/mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, find `focus_block_for` (around line 135). The current first arm is:

```rust
        BlockAction::EditInline { block_id, .. }
        | BlockAction::EditCode { block_id, .. }
        | BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
```

Remove `BlockAction::EditInline { block_id, .. } |` and `BlockAction::EditCode { block_id, .. } |` from the alternatives. Result:

```rust
        BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
```

(The list-specific arm and the `EditBlockBody` arm stay.)

- [ ] **Step 7: Rewrite tests in `actions_tests.rs` that constructed `EditInline` / `EditCode`**

Run `grep -n "EditInline\|EditCode" crates/lopress-editor/tests/actions_tests.rs` to enumerate. For each, rewrite the action to `EditBlockBody { block_id, new_body: BlockBody::Inline(...) }` or `EditBlockBody { block_id, new_body: BlockBody::Code(...) }`. Tests that destructured the variant in match arms drop those branches.

The `edit_inline_round_trip` and `edit_code_round_trip` tests inside `mod inverse_symmetry` are removed — `edit_block_body_inline_round_trip` and `edit_block_body_code_round_trip` already cover the same property.

- [ ] **Step 8: Rewrite tests in `undo_tests.rs` that referenced `EditInline` / `EditCode`**

Run `grep -n "EditInline\|EditCode" crates/lopress-editor/tests/undo_tests.rs`. Update each:

- `inverse_of_edit_inline_is_old_runs` — drop the test (covered by the `EditBlockBody` round-trip tests in `actions_tests.rs`).
- `undo_stack_push_and_pop` — change the action constructor to `EditBlockBody { block_id: id, new_body: BlockBody::Inline(vec![InlineRun::plain("edited")]) }`. The pop_undo match arm becomes `BlockAction::EditBlockBody { new_body, .. }` and asserts on the body shape, e.g.:
  ```rust
  match undo_action {
      BlockAction::EditBlockBody {
          new_body: BlockBody::Inline(runs),
          ..
      } => {
          assert_eq!(runs, vec![InlineRun::plain("text")]);
      }
      _ => panic!("wrong variant"),
  }
  ```
- `undo_stack_redo_available_after_undo` — same shape transform.
- `edit_inline_within_one_second_coalesces` — rename to `edit_block_body_within_one_second_coalesces`. Use `EditBlockBody` actions throughout. Assert that two rapid `EditBlockBody` on the same block produce `undo_depth() == 1`.

The inverse-shape tests for the deleted variants are removed (their semantics are covered by the new round-trip tests added in stage 2).

- [ ] **Step 9: Add `BlockBody` to test-file imports if needed**

If any test file complains about an unresolved `BlockBody` import after the rewrites, add it to the `use lopress_editor::model::types::{...}` line. The existing actions_tests.rs already imports `BlockBody`. undo_tests.rs already imports `BlockBody` near the bottom.

- [ ] **Step 10: Verify build, tests, clippy, fmt**

Run:

```bash
cargo build -p lopress-editor 2>&1 | tail -5
cargo test --workspace 2>&1 | grep "test result: " | awk '{ok+=$4; failed+=$6} END { print "passed:" ok " failed:" failed }'
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: all green. The test count drops a bit because dead-equivalent tests were removed, but the new symmetry tests in stage 2 plus the new recordable-Split tests in Task 3 of this stage carry the coverage.

- [ ] **Step 11: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/src/undo.rs \
        crates/lopress-editor/src/ui/mod.rs \
        crates/lopress-editor/tests/actions_tests.rs \
        crates/lopress-editor/tests/undo_tests.rs
git commit -m "refactor(editor): delete EditInline and EditCode; coalesce on EditBlockBody

Nothing emits EditInline or EditCode anymore — Tasks 1-3 of this stage
migrated every UI emit site, the ctrl translation, and the apply_split
internal path. Delete the variants, their dispatcher arms, the
apply_edit_inline / apply_edit_code helpers, and the focus_block_for
arms.

Generalize undo.rs coalescing from EditInline to EditBlockBody (keyed on
block_id) — typing in a Code block now coalesces the same way paragraph
typing already did, and stage 4's list typing will inherit the same
behavior for free.

Tests that constructed or destructured the deleted variants are rewritten
to use EditBlockBody. Old inverse-shape tests are dropped (covered by the
EditBlockBody round-trip tests added in stage 2).

Closes stage 3 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Stage 3 done-when

After Task 4 commits:

- `BlockAction` enum no longer has `EditInline` or `EditCode` variants.
- `apply_edit_inline` and `apply_edit_code` helpers are gone.
- `undo.rs` coalescing keys on `EditBlockBody`.
- `focus_block_for` matches the trimmed set of variants.
- `cargo test --workspace`, `cargo clippy -p lopress-editor --all-targets -- -D warnings`, `cargo fmt --all -- --check` all clean.
- Code-body and List-body splits are now recordable (their inverse is an `EditBlockBody` restoring the old body).
- List-specific variants (`EditListItem`, `SplitListItem`, `MergeListItemWithPrev`) still exist — stage 4 deletes them.

Stage 4 (extract `mount_block_editor`, migrate list to it, delete the list variants) gets its own plan once this lands.

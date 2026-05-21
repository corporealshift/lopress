# Stage 2: Add `EditBlockBody` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `BlockAction::EditBlockBody { block_id, new_body: BlockBody }` variant. Its `apply` arm replaces the block's body and returns an `EditBlockBody` carrying the old body as the inverse. One arm. One inverse rule. Works for any body shape.

**Architecture:** Purely additive. The new variant rides on top of stage 1's `apply -> Option<(BlockAction, BlockAction)>` shape. All existing per-type content variants (`EditInline`, `EditCode`, `EditListItem`, `SplitListItem`, `MergeListItemWithPrev`) stay untouched — stage 3 migrates widgets to emit `EditBlockBody` and deletes those variants once their emit sites are gone.

**Tech Stack:** Rust, existing `lopress-editor` crate.

**Spec:** [`docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`](../specs/2026-05-20-list-editor-unification-and-generic-undo-design.md) — Section 3, "Shift B".

**Prior stage:** [`docs/superpowers/plans/2026-05-20-stage1-inverse-from-apply.md`](2026-05-20-stage1-inverse-from-apply.md) (merged — gives us the `(canonical, inverse)` return shape this stage builds on).

**Scope:** This plan covers **stage 2 only**. No widget changes (stage 3). No deletion of `EditInline`/`EditCode`/etc. (also stage 3). Behavior is unchanged because nothing emits the new variant yet.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/lopress-editor/src/actions.rs` | New `EditBlockBody { block_id, new_body }` variant on `BlockAction`. New `apply_edit_block_body` helper. New dispatcher arm. |
| `crates/lopress-editor/tests/actions_tests.rs` | Three new tests in `mod inverse_symmetry` covering round-trip on Inline, Code, and List body shapes. |

No new files. No public API outside the existing `actions` module surface.

---

## Task 1: Add `EditBlockBody` variant and apply arm

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` — enum (~line 12), dispatcher (`apply` function, ~line 120), new helper at the bottom of the file.
- Test: `crates/lopress-editor/tests/actions_tests.rs` — append three tests to the existing `mod inverse_symmetry` block.

- [ ] **Step 1: Write the failing test for an Inline body round-trip**

Append to `mod inverse_symmetry` at the bottom of `crates/lopress-editor/tests/actions_tests.rs`:

```rust
    #[test]
    fn edit_block_body_inline_round_trip() {
        let (id, block) = paragraph_with_id("hello world");
        let mut doc = doc_with(vec![block]);
        let new_body =
            BlockBody::Inline(vec![InlineRun::plain("entirely different content")]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
            },
        );
    }
```

(`assert_round_trip` already exists in `mod inverse_symmetry` from stage 1.)

- [ ] **Step 2: Write the failing test for a Code body round-trip**

Append:

```rust
    #[test]
    fn edit_block_body_code_round_trip() {
        let mut block = EditorBlock::paragraph(vec![InlineRun::plain("")]);
        block.body = BlockBody::Code("fn main() {}".to_string());
        block.kind = BlockKind::Code {
            lang: String::new(),
        };
        let id = block.id;
        let mut doc = doc_with(vec![block]);
        let new_body = BlockBody::Code("fn other() { /* … */ }".to_string());
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
            },
        );
    }
```

- [ ] **Step 3: Write the failing test for a List body round-trip**

Append:

```rust
    #[test]
    fn edit_block_body_list_round_trip() {
        use lopress_editor::model::types::ListItem;
        let it0 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("first")],
        };
        let it1 = ListItem {
            id: BlockId::new(),
            runs: vec![InlineRun::plain("second")],
        };
        let list = EditorBlock::list(false, vec![it0, it1]);
        let id = list.id;
        let mut doc = doc_with(vec![list]);
        let new_body = BlockBody::List(vec![
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("entirely")],
            },
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("different")],
            },
            ListItem {
                id: BlockId::new(),
                runs: vec![InlineRun::plain("items")],
            },
        ]);
        assert_round_trip(
            &mut doc,
            BlockAction::EditBlockBody {
                block_id: id,
                new_body,
            },
        );
    }
```

- [ ] **Step 4: Run the new tests; verify they fail to compile**

Run: `cargo build -p lopress-editor --tests 2>&1 | head -20`
Expected: `error[E0599]: no variant or associated item named 'EditBlockBody' found for enum 'BlockAction'` (or similar — the variant doesn't exist yet). This is the correct failure mode.

- [ ] **Step 5: Add the `EditBlockBody` variant to the `BlockAction` enum**

In `crates/lopress-editor/src/actions.rs`, add this variant alongside the existing ones in the `BlockAction` enum (placed after `EditAttrs`, just before the final `}`):

```rust
    /// Replace `block_id`'s entire `body` with `new_body`. Generic content
    /// edit — works for any body shape (Inline, Code, List, Opaque). The
    /// inverse swaps the old body back. Used by widgets that construct the
    /// target body locally rather than declaring a per-shape intent.
    EditBlockBody {
        block_id: BlockId,
        new_body: BlockBody,
    },
```

- [ ] **Step 6: Add the dispatcher arm to `apply`**

In `crates/lopress-editor/src/actions.rs`, add a new arm to the `match action { ... }` in `apply`. Place it after the existing `BlockAction::EditAttrs { ... }` arm:

```rust
        BlockAction::EditBlockBody {
            block_id,
            new_body,
        } => apply_edit_block_body(doc, block_id, new_body),
```

- [ ] **Step 7: Implement `apply_edit_block_body`**

Add this helper at the bottom of `crates/lopress-editor/src/actions.rs` (after `split_item_at_with_id`):

```rust
/// Replace the body of `id` with `new_body`. Returns the (canonical action,
/// inverse action) pair: the inverse is another `EditBlockBody` carrying
/// the old body. Works for any body shape — the helper is shape-agnostic.
fn apply_edit_block_body(
    doc: &mut EditorDoc,
    id: BlockId,
    new_body: BlockBody,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
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
```

- [ ] **Step 8: Run the new tests; verify they pass**

Run: `cargo test -p lopress-editor --test actions_tests inverse_symmetry::edit_block_body 2>&1 | tail -10`
Expected: PASS — all three tests (`edit_block_body_inline_round_trip`, `edit_block_body_code_round_trip`, `edit_block_body_list_round_trip`).

- [ ] **Step 9: Run the full lopress-editor test suite**

Run: `cargo test -p lopress-editor 2>&1 | grep "test result: " | head -20`
Expected: every line shows `ok. N passed; 0 failed`. No regressions in existing tests — the addition is non-breaking.

- [ ] **Step 10: Run workspace tests, clippy, fmt**

Run, all three:

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy -p lopress-editor --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check 2>&1 | tail -3
```

Expected: workspace tests all pass, clippy clean (no warnings), fmt clean (no diff).

- [ ] **Step 11: Commit**

```bash
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/tests/actions_tests.rs
git commit -m "feat(editor): add EditBlockBody action variant for shape-agnostic body swaps

A new BlockAction::EditBlockBody { block_id, new_body: BlockBody } variant.
apply_edit_block_body swaps the body and returns an EditBlockBody carrying
the old body as the inverse — one arm, one inverse rule, works for any
body shape.

Purely additive. Nothing emits this variant yet; stage 3 migrates the
paragraph / heading / code widgets to use it (and then deletes the
per-type EditInline / EditCode arms once their emit sites are gone).

Three new round-trip tests cover Inline, Code, and List body shapes.

Stage 2 of docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Stage 2 done-when

After Task 1 commits:

- `BlockAction::EditBlockBody { block_id, new_body }` exists with a doc comment explaining the shape-agnostic semantics.
- `apply` dispatches it to `apply_edit_block_body`, which returns the symmetric `(canonical, inverse)` pair.
- All existing variants (`EditInline`, `EditCode`, `EditListItem`, `SplitListItem`, `MergeListItemWithPrev`, etc.) still exist and behave identically — nothing emits the new variant yet.
- Three round-trip tests cover Inline / Code / List body shapes inside `mod inverse_symmetry`.
- `cargo test --workspace`, `cargo clippy -p lopress-editor --all-targets -- -D warnings`, `cargo fmt --all -- --check` all clean.

Stage 3 (migrate paragraph, heading, code widgets to emit `EditBlockBody`; delete `EditInline` and `EditCode`) gets its own plan once this lands.

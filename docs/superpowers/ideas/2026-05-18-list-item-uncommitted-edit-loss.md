# List items lose uncommitted edits on a structural action

**Date:** 2026-05-18
**Status:** bug — root cause found, fix deferred
**Severity:** data loss (typed text discarded)

## Symptom

With several items in a list block, typing into multiple items and then
pressing Enter in one of them adds a new item but **clears the typed text
from every other item** that was edited but not yet committed.

## Reproduction

1. Open a document with a multi-item list.
2. Type into item 1 (do not press Enter/arrows — just type and stop).
3. Type into item 3 the same way.
4. Click into item 2, press Enter.
5. The new (split) item appears, but the text typed in steps 2 and 3 is gone.

Verified live via the debug control server: after the Enter, `/state` shows
the model never received the edits to items 1 and 3.

## Root cause

Each list item is rendered by its own inline editor with a **live buffer**
(`build_block_editor`). Typed text lands in that buffer; it is written to the
document model only when the item is *committed* — and `commit_list_item` is
called only for the **focused** item, only on its own structural keys
(Enter / Backspace / arrows), in `crates/lopress-editor/src/ui/blocks/list.rs`.

A structural action (`SplitListItem`, `MergeListItemWithPrev`) mutates
`current_doc`, which re-renders the whole list block. `editable_list_view`
rebuilds **every** item editor from the model via `v_stack_from_iter`. Any
item whose buffer was never committed is rebuilt from its stale model runs —
the typed text is discarded.

The focused item is fine (it commits itself before the structural action).
Every *other* edited item is lost.

This is **not** caused by the `feat/list-as-full-plugin` plugin migration —
that branch did not change the list widget's commit logic. The defect has
existed since editable lists shipped. Paragraphs share the same latent
weakness (an uncommitted paragraph buffer is also lost on a re-render), but it
is masked because paragraphs are edited and committed one at a time.

## Proposed fix

Before any structural list action, commit **all** of the list's item buffers,
not just the focused one:

1. `editable_list_view` collects each item's editor handles
   (`item_id`, `editor_sig`, `spans_sig`) into a shared structure as the rows
   are built.
2. A new batched action — e.g. `BlockAction::CommitListItems { block_id,
   items: Vec<(BlockId, Vec<InlineRun>)> }` with an `apply_commit_list_items`
   that sets each item's runs — writes every item's current buffer to the
   model in one step.
3. The per-item key handler emits `CommitListItems` (all items) instead of
   `commit_list_item` (focused item only) before `SplitListItem` /
   `MergeListItemWithPrev`.

Undo granularity is unchanged: today an Enter is `EditListItem` + `SplitListItem`
(2 steps); it becomes `CommitListItems` + `SplitListItem` (still 2 steps). The
new action needs an undo inverse, consistent with the existing list actions.

## Deeper note

The underlying tension is per-item live buffers versus a model that is fully
rebuilt on every change. The proposed fix resolves the reported data loss
within the current rebuild-all architecture. A larger redesign (keyed
reconciliation so unchanged item editors persist, or finer-grained reactivity
so non-structural edits do not trigger a full re-render) is a separate,
bigger question and is explicitly out of scope for this bug.

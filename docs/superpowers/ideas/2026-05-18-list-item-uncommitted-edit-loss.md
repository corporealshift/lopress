# List items lose uncommitted edits on a structural action

**Date:** 2026-05-18
**Status:** resolved 2026-05-22 — structurally impossible after stage 4
**Severity:** data loss (typed text discarded)

## Resolution

Fixed by the list-editor-unification refactor
(`docs/superpowers/specs/2026-05-20-list-editor-unification-and-generic-undo-design.md`),
specifically stage 4 (commits `14a31bd` … `62eb887`).

The bug is now **structurally impossible**: list mutations no longer flow
through item-scoped actions like `EditListItem` / `SplitListItem` /
`MergeListItemWithPrev` (all deleted in `62eb887`). Every list edit is a
single `BlockAction::EditBlockBody { block_id, new_body: BlockBody::List(_) }`
carrying the **complete** new list body. The commit closure in
`crates/lopress-editor/src/ui/blocks/list.rs` (`commit_list_from_handles`)
walks every item's live editor handles, builds a fresh `BlockBody::List`
from each item's current buffer, and emits a single `EditBlockBody`. The
structural-key callback runs that commit before any Enter/Backspace
structural change, so no path mutates a list without first capturing every
item's typed text. There is no longer a "focused item committed, others
not" failure mode by construction.

The `CommitListItems` action proposed below was never built — collapsing
to one `EditBlockBody` variant subsumed it.

Original report below kept for context.

---

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

# List Editor Unification and Generic Undo — Design

**Date:** 2026-05-20
**Author:** Kyle
**Status:** Designed, awaiting implementation plan

## Background — why this work exists

Three problems on the lopress editor today, all caused by list items running
on a separate, older editor-mount code path than paragraphs and headings:

1. **Caret bug.** In `crates/lopress-editor/src/ui/blocks/list.rs`,
   `list_item_editor` uses `editor_container_view` with
   `is_active = ed.active.get()`. Floem sets `ed.active` true only between
   pointer-down and pointer-up, so the caret vanishes the moment the mouse
   button is released. This exact bug was fixed for paragraphs and headings
   in commit `653f9fd` by switching to the lower-level `editor_view` with
   explicit `FocusGained` / `FocusLost` tracking via a
   `focused: RwSignal<bool>`. List items never got the migration.
2. **Data-loss bug** (filed in
   `docs/superpowers/ideas/2026-05-18-list-item-uncommitted-edit-loss.md`).
   With multiple items in a list, typing into items 1 and 3, then pressing
   Enter in item 2, *clears* the uncommitted text from items 1 and 3. Root
   cause: each list item is its own editor with a live buffer;
   `commit_list_item` is called only for the focused item on its own
   structural keys. A structural action mutates the doc, the
   `dyn_container` rebuilds every item editor from the model, and any
   uncommitted buffer is silently discarded.
3. **Missing keyboard shortcuts in list items.**
   `handle_list_item_key` returns `CommandExecuted::No` for any Ctrl/Cmd
   key and falls through to Floem's default handler, which does not
   implement Ctrl+B/I/E/K (style toggles) or Ctrl+Z/Y (undo/redo). So
   bold/italic/code/link/undo/redo from the keyboard do not work inside
   list items.

All three are downstream of the same architectural gap: paragraphs and
headings flow through the unified `editable_inline` in
`crates/lopress-editor/src/ui/blocks/inline_editor.rs`; list items do not.
Closing this gap is the work.

While scoping the data-loss fix and its effect on the action enum and
`undo.rs`, the user pushed for a larger architectural cleanup: the undo
system today knows about each block type's internal mutation shape via a
110-line `compute_inverse` function and four post-apply `fix_*` methods.
Adding `CommitListItems` / `RemoveListItem` / `InsertListItem` would grow
both. Every future block type would grow both again. The user explicitly
asked for the bigger architectural change now while there are only a few
block types, instead of accreting more per-type variants. That decision is
captured in section 3 below.

---

## Section 1 — Shared mount helper

Extract the editor mounting work from `editable_inline` (in
`crates/lopress-editor/src/ui/blocks/inline_editor.rs`) into a new function
— call it `mount_block_editor` — that owns everything not specific to
paragraph behavior:

- The `editor_view(editor_sig, |_| focused.get())` mount (the lower-level
  one, not `editor_container_view`).
- The `focused: RwSignal<bool>` plus `FocusGained` / `FocusLost`
  listeners.
- The pointer-down / move / up listeners.
- The `KeyDown` listener that combines a key handler with character
  insertion.
- The Ctrl+Z / Y / B / I / E / K shortcuts (style toggles, undo, redo,
  link-URL row opening).
- The Ctrl+Home / End / PageUp / PageDown block-jumping (uses the
  supplied `commit` closure, then sets `focus_target`).
- The `editor_view_id` registration on the `Editor`.
- The `editor_view_focused` / `editor_view_focus_lost` notifications.
- The `focus_target` programmatic-focus effect.
- The `focus_pub` publish effect for the toolbar.
- The height-from-visual-lines styling.
- `CARET_COLOR` and the text-cursor style.

It accepts these parameters (intent, not Rust-exact signature):

```text
mount_block_editor(
    state: BlockEditorState,
    block_id: BlockId,
    on_action, focus_target, focus_pub, current_doc,
    on_undo, on_redo,
    commit: Rc<dyn Fn()>,
        // Called before any focus-changing or block-jumping shortcut.
    structural_key: Rc<dyn Fn(&KeyPress, Modifiers) -> Option<CommandExecuted>>,
        // Called BEFORE shared Ctrl-shortcuts.
        // Some(Yes) = handled; None = fall through to shared logic.
    slash_eligible: bool,
        // Paragraphs only: '/' on empty block opens the slash menu.
)
```

Key-handling order inside the shared mount:

1. `structural_key(...)` — returns `Some(Yes)` if the caller fully
   handled it.
2. If `None`, fall through to shared logic:
   1. Ctrl-shortcuts (style toggles, undo, redo, Ctrl+Home/End).
   2. Slash trigger (when `slash_eligible` and the block is empty).
   3. Default Enter (block-split via `commit` then `Split`).
   4. Backspace-at-0 (commit then `MergeWithPrev`).
   5. ↑ / ↓ on first / last vline (commit then cross-block nav).
   6. PageUp / PageDown.

Wrappers:

- **Paragraph wrapper** (`render_paragraph_editable`): supplies
  `structural_key = |_, _| None`, `slash_eligible = true`, and a `commit`
  closure that builds the new paragraph body and emits `EditBlockBody`
  (section 3). Behavior identical to today.
- **List item wrapper** (`list_item_editor`): supplies the thin
  `structural_key` callback from section 2, `slash_eligible = false`, and
  a `commit` closure that flushes *every* item's buffer into a new list
  body and emits `EditBlockBody` for the whole list block (section 3).

Subtlety to preserve: list items publish the *block*'s id to
`focus_pub.block` but expose the *item* editor's handles to
`focus_pub.editor_and_spans`. The caller supplies the publish closure;
paragraphs use `block_id` with the paragraph editor's handles, lists use
`block_id` with the item editor's handles. Total parameter count goes up,
but everything is data flowing in — no per-type knowledge inside the
shared mount.

---

## Section 2 — List-item structural-key callback

The list-item-specific key handler shrinks to just the cases where
item-level semantics differ from block-level. Everything else falls
through to the shared mount. List blocks are **keyboard-isolated**:
arrows cannot leave the list; Backspace cannot lift content out of it.
The only keyboard ways out of a list are Ctrl+Home / End and
PageUp / PageDown (handled by the shared mount).

Full behavior table (no Ctrl/Cmd modifier):

| Key                          | Condition                                                    | Action                                                                                                                       | Return     |
|------------------------------|--------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------|------------|
| `Shift+Enter`                | always                                                       | (fall through — shared handler inserts `\n`)                                                                                 | `None`     |
| `Enter`                      | always                                                       | batched commit + new list body with item split at `byte_offset` → emit `EditBlockBody`; `focus_target = new item id`         | `Some(Yes)`|
| `Backspace` at offset > 0    | any                                                          | (fall through — default handler deletes char)                                                                                | `None`     |
| `Backspace` at offset 0      | `item_index > 0`                                             | batched commit + new list body with this item merged into the previous → emit `EditBlockBody`; `focus_target = prev item id` | `Some(Yes)`|
| `Backspace` at offset 0      | `item_index == 0`, item is **empty**, list has ≥ 2 items     | batched commit + new list body with this item removed → emit `EditBlockBody`; `focus_target = new first item id`             | `Some(Yes)`|
| `Backspace` at offset 0      | `item_index == 0`, item is **empty**, list has 1 item        | emit `Delete { block_id }` (the whole list block goes away)                                                                  | `Some(Yes)`|
| `Backspace` at offset 0      | `item_index == 0`, item is **non-empty**                     | consume, no-op                                                                                                               | `Some(Yes)`|
| `↑` not on first vline       | any                                                          | (fall through — default handler moves cursor up within item)                                                                 | `None`     |
| `↑` on first vline           | `item_index > 0`                                             | batched commit + `focus_target = prev item id`                                                                               | `Some(Yes)`|
| `↑` on first vline           | `item_index == 0`                                            | consume, no-op                                                                                                               | `Some(Yes)`|
| `↓` not on last vline        | any                                                          | (fall through)                                                                                                               | `None`     |
| `↓` on last vline            | `item_index + 1 < item_count`                                | batched commit + `focus_target = next item id`                                                                               | `Some(Yes)`|
| `↓` on last vline            | last item                                                    | consume, no-op                                                                                                               | `Some(Yes)`|
| anything else                | —                                                            | (fall through)                                                                                                               | `None`     |

Three consequences worth recording:

1. The list is keyboard-isolated. No accidental list-deletion via arrow
   keys or Backspace.
2. Ctrl-shortcuts and Page navigation work in list items because the
   structural callback returns `None` for them and the shared mount
   handles them.
3. The shared mount's `commit` closure (used for Ctrl+Home/End, Page
   keys, cross-block ↑/↓ from the *outer* boundary, and any other path
   the shared mount runs) is the batched-commit-all-items closure — so
   every commit point for a list flushes the whole list. No data-loss
   holes on any path.

---

## Section 3 — Generalize the action/undo layer (the architectural shift)

Two combined shifts that together eliminate per-block-type knowledge from
`undo.rs` and collapse the content-action surface to a single variant.

### Shift A — `apply` returns the inverse it just performed

Change the signature in `crates/lopress-editor/src/actions.rs` from
`pub fn apply(doc: &mut EditorDoc, action: BlockAction)` to:

```text
pub fn apply(doc: &mut EditorDoc, action: BlockAction) -> Option<BlockAction>
```

Each apply arm reads pre-state, mutates, and returns the action that would
restore the prior state. It has access to both pre-state (because it just
observed it) and post-state (newly-minted ids in particular). Consequences:

- `compute_inverse` in `crates/lopress-editor/src/undo.rs` disappears.
- The four post-apply fix-up methods — `fix_split_inverse`,
  `fix_split_list_item_inverse`, `fix_merge_redo`,
  `fix_merge_list_item_redo` — disappear.
- `Split` carries a new field `new_block_id: Option<BlockId>`: `None`
  means mint a fresh id; `Some` means use this id. This makes undo→redo
  round-trips id-stable without any post-apply surgery.
- The undo stack's `push_before_apply` becomes
  `push_after_apply(action, inverse)`. The chokepoint in
  `crates/lopress-editor/src/ui/mod.rs` calls `apply` and pushes the
  returned `(action, inverse)` pair.

Coalescing logic for typing bursts stays — it is a UX choice independent
of inverse computation — and reduces to "same `block_id` within the
1-second window." Generic across block types.

### Shift B — Collapse content edits into one action

Replace `EditInline`, `EditCode`, `EditListItem`, `SplitListItem`, and
`MergeListItemWithPrev` with a single new variant:

```text
BlockAction::EditBlockBody { block_id, new_body: BlockBody }
```

The block widget — the thing that knows how its body works — constructs
the desired new body locally and emits the action. The apply arm
replaces the block's body and returns
`EditBlockBody { block_id, new_body: old_body }` as the inverse. One arm.
One inverse rule. Works for any current or future body shape.

Concrete examples:

- **Bold a paragraph selection** → editor builds
  `BlockBody::Inline(new_runs)` → emits `EditBlockBody`.
- **Type in a code block** → editor builds
  `BlockBody::Code(new_text)` → emits `EditBlockBody`.
- **Type in a list item** → editor builds
  `BlockBody::List(items_with_this_item_updated)` → emits
  `EditBlockBody`. Other items' live buffers are flushed into the same
  new body in the same call. **The data-loss bug becomes structurally
  impossible** — there is no way to mutate a list block other than by
  passing in its complete new body, so "uncommitted items" cannot exist
  by construction.
- **Enter in a list item** → editor builds
  `BlockBody::List(items_with_new_item_inserted_at_split)`, mints the
  new item's id locally, sets `focus_target = new_id`, emits
  `EditBlockBody`. No `SplitListItem` action needed.
- **Backspace at offset 0 of an empty item 0** with > 1 items remaining
  → editor builds new body with that item removed, emits `EditBlockBody`.
- **Backspace at offset 0 of an empty item 0** with exactly 1 item
  → editor emits `Delete { block_id }` (whole-block change, not body
  change). An empty list is unrepresentable in markdown, so the block
  must go.

### Final `BlockAction` enum after both shifts

Block-list-shape changes that affect `doc.blocks` order/identity, plus
the unified content action, plus the UI-only menu action:

- `Split { block_id, byte_offset, new_block_id: Option<BlockId> }`
- `MergeWithPrev { block_id }`
- `InsertAfter { anchor, new_block }`
- `Delete { block_id }`
- `Move { block_id, to_index }`
- `ChangeType { block_id, new_kind }`
- `EditAttrs { block_id, new_attrs }`
- `EditBlockBody { block_id, new_body }` ← new, replaces five variants
- `OpenSlashMenu { block_id }` ← UI-only, unrecorded in undo

Eight content-bearing variants instead of today's twelve. Adding a new
block type (table, embed, callout, etc.) adds zero variants — its widget
just emits `EditBlockBody` with the right body shape.

### Trade-offs recorded explicitly

- **Payload size.** Today `EditListItem` carries one item's runs. After
  the shift, `EditBlockBody` for a list carries the whole list body. A
  100-item list ships ~100× as many runs per action. Lopress documents
  are blog-post-shaped (lists rarely exceed ~20 items) and
  `MAX_UNDO = 100`, so worst-case extra memory is bounded and small
  (back-of-envelope: hundreds of KB for pathological cases). The recent
  perf work T7 (commit `895287c`) eliminated a per-action *full-doc*
  clone; this change is per-action *one-block-body* clone — an order of
  magnitude less than the regression that was fixed.
- **Editor responsibility grows.** The widget constructs the target
  body instead of declaring intent. That's more code in widgets but
  it's the right place for it. It also removes the awkward "widget says
  split here / apply mints id / widget learns id back" dance that the
  four `fix_*` methods exist to paper over.
- **Ctrl HTTP API (`/action` handler at
  `crates/lopress-editor/src/ctrl/mod.rs:398-408`).** Translates
  incoming verb-shaped requests into the new enum. The public ctrl API
  can keep its verb shapes; translation lives in the handler. Small
  change.
- **Migration cost.** Real but contained. Action enum + apply net
  negative LoC (twelve arms collapse, post-fix methods deleted). Block
  widgets each gain a "build new body" helper. Every test site that
  built specific action variants is revised.

### Effect on sections 1 and 2

- Section 1's `mount_block_editor` extraction is unchanged.
- Section 2's structural-key callback intent is unchanged, but every
  "emit `SplitListItem` / `MergeListItemWithPrev` / `CommitListItems`"
  line becomes "build the new list body and emit `EditBlockBody`."
- The `RemoveListItem` and `InsertListItem` actions earlier sketched in
  the brainstorm are dropped — empty-first-item Backspace is either
  `EditBlockBody` (list shrinks by one item) or `Delete { block_id }`
  (list block disappears entirely if that was the last item).

---

## Section 4 — Migration order, test strategy, risk

### Staging — six reviewable chunks, each leaves the editor working

1. **Inverse-from-apply** (pure refactor, no behavior change). Change
   `apply`'s signature to return `Option<BlockAction>`. Move each
   existing `compute_inverse` arm into the matching apply arm. Delete
   `compute_inverse`. Add `new_block_id: Option<BlockId>` to `Split`
   (and `SplitListItem` for now — both still exist at this stage) so
   undo→redo is id-stable; delete the four `fix_*` methods. Existing
   test suites stay green throughout — no semantics change.
2. **Add `EditBlockBody`** (purely additive). New variant, new apply
   arm with body-swap + inverse, new tests for symmetry across
   `Inline` / `Code` / `List` body shapes. Old variants still exist and
   work. No widget changes yet.
3. **Migrate paragraph, heading, and code** widgets to construct the new
   body locally and emit `EditBlockBody`. Once a variant has no
   remaining emit sites, delete it (`EditInline`, then `EditCode`).
   Tests in `actions_tests` and `from_to_core_tests` rewrite their
   fixture builders.
4. **Migrate list** — combines sections 1, 2, and the body-build
   pattern. Extract `mount_block_editor`. Move `list_item_editor` onto
   it. Implement the structural-key callback from section 2 with each
   branch building a new `BlockBody::List(...)` from the live per-item
   buffers and emitting `EditBlockBody` (or `Delete { block_id }` for
   the empty-last-item case). Delete `EditListItem`, `SplitListItem`,
   `MergeListItemWithPrev`. The data-loss bug, the caret bug, and the
   missing Ctrl-shortcuts in list items are all gone at the end of this
   stage.
5. **Ctrl API translation.** Update `/action` at
   `crates/lopress-editor/src/ctrl/mod.rs:398-408` to accept its
   current verb-shaped requests and translate to `EditBlockBody` or the
   appropriate structural action internally. Run existing ctrl
   integration tests (exercised by the `driving-lopress-editor` debug
   control workflow). While here, address finding #2 from
   `docs/superpowers/ideas/2026-05-15-editor-assessment.md` if it is
   not already fixed — ctrl edits must trigger save through the same
   `on_action` chokepoint as the UI, so `mark_dirty` and `focus_target`
   updates work uniformly.
6. **Cleanup.** Remove now-unused helpers (e.g. `commit_list_item`, the
   `compute_inverse`-related types). Sweep docs. Update the
   editor-assessment idea doc and the list-item-uncommitted-edit-loss
   idea doc to mark the resolved items.

### Test strategy

- **Existing suites stay green at every stage.** `from_to_core_tests`,
  `actions_tests`, `undo_tests`, `list_plugin_meta_tests`,
  `block_decl_tests`, all workspace integration tests. CI runs after
  every commit.
- **New tests in stage 2.** `EditBlockBody` apply + inverse symmetry
  for every body shape (paragraph, heading, code, list with 0 / 1 / N
  items). Property-style: `apply(apply(doc, a)?, inverse)` recovers the
  original doc.
- **New tests in stage 4** for the data-loss regression specifically:
  build a list of N items with simulated per-item buffer state,
  dispatch the Enter-on-item-2 path, assert all items retain their
  buffered text in the resulting doc.
- **Live verification** via the debug control server at the end of
  stage 4: open a real list document, drive typing into items 1 and 3
  via `/input`, hit Enter in item 2 via `/key`, read `/state`, confirm
  runs match expectations. Same session: drive a
  pointer-down → up → focus-still-here sequence on a list item and
  confirm caret remains visible (the symptom of issue #1).
- **Manual verification** via the running editor for feel: typing in
  list items (caret visible, no lag), Enter inserts a new item (never
  closes the list), ↑ / ↓ at list boundaries do nothing, Backspace at
  top of empty first item removes the item (or the block if last),
  Ctrl+B / I / Z / Y all work in list items.

### Risk register

- **Regression scope is the biggest risk.** Six stages, each landing a
  discrete behavior-preserving change, contain it. If stage 1 misfires,
  revert one PR; the broader collapse does not start until that lands
  cleanly.
- **Id stability across undo ↔ redo.** Mitigated by
  `new_block_id: Option<BlockId>` carrying the stable id. Stage 1 tests
  explicitly exercise undo→redo round trips on `Split` and
  `MergeWithPrev`.
- **Ctrl API consumers.** Only the ctrl integration tests and any
  scripts driving the editor. Translation layer covers the existing
  surface; public verb shapes do not change.
- **Performance.** Per-action body clone instead of per-action
  item-runs clone. Invisible for typical documents; bounded for
  pathological cases. Re-run the perf template in
  `docs/superpowers/plans/2026-05-19-performance-findings.md` if
  anything feels off after stage 4.
- **Explicitly out of scope:** the rebuild-all-on-structural-change
  pattern in `dyn_container`. Keyed reconciliation or finer-grained
  reactivity is a separate, larger redesign. The data-loss fix works
  within the current rebuild-all architecture because every structural
  or focus-changing list action carries the full new body in its
  `EditBlockBody` payload.

---

## Resolved decisions and tradeoffs

- **List keyboard isolation chosen over cross-boundary nav.**
  User-directed: ↑ at top and ↓ at bottom of a list do nothing; Enter
  never closes a list; Backspace doesn't lift content out. Rejected:
  today's "lift first item into previous block on Backspace" behavior
  from commit `15f73c9`.
- **Empty-first-item Backspace deletes the item only.** Rejected: a
  pure no-op even when empty; and lifting content into the previous
  block. If the empty item is the only item, the whole list block is
  deleted (an empty list is unrepresentable in markdown).
- **Big architectural refactor of action/undo chosen over a localized
  fix.** The user explicitly preferred fixing the per-block-type leak
  in `undo.rs` now ("while there are only a few block types"). Rejected:
  adding `CommitListItems` + `RemoveListItem` + `InsertListItem` as new
  per-type variants, which would have grown the per-type knowledge in
  `compute_inverse` and required new `fix_*` methods.
- **`apply` returns the inverse vs. keep `compute_inverse` separate.**
  Chose returning-from-apply because the relevant state (pre-state
  observed in place, post-state including newly-minted ids) is
  naturally available inside apply. Rejected: keeping `compute_inverse`
  separate, which forced the four post-apply fix-up methods.
- **One unified `EditBlockBody` variant vs. keeping per-type content
  actions with apply-returned inverses.** Chose the full collapse for
  the smaller enum and the structural impossibility of partial-commit
  bugs. Rejected: keeping per-type variants ("approach F" in the
  brainstorm), which would have removed `compute_inverse` but left the
  enum growing with each block type.
- **Per-action full body clone accepted as a payload cost.** Rejected:
  diff-based action payloads (too much complexity for the size of
  lopress's documents); `Arc<BlockBody>` cheap-clone bodies (bigger
  refactor; the doc model is not ready for it; revisit if profiling
  ever shows the body clone hurting).
- **Shared mount helper takes a `commit` closure and a `structural_key`
  callback.** Rejected: pushing list-item-specific behavior into the
  shared mount itself (would re-introduce per-type knowledge in the
  shared layer); pulling the shared logic up via traits (overkill for
  two-callsite reuse).
- **Caret + Ctrl-shortcuts fix in list items rides on the shared mount
  migration.** Rejected: a minimal-touch fix that adds focus tracking
  to `list_item_editor` without unifying. Would leave the duplicated
  mount, two key handlers, and the latent risk that list items keep
  drifting behind paragraphs.

---

## Non-goals

- No change to the on-disk markdown format.
- No change to the rebuild-all-on-structural-change `dyn_container`
  pattern (keyed reconciliation is a separate, future redesign).
- No change to the public verb shapes of the ctrl HTTP API.
- No change to plugin manifest format or the editor registry signature
  (`editor_for` and `EditorWidget` stay as they are).

## Open questions for Claude

None at write-time. All four sections were approved by the user during
brainstorming; the spec records the resolved positions.

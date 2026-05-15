# Editor Assessment Fixes — Design Spec

**Date:** 2026-05-15
**Status:** approved
**Source:** `docs/superpowers/ideas/2026-05-15-editor-assessment.md`

Nine of the ten assessment issues are addressed here in priority order.
Issue #5 (Enter in List / editable list items) is a larger standalone project
with its own spec: `2026-05-15-editable-list-base-plugin-design.md`. Lists
remain read-only until that work is done.

---

## 1. Scroll-to-cursor (#1 — Critical)

In `inline_editor.rs`, the focus effect already calls `view_id.request_focus()`
when `focus_target` matches this block. Add a single call immediately after:

```rust
view_id.scroll_into_view();
```

Floem's `ViewId` walks the view tree upward to find the nearest scroll ancestor
and reveals the element. No new signals or plumbing needed — the existing
`focus_target` mechanism carries the fix.

---

## 2. Ctrl API save fix (#2 — Critical)

In `ui/mod.rs` lines 397–408, the debug ctrl effect calls
`crate::actions::apply(doc, block_action)` directly, bypassing `on_action`.
Fix: read the doc to resolve block ids, then route through `on_action`:

```rust
let block_action = current_doc.with_untracked(|d| {
    ctrl_action.into_block_action(d.as_ref()?)
});
if let Some(action) = block_action {
    on_action_for_ctrl(action);
}
```

This simultaneously fixes both reported bugs: `mark_dirty()` fires (triggering
the 500 ms debounced save), and `focus_target` updates correctly for
Split/ChangeType/MergeWithPrev since `on_action` already handles those cases.

---

## 3. Undo / redo (#6 — Functional)

### 3.1 Storage

`UndoStack` lives in `EditingState`, one per open document session:

```rust
struct UndoStack {
    undo: VecDeque<UndoEntry>,              // max 100; front = oldest
    redo: Vec<UndoEntry>,
    last_inline_edit: Option<(BlockId, Instant)>,  // for coalescing
}

struct UndoEntry {
    action:  BlockAction,   // original (for redo)
    inverse: BlockAction,   // computed at apply-time (for undo)
}
```

### 3.2 Inverse computation

Computed inside `on_action` before `apply()` is called, while pre-state is
readable:

| Action | Inverse |
|---|---|
| `EditInline { block_id, new_runs }` | `EditInline { block_id, old_runs }` |
| `EditCode { block_id, new_text }` | `EditCode { block_id, old_text }` |
| `Split { block_id, byte_offset }` | `MergeWithPrev { new_block_id }` — read from post-state |
| `MergeWithPrev { block_id }` | `Split { prev_id, prev_text_len }` — captured from pre-state |
| `Delete { block_id }` | `InsertAfter { anchor, full_block_clone }` |
| `InsertAfter { anchor, new_block }` | `Delete { new_block.id }` |
| `Move { block_id, to_index }` | `Move { block_id, original_index }` |
| `ChangeType { block_id, new_kind }` | `ChangeType { block_id, old_kind }` |
| `EditAttrs` | `EditAttrs` with old attrs |
| `OpenSlashMenu` | not recorded (UI-only) |

### 3.3 Coalescing

Consecutive `EditInline` actions on the same block within 1 second are merged
into one undo entry: the entry keeps `old_runs` from the first action and
updates `new_runs` to the latest. Structural actions (Split, Delete, Move,
ChangeType, MergeWithPrev) always push a fresh entry regardless of timing.

### 3.4 Keyboard

`Ctrl+Z` and `Ctrl+Y` / `Ctrl+Shift+Z` are intercepted in `handle_key`
alongside `Ctrl+B/I/E/K`. Two new callbacks — `on_undo: Rc<dyn Fn()>` and
`on_redo: Rc<dyn Fn()>` — are threaded from `editing_view` through
`editor_pane` down to `editable_inline`. Undo/redo apply the inverse/original
action directly via `current_doc.update(|d| apply(d, ...))`, bypassing
`on_action` to avoid re-recording. `mark_dirty()` is called after each.

Performing a new normal edit clears the redo stack (standard linear behaviour).

---

## 4. Slash menu keyboard trigger (#3 — Functional)

In `inline_editor.rs`, remove the `_` prefix from `_slash_eligible` and thread
it into `handle_key`. Before the main `match &kp.key` block, intercept `/` when
eligible and the block text is empty:

```rust
if !ctrl_or_cmd {
    if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
        if s == "/" && slash_eligible {
            let empty = editor_sig.with_untracked(|ed| ed.doc().text().len() == 0);
            if empty {
                on_action(BlockAction::OpenSlashMenu { block_id });
                return CommandExecuted::Yes;
            }
        }
    }
}
```

`slash_eligible` is `true` for Paragraph blocks, `false` for Heading and Code.

---

## 5. Link URL tooltip (#4 — Functional)

### 5.1 State

One `RwSignal<Option<String>>` per block editor for the link URL currently
being edited. Set when a link toggle activates (selection gains a span with
`link: Some("")`), cleared on commit or remove.

### 5.2 Rendering

The toolbar slot in `block_view` becomes a `v_stack` of two conditional rows:

1. **Existing row** — kind buttons + B/I/code/Link toggles + delete (unchanged)
2. **URL row** — visible when the current selection (collapsed or not) overlaps
   any span with `link: Some(_)`. A collapsed cursor inside a link span also
   triggers the row, so clicking into an existing link immediately shows its URL.
   Contains a text input pre-filled with the existing URL and a "Remove" button.

No absolute positioning — the URL row is simply the second row of the toolbar
`v_stack`, appearing and disappearing reactively.

### 5.3 Committing

On Enter or focus-loss from the URL input, the URL is written into all link
spans touching the selection via `BlockAction::EditInline` with updated runs.
"Remove" calls `apply_style_toggle(InlineFlag::Link)` to clear the link flag.

### 5.4 Initial trigger

`Ctrl+K` and the "Link" toolbar button already call
`apply_style_toggle(InlineFlag::Link)`. After toggling on, the URL row appears
automatically because the selection now contains a link span — no extra wiring
needed.

---

## 6. Title/H1 divergence warning (#8 — Inspector)

`inspector_view` already reads `current_doc` reactively. A derived signal
compares `front_matter.title` with the plain text of the first `Heading(1)`
block (joining its inline runs). When they differ and an H1 exists, a `⚠`
label and a "Sync from H1" button appear next to the title field. The button
writes the H1 text into the title field and calls `mark_dirty()`.

No auto-sync — the warning is passive until the user acts.

---

## 7. Description / excerpt field (#9 — Inspector)

`FrontMatter` gains a `description: Option<String>` field. `from_core` reads it
from the existing front-matter map (key `"description"`); `to_core` writes it
back. The inspector gains a "Description" multi-line text input below the Title
field. Editing calls `mark_dirty()`.

No changes to `lopress-core` parse/serialize — front-matter is already a
passthrough map.

---

## 8. Navigation shortcuts (#7 — Navigation)

In `handle_key`, alongside `Ctrl+B/I/E/K`:

| Shortcut | Behaviour |
|---|---|
| `Ctrl+Home` | `commit_from_editor` + `focus_target` → first block |
| `Ctrl+End` | `commit_from_editor` + `focus_target` → last block |
| `Page Up` | `commit_from_editor` + `focus_target` → 10 blocks back (clamped) |
| `Page Down` | `commit_from_editor` + `focus_target` → 10 blocks forward (clamped) |

"commit" here means `commit_from_editor` — writes the current block's runs back
to the doc via `BlockAction::EditInline` before moving focus, consistent with
the existing ↑/↓ cross-block navigation pattern.

Page Up/Down land at the start of the target block (no x-position
preservation — deferred until a caret-x cache exists).

---

## 9. H4–H6 in toolbar (#10 — Toolbar)

Three entries appended to the `kinds` vec in `toolbar.rs`:

```rust
("H4", BlockKind::Heading(4)),
("H5", BlockKind::Heading(5)),
("H6", BlockKind::Heading(6)),
```

If the toolbar row becomes too wide, the deferred option is a collapsible "H"
group button that expands to H1–H6 on click.

---

## Implementation order

1. Scroll-to-cursor (#1)
2. Ctrl API save fix (#2)
3. Undo/redo (#6)
4. Slash menu keyboard trigger (#3)
5. Link URL tooltip (#4)
6. Title/H1 warning (#8) + description field (#9) — implement together
7. Navigation shortcuts (#7)
8. H4–H6 toolbar (#10)

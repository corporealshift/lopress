# Editor UI Review — 2026-05-26

Findings from a live UI sweep using the debug HTTP control API on the
`feat/code-editor-block` branch (commit `849c418`). Document driven was
`lopress-listtest/src/posts/listtest.md` (1 H1 + 1 paragraph + 1 ten-item
list + 1 paragraph; a code block was added mid-sweep via `/action`).

Findings are grouped by severity. None of the open items below appear in
prior `docs/superpowers/ideas/` notes — this is a fresh pass.

---

## Functional bugs

**1. Slash menu doesn't open when `/` is typed on an empty paragraph**
`crates/lopress-editor/src/ui/blocks/inline_editor.rs:480-490`

Repro:
1. Click into any non-empty paragraph; press `End` then `Enter` (creates a
   new empty paragraph and routes focus to it).
2. Or, `POST /action {"type":"Split","block_id":<id>,"byte_offset":<len>}`
   then click into the new empty block.
3. Type `/`.

Expected: slash-menu popup. Observed: the literal `/` character is inserted
into the block. The interception path is wired (commits `bc55680` and
earlier) — `slash_eligible` is `true` for paragraphs, and the handler
checks `editor_sig.with_untracked(|ed| ed.doc().text().is_empty())` before
inserting. So one of three things is happening:

- The `is_empty` read sees a non-empty buffer at the moment the `/`
  arrives (initial empty-runs body materializes to `"\u{0}"` or similar
  sentinel before the first edit?). Possible if `build_block_editor`
  seeds the editor with a placeholder run.
- The character event arrives via a path that bypasses `combined_key`
  (e.g., floem's IME or composition path).
- The `/` key is routed before focus has actually landed on the new
  block — the visual cursor is present and `text` typed after this same
  click *does* land in block 18, so this is unlikely but worth ruling out
  with a `dbg!(slash_eligible, is_empty, block_id)` print.

This regresses the resolved finding #3 in
`docs/superpowers/ideas/2026-05-15-editor-assessment.md` (the wiring is
there, but the trigger is now no-op in practice).

**2. Toolbar block-type buttons don't apply when invoked via the `/click`
control API; behavior on real mouse clicks needs confirmation**
`crates/lopress-editor/src/ui/toolbar.rs:79-98`

Repro via ctrl:
1. Click into a paragraph: `POST /click {"x":300,"y":134}`. Confirm the
   toolbar appears at `y≈128`.
2. Click an H1/H2/H3/Code/UL/OL/H4/H5/H6/Delete button position.
3. `/state` shows no `BlockKind` change. The block defocuses (toolbar
   disappears) and the dirty indicator switches to `unsaved` despite no
   structural change.

The inspector's **Sync from H1** button at the top-right `does` fire
correctly under `/click`, so this is not a generic `/click` limitation —
it is something specific to the focused-block toolbar. Two hypotheses:

- The toolbar is rendered as an overlay above the editor view but
  pointer events are still routed to the underlying editor canvas
  (z-order without pointer-event capture). The visible "defocus on click"
  suggests the click reaches the editor canvas behind the toolbar, blurs
  it, and never fires the button's `.action()`.
- `floem::views::button` may swallow the click but the editor's
  `FocusLost` commit (recent fix `0efe124`) races the button action and
  the `on_action_for_btn(ChangeType {...})` call reads stale state.

This needs human confirmation with a real mouse before fixing — if the
buttons work under real input and only `/click` injection fails, the bug
is in `ctrl/input.rs::send_click` (no `ensure_foreground`; bare
`SetWindowPos`/`SendInput` mouse events without activation). If real
clicks fail too, it's a pointer-capture / hit-testing issue in the
toolbar layout. Either way, this blocks Claude from driving the toolbar.

**3. Front-matter changes (Title, Slug, Date, Tags, Description,
"Sync from H1") are not in the undo stack**
`crates/lopress-editor/src/ui/mod.rs` + `crates/lopress-editor/src/undo.rs`

Repro:
1. Click "Sync from H1" — title field updates from "List Test" to
   "List Test Document". Document marks dirty and saves.
2. Press Ctrl+Z (focus on the editor pane).
3. Title field remains "List Test Document".

`UndoStack` only records `BlockAction` inverses produced by
`actions::apply`. The inspector edits front-matter through a separate
path (`inspector::front_matter_view` → direct field mutation) that never
touches the action sink. Result: any front-matter edit is a permanent
operation, including the convenience "Sync from H1" button which has no
undo affordance.

Fix shape: add front-matter mutations to the same undo stack via a new
`BlockAction::EditFrontMatter { new_meta: Metadata }` or a parallel
`FrontMatterAction` enum threaded through `on_action`. The latter keeps
`BlockAction` focused on block-tree edits.

**4. Code-block `lang` text input commits on blur but not on Enter**
`crates/lopress-editor/src/ui/blocks/code_editor.rs` (lang `text_input`
wiring; see commit `fe70b6c` "replace lang label dyn_container with
always-on text_input")

Repro:
1. Focus a code block. Click on the `python`/`rust`/etc. lang label
   (top-right of the block).
2. Select all (Ctrl+A) and type a new lang (e.g., `python`).
3. Press Enter. `/state` still reports the old lang.
4. Click anywhere outside the input. `/state` now reflects the new lang.

Every other text input in the editor that takes a discrete value
(toolbar URL field — `toolbar.rs:204-212`) commits on Enter. The lang
input should match. Listen for `EventListener::KeyDown` + `NamedKey::Enter`
and call the same commit closure that `on_blur` uses.

Bonus: pressing Escape should revert the buffer to the last committed
value rather than commit.

**5. Lang-input edits aren't recorded in the undo stack**
Same area as #4.

Repro:
1. Change lang `rust` → `python` and blur to commit.
2. Press Ctrl+Z (focused inside a block editor — so the keybinding
   reaches `on_undo`).
3. `/state` still shows `python`.

The lang commit fires `BlockAction::EditAttrs { new_attrs: {"lang": ... } }`
which `apply_edit_attrs` *does* return an inverse for — yet the change
doesn't undo. Suspect the commit path is using a side channel
(direct mutation on `block.kind` / `block.plugin.attrs`) rather than
routing through `on_action`. Worth grepping for `BlockKind::Code { lang`
assignments in `code_editor.rs` and confirming the commit dispatches the
action via the sink.

---

## UX / visual

**6. Focusing a block jumps the document layout by ~32–45 px**
`crates/lopress-editor/src/ui/toolbar.rs` + `editor_pane.rs`

The block toolbar is rendered above the focused block inline with the
flow. Moving focus from block A to block B removes the toolbar from
above A and inserts it above B — every other block on the page shifts
vertically by the toolbar's height + margin (~32 px). On a long document
this is jarring: clicking a block makes the rest of the document jump,
and a fast click sequence will land on a different block than the user
aimed at.

Options:
- Render the toolbar as an absolute-positioned floating overlay anchored
  to the focused block's bounding box (no layout impact when the anchor
  changes).
- Reserve a fixed-height slot above every block (always present, only
  populated for the focused block). Wastes vertical space.
- Move the toolbar to a sticky bar at the top of the editor pane (no
  layout shift; loses the proximity-to-content affordance).

The overlay variant is the standard solution and matches how most
WYSIWYG editors (TipTap, ProseMirror, Notion) handle this.

**7. Toolbar button order: H4/H5/H6 are placed AFTER Code/UL/OL instead
of grouped with H1/H2/H3**
`crates/lopress-editor/src/ui/toolbar.rs:55-71`

Current order: `P · H1 · H2 · H3 · Code · UL · OL · H4 · H5 · H6 · | · B · I · </> · Link · | · x`

H4/H5/H6 were added in a later pass (resolving editor-assessment #10)
without re-sorting the row. Heading levels should be contiguous —
either `P · H1 · H2 · H3 · H4 · H5 · H6 · Code · UL · OL` (full row) or
collapse H4–H6 behind an overflow `…` menu since they're rarely used.
The current ordering visually implies H4–H6 are a different *kind* of
control than H1–H3.

**8. Empty list items render as bare bullets with no clickability
affordance**

A list block can contain items whose `runs` are empty (the open document
has 4 of them). Visually these render as a bullet glyph with no
horizontal extent — there's no placeholder text, no caret hint, and the
bullet is small enough that clicking near it usually lands in the gap
between items rather than on the item. Users would not know they can
type into those rows.

This is also relevant to authoring: if the user accidentally creates
empty items (Enter + Backspace pattern), they accumulate invisibly until
the user notices the gap in spacing.

Two complementary fixes:

- Render empty items with a faint placeholder (e.g. greyed-out
  "Type or press Enter to delete") inside the item, hit-testable for
  click-to-focus.
- Auto-collapse runs of empty items at save time, or surface a count in
  the status bar ("4 empty list items"). The current behavior is
  preserved on disk (empty `\n` lines in the markdown).

**9. The block toolbar appears with no spacing inside the focused
block's box — the type buttons sit directly on the block's top border**
See screenshot `.review-shots/13-para-focus.png`.

Compare with the URL input row (which uses `padding_horiz(6).padding_vert(4)`
and a clean gap). The button-row toolbar is drawn flush against the
block content with only the `margin_bottom(4)`; the result looks like
the toolbar is *part of* the block rather than a floating control. The
block's blue focus outline also frames both the toolbar and the
content, which compounds the "they're the same widget" impression.

Either give the toolbar its own background panel with shadow/border
separation from the block, or move it to a floating overlay as in #6.

**10. Status bar misreports `unsaved` after focus changes that don't
edit anything**
`crates/lopress-editor/src/ui/save_pipeline.rs` (the `mark_dirty` path)

Repro:
1. Click into block A — toolbar appears; status says `saved`.
2. Click outside any block (on the canvas margin) without editing
   anything. Status flips to `unsaved`.
3. Click back into a block. Status returns to `saved` after the
   debounce.

The FocusLost commit (commit `0efe124`) is unconditional: every blur
fires a `commit_from_editor` that calls `on_action` with the same runs
the model already has. `apply_edit_block_body` runs, sees the body is
identical, and (apparently) still returns `Some(inverse)` or
`mark_dirty()` is called unconditionally in the action sink
(`action_sink.rs:98`). Either:

- Make `apply_edit_block_body` return `None` when `new_body == old_body`.
- Compare before/after in `action_sink::build_action_sink` and skip
  `mark_dirty()` when no recordable change occurred.

This is mostly cosmetic but it also fires the save debounce, which
triggers an unnecessary write of the same content (and a build run, as
visible in the status — `0+1 pages · N ms` re-fires).

**11. Duplicate workspace entry in the Welcome recents list**
`crates/lopress-editor/src/ui/welcome.rs` (recents source)

The welcome screen shows recents `skwared-blog`, `lopress-listtest`,
`skwared-blog` — the same workspace listed twice. Likely a path
canonicalization issue (`C:\Users\...` vs `c:\users\...`, with vs without
trailing slash) so two entries refer to the same workspace. The recents
list should normalize paths via `Path::canonicalize()` before dedup.

---

## Process / control-API coverage

**12. The control HTTP API has no `/open` endpoint**
`crates/lopress-editor/src/ctrl/mod.rs`

`/action` is a no-op when no document is open; the only way to open one
is to drive the welcome / sidebar UI via `/click`. For Claude to verify
fixes from a cold start, an explicit `POST /open {"path":"…"}` would let
the agent bypass the welcome screen and load a specific markdown file
into the editor. Currently the test fixture path has to be guessed via
sidebar coordinate hunting.

A symmetric `POST /close` would also help — repro flows often need to
reset the doc state to a known starting point.

---

## Status of prior open items

From `2026-05-15-editor-assessment.md`:

- **#8 Title / H1 divergence** — partially addressed. The inspector now
  shows a "Title differs from H1" warning and a "Sync from H1" button
  (visible in shots). But: (a) sync is not undoable (item #3 above);
  (b) the warning only fires for the H1 → Title direction; if the user
  edits the H1 and the title is stale, the warning appears, but if the
  user edits the title to something that doesn't match the H1, no
  warning surfaces. Symmetric validation would be more useful.
- **#9 No description / excerpt field** — resolved. Description field
  is visible at the bottom of the inspector.
- **#10 H4–H6 not exposed** — resolved but with the ordering bug noted
  in #7 above.

---

## Priority order (suggested)

1. **#3 Front-matter undo** — silent data-loss-ish; users who hit
   "Sync from H1" by mistake can't recover.
2. **#2 Toolbar buttons unresponsive to `/click`** — blocks Claude's
   ability to drive UI flows end-to-end via the control server.
3. **#1 Slash menu regression** — feature shipped but practically
   unreachable.
4. **#5 Lang-input undo** — silent: edits work but can't be undone.
5. **#4 Lang-input Enter-to-commit** — small fix, big consistency win.
6. **#12 `/open` ctrl endpoint** — unblocks reproducible cold-start
   tests for any future bug.
7. **#10 False dirty marks** — bandwidth waste (re-saves + re-builds).
8. **#6 Layout jump on focus** — moderate UX wart, needs a design call
   first.
9. **#11 Duplicate recents** — purely cosmetic.
10. **#8 Empty list items** + **#7 Toolbar ordering** + **#9 Toolbar
    visual separation** — UX polish.

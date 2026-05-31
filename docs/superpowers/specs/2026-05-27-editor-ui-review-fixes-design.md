# Editor UI Review — Fixes Design Spec

**Date:** 2026-05-27
**Author:** Kyle (designed with Claude)
**Status:** Approved — ready for implementation planning
**Source:** `docs/superpowers/ideas/2026-05-26-editor-ui-review.md`

---

## Scope

Fix every UI finding in the 2026-05-26 review of the lopress editor — the
twelve items in the ideas doc, all in the `lopress-editor` crate. The
review was driven against `feat/code-editor-block` at commit `849c418`;
this spec assumes that branch as the base.

The spec is intentionally a single plan's worth of work even though it
covers twelve items, because every item is small and confined to the
editor crate. Pi should treat each item as its own stage (one commit per
stage) so partial progress is clean to inspect and roll back.

**Out of scope:** memory-optimization findings
(`docs/superpowers/ideas/2026-05-26-editor-memory-review.md`) — separate
spec, separate plan. None of the UI fixes here should regress the hot
paths called out in that memory review (`commit_from_editor`,
`apply_edit_block_body`).

**User-confirmed during scoping:** toolbar block-type buttons truly do
nothing under real mouse input — not a `/click` artifact. Treat #2 as a
real bug that real users hit. (See Section 2 below.)

---

## Problem Statement

Twelve UI defects, summarized below. Full reproduction steps and code
references for each are in the ideas doc; this spec gives the *fix* for
each.

| # | Severity | Symptom |
|---|---|---|
| 1 | functional | Typing `/` on an empty paragraph inserts the character instead of opening the slash menu |
| 2 | functional | Toolbar block-type / inline-style / delete buttons don't fire their actions on real mouse clicks |
| 3 | functional | Front-matter changes (Title, Slug, Date, Tags, Description, Sync from H1) are not in the undo stack |
| 4 | functional | Code-block `lang` input commits on blur but not on Enter |
| 5 | functional | Lang edits aren't undoable |
| 6 | UX | Focus change between blocks shifts the document layout ~32–45 px |
| 7 | UX | Toolbar ordering — H4/H5/H6 placed after Code/UL/OL instead of after H1/H2/H3 |
| 8 | UX | Empty list items render as bare bullets with no affordance to click into |
| 9 | UX | Toolbar visually fuses with the focused block (no separator) |
| 10 | UX | Status bar reports `unsaved` after focus changes that didn't edit anything |
| 11 | cosmetic | Welcome recents list shows the same workspace twice |
| 12 | tooling | No `/open` endpoint on the debug ctrl server |

---

## Section 1 — Slash menu regression (Finding #1)

### Diagnosis

The slash interception lives at
`crates/lopress-editor/src/ui/blocks/inline_editor.rs:480-490`:

```rust
if !shift {
    if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
        if s.as_str() == "/" && slash_eligible {
            let is_empty = editor_sig.with_untracked(|ed| ed.doc().text().is_empty());
            if is_empty {
                on_action(BlockAction::OpenSlashMenu { block_id });
                return CommandExecuted::Yes;
            }
        }
    }
}
```

A live test confirms this branch does **not** fire even with a freshly
focused empty paragraph: typing `/` inserts the literal character.

The two most likely causes:

- `kp.key` is matched against `Key::Character`, but the `/` arrives via
  the KeyDown event at the lower level (after the 2026-05-18 rewire to
  `editor_view` — see `2026-05-18-editor-visual-ux-fixes-design.md`
  Section 1). Look at where character keys are processed in the new
  KeyDown path that was added to `editable_inline`: when
  `mods.is_empty()` and the key is `Key::Character`, the handler calls
  `editor.receive_char(&c)` directly. If `combined_key` returned
  `CommandExecuted::No`, but the post-handler still inserts the char,
  the `/` slips through.
- Or the `is_empty` check itself reads stale state (the editor's doc has
  already received the `/` via composition before `handle_key` runs).
  Less likely given the path order, but worth a `dbg!` to rule out.

### Fix

`combined_key` already returns `CommandExecuted::Yes` for slash; the
caller must respect that and short-circuit before
`editor.receive_char(&c)` runs.

In `editable_inline`'s KeyDown handler (introduced in the 2026-05-18
plan, Task 2b), capture the return value of `combined_key(&keypress,
key_event.modifiers)` and **only fall through to the receive_char path
when it returned `CommandExecuted::No`**.

```rust
.on_event_stop(EventListener::KeyDown, move |event| {
    let Event::KeyDown(key_event) = event else { return; };
    let key_text = key_event.key.text.clone();
    let Ok(keypress) = KeyPress::try_from(key_event) else { return; };
    if combined_key(&keypress, key_event.modifiers) == CommandExecuted::Yes {
        return;                  // <- the fix
    }
    // …existing receive_char fallback…
})
```

### Acceptance

- Click into a paragraph. Press End, then Enter (creates a new empty
  paragraph and focus moves to it). Press `/`. The slash menu opens; `/`
  is not inserted into the block.
- Same flow but the block is non-empty when `/` is pressed: `/` is
  inserted normally, no menu.
- Slash menu can be cancelled with Escape; the focus returns to the
  original block.

---

## Section 2 — Toolbar buttons unresponsive (Finding #2)

### Diagnosis

The block toolbar is rendered above the focused block via the
per-block `block_view` wrapper. Clicking a toolbar button (P / H1 / H2
/ H3 / Code / UL / OL / H4 / H5 / H6 / B / I / `</>` / Link / Delete)
visibly defocuses the underlying editor (toolbar disappears) but does
not run the button's `.action()`.

The inspector's **Sync from H1** button — rendered in a different
column entirely — works under the same input pipeline (confirmed
during the live review). So the bug is specific to the toolbar's
rendering context, not a global click problem.

Two hypotheses, both pointing at the same root cause: the toolbar lives
*inside* the block's container that also hosts the editor view. The
editor view captures pointer events for its entire bounding box (this is
how clicking-into-text positions the caret), and may be claiming the
event before the toolbar button can handle it. The toolbar shipped in
2026-05-15 (`editor-assessment` resolutions); the editor-view rewire in
2026-05-18 changed the pointer-event surface beneath it.

### Fix

Lift the toolbar out of the block's editor surface so the toolbar's
button hit-test wins. Two options, in decreasing order of preference:

**Option A — Floating overlay anchored above the focused block.** This
also fixes Finding #6 (layout jump) as a bonus and matches how WYSIWYG
editors normally do it. The toolbar becomes a single instance at the
editor-pane level, positioned via the focused block's bounding box.

**Option B — Keep the toolbar in the per-block tree but render it
*outside* the editor surface.** The simplest expression: change
`block_view` so that the toolbar `v_stack` wraps the editor `row` from
*outside* the focus-border container (which currently has `width_full`
and is the click target). Put the toolbar in a sibling position to the
editor surface, not on top of it. Less of a UI change than Option A,
also moves Finding #9 forward (visual separation) since the toolbar
gets its own bounding box.

**Pick Option B for this spec.** Option A is a larger UX redesign and
adds anchoring code; Option B is a hit-testing fix in `block_view`
that resolves the bug with minimal surface area. Finding #6 stays a
separate stage (Section 6) and may be deferred or further scoped down.

Concretely, in `crates/lopress-editor/src/ui/blocks/mod.rs` (the
`block_view` function), the structure today is:

```rust
v_stack((toolbar_slot, row))           // toolbar above editor row
    .style(|s| { /* focus border */ })  // border wraps both — this captures clicks
```

Move the focus border to wrap only `row`, leaving `toolbar_slot`
outside the click-capturing surface:

```rust
let row_with_border = row.style(/* focus border closure */);
v_stack((toolbar_slot, row_with_border))
    .style(|s| s.width_full())
```

### Investigation step (mandatory before the fix)

The hypothesis above is informed but not proven. The first task of this
stage is a 10-minute diagnosis to confirm:

1. Print debug into one of the toolbar button `.action()` closures and
   rebuild. Try clicking any toolbar button with a real mouse. If the
   debug print fires, the action is firing and the bug is elsewhere
   (state read in the action, focus race, etc.) — switch to that.
2. If it doesn't fire, confirm that the editor surface is intercepting
   the click by similarly printing in the editor view's `PointerDown`
   handler.

The fix above assumes case (2). If case (1) is the truth, revise the
fix to address the actual root cause; do not implement the structural
move blindly.

### Acceptance

- With a paragraph focused, clicking H1 in the toolbar changes the
  block to a heading. `/state` confirms `kind: "Heading1"`.
- Clicking Bold in the toolbar with a non-empty selection toggles the
  Bold flag on the selection (`/state` shows `bold: true` on the run).
- Clicking the Delete (`x`) button removes the focused block.
- All toolbar buttons fire under real mouse clicks (confirmed by the
  user; ctrl-API `/click` is no longer the only way to test).

---

## Section 3 — Front-matter undo (Finding #3)

### Diagnosis

`UndoStack` (`crates/lopress-editor/src/undo.rs`) only records
`BlockAction` inverses produced by `actions::apply`. Front-matter edits
(Title, Slug, Date, Tags, Description, the "Sync from H1" button) flow
through the inspector view's direct field mutation
(`crates/lopress-editor/src/ui/inspector.rs`) and never touch the action
sink.

### Fix

Front-matter is a single struct (`FrontMatter` in `lopress-core`).
Instead of inventing a new action type per field, add one variant that
swaps the whole struct:

```rust
// In crates/lopress-editor/src/actions.rs (BlockAction enum)
EditFrontMatter {
    new_front_matter: lopress_core::FrontMatter,
}
```

And one apply arm:

```rust
fn apply_edit_front_matter(
    doc: &mut EditorDoc,
    new: lopress_core::FrontMatter,
) -> Option<(BlockAction, BlockAction)> {
    if doc.front_matter == new {
        return None;
    }
    let old = std::mem::replace(&mut doc.front_matter, new.clone());
    Some((
        BlockAction::EditFrontMatter { new_front_matter: new },
        BlockAction::EditFrontMatter { new_front_matter: old },
    ))
}
```

Then update every front-matter edit site in `inspector.rs` to dispatch
this action through the existing `on_action` sink instead of mutating
`current_doc` directly. The inspector already receives `on_action` (it
ships actions for slug changes, etc., but not for front-matter — check
which path is used today and migrate any that aren't already going
through the sink).

The "Sync from H1" button computes the new title from the H1's text and
issues one `EditFrontMatter` action.

### Acceptance

- Edit the Title field, blur it. Press Ctrl+Z (focus inside a block).
  The Title reverts to its previous value. `/state` is unchanged
  through this flow (only front matter changed, which `/state` doesn't
  surface).
- Click "Sync from H1". Press Ctrl+Z. The Title reverts to its
  pre-sync value.
- Front-matter undo and block-action undo interleave correctly: type
  in a block (Edit A), change Title (Edit B). Ctrl+Z reverts Title
  first, then text.

---

## Section 4 — Code-block lang commits on Enter (Finding #4)

### Diagnosis

The `text_input` for code-block `lang` (introduced in
`crates/lopress-editor/src/ui/blocks/code_editor.rs` per commit
`fe70b6c`) commits only on blur. The toolbar URL input (the only other
discrete-value text input in the editor, `toolbar.rs:204-212`) commits
on Enter. The lang input should match.

### Fix

In `code_editor.rs`, locate the `text_input` builder for the lang field
and add an Enter handler that fires the same commit closure the blur
handler uses:

```rust
text_input(lang_buf)
    .on_event_stop(EventListener::KeyDown, move |e: &Event| {
        if let Event::KeyDown(k) = e {
            if matches!(k.key.logical_key, Key::Named(NamedKey::Enter)) {
                commit_lang_for_enter();   // same closure as on_blur
            }
        }
    })
    // …existing on_blur, style, etc.…
```

Add an Escape handler that *reverts* the buffer to the last committed
value (read from the block's `BlockKind::Code { lang }`):

```rust
} else if matches!(k.key.logical_key, Key::Named(NamedKey::Escape)) {
    let original = current_doc.with_untracked(|maybe| {
        maybe.as_ref()
            .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
            .and_then(|b| match &b.kind {
                BlockKind::Code { lang } => Some(lang.clone()),
                _ => None,
            })
            .unwrap_or_default()
    });
    lang_buf.set(original);
}
```

### Acceptance

- Click lang input, change `rust` to `python`, press Enter. `/state`
  shows `lang: "python"`. Focus stays in the input.
- Press Escape mid-edit: input reverts to the previous committed value.
- Blur still commits (regression check).

---

## Section 5 — Lang edits in undo stack (Finding #5)

### Diagnosis

The lang commit path appears not to dispatch through `on_action` — `Ctrl+Z`
does not revert lang changes. Grep `code_editor.rs` for
`BlockKind::Code { lang` assignments to find the direct-mutation site.

### Fix

Route lang changes through `BlockAction::EditAttrs` (the existing path
for plugin attrs) so they pick up the existing
`apply_edit_attrs`-driven undo recording. `apply_edit_attrs`
(`actions.rs:123-166`) already mirrors `attrs["lang"]` into
`BlockKind::Code.lang`, so the routing is "free" — the apply function
does the right thing as soon as the dispatch is correct.

In the lang `text_input`'s commit closure (the one called from blur and
from Section 4's Enter handler), build the new attrs map and dispatch:

```rust
let new_lang = lang_buf.get_untracked();
let new_attrs = current_doc.with_untracked(|maybe| {
    maybe.as_ref()
        .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
        .and_then(|b| b.plugin.as_ref())
        .map(|m| {
            let mut a = m.attrs.clone();
            a.insert("lang".to_string(), serde_json::Value::String(new_lang));
            a
        })
        .unwrap_or_default()
});
on_action(BlockAction::EditAttrs { block_id, new_attrs });
```

Remove the existing direct-mutation site that bypasses `on_action`.

### Acceptance

- Change lang `rust` → `python` and commit (Enter or blur). Press
  Ctrl+Z (focused inside a block editor). `/state` shows `lang: "rust"`.
- Redo (Ctrl+Y) re-applies `python`.

---

## Section 6 — Layout jump on focus (Finding #6)

### Diagnosis

The toolbar adds ~32 px when it appears above the focused block. Every
focus change shifts the rest of the document below by that amount.

### Fix

Reserve a fixed-height slot above every block for the toolbar — always
present, only populated for the focused block. The slot height matches
the toolbar's own height. This is the minimum-change fix.

In `block_view`, the toolbar today is rendered via a `dyn_container`
that returns either the toolbar or `empty()`. Wrap that container in a
fixed-height styling that matches the toolbar's natural height:

```rust
let toolbar_slot = dyn_container(/* … focused? toolbar : empty …  */)
    .style(|s| s.width_full().height(TOOLBAR_HEIGHT_PX));
```

`TOOLBAR_HEIGHT_PX` is a `const f32` derived from the toolbar's actual
rendered height (measure once with the debug skill: focus a block,
screenshot, measure the toolbar's pixel extent including its bottom
margin). Likely 32–36 px — pick the smallest value that doesn't clip
the toolbar's border.

### Acceptance

- Click between three different blocks in succession. Take screenshots.
  The Y position of the third block does not change between focuses.
- The toolbar appears in the same position relative to the focused
  block as before; the document below it does not shift.

### Tradeoff acknowledged

Reserving the slot costs 32 px of vertical whitespace above every
unfocused block. The alternative (true floating overlay anchored to the
focused block) is bigger work — left as a follow-up if the whitespace
becomes objectionable.

---

## Section 7 — Toolbar button ordering (Finding #7)

### Diagnosis

`crates/lopress-editor/src/ui/toolbar.rs:55-71` defines the kinds vector:

```rust
let kinds: Vec<(&'static str, BlockKind)> = vec![
    ("P", BlockKind::Paragraph),
    ("H1", BlockKind::Heading(1)),
    ("H2", BlockKind::Heading(2)),
    ("H3", BlockKind::Heading(3)),
    ("Code", BlockKind::Code { lang: String::new() }),
    ("UL", BlockKind::List { ordered: false }),
    ("OL", BlockKind::List { ordered: true }),
    ("H4", BlockKind::Heading(4)),
    ("H5", BlockKind::Heading(5)),
    ("H6", BlockKind::Heading(6)),
];
```

### Fix

Reorder the vector so headings are contiguous:

```rust
let kinds: Vec<(&'static str, BlockKind)> = vec![
    ("P", BlockKind::Paragraph),
    ("H1", BlockKind::Heading(1)),
    ("H2", BlockKind::Heading(2)),
    ("H3", BlockKind::Heading(3)),
    ("H4", BlockKind::Heading(4)),
    ("H5", BlockKind::Heading(5)),
    ("H6", BlockKind::Heading(6)),
    ("Code", BlockKind::Code { lang: String::new() }),
    ("UL", BlockKind::List { ordered: false }),
    ("OL", BlockKind::List { ordered: true }),
];
```

### Acceptance

- Focus any paragraph. The toolbar row reads
  `P · H1 · H2 · H3 · H4 · H5 · H6 · Code · UL · OL · | · B · I · </> · Link · | · x`.

---

## Section 8 — Empty list items affordance (Finding #8)

### Diagnosis

List items with empty `runs` render as a bare bullet glyph. No
placeholder, no caret hint, narrow click target. Users don't realize
they can click into them.

### Fix

Render empty list items with a faint placeholder so they show as a
clickable target. The placeholder text is purely visual — never written
to the model.

In `crates/lopress-editor/src/ui/blocks/list.rs`, where each list item
is mounted, check the item's runs for emptiness and overlay a
placeholder label when empty. The placeholder participates in
hit-testing (clicking it routes focus to the item editor) but is hidden
the moment the editor's buffer is non-empty.

Implementation sketch:

```rust
let runs_empty = item.runs.iter().all(|r| r.text.is_empty());
let placeholder = dyn_container(
    move || /* read editor's text — empty? */,
    move |is_empty| if is_empty {
        label(|| "Type or press Backspace to remove".to_string())
            .style(|s| s.color(Color::rgb8(160, 160, 160)))
            .into_any()
    } else {
        empty().into_any()
    },
);
// Overlay placeholder on top of the editor surface for the item.
```

If overlaying clutters the layout, a simpler alternative: extend the
item's hit area to the full row width with a CSS-equivalent style and
add the placeholder inline (replacing the empty editor visually when
runs are empty).

### Acceptance

- Open the list-test document. The empty list items each show greyed
  "Type or press Backspace to remove" placeholder text.
- Clicking on the placeholder focuses that item; typing fills it,
  placeholder disappears.
- A non-empty item shows its text without placeholder.

---

## Section 9 — Toolbar visual separation (Finding #9)

### Diagnosis

The toolbar sits flush against the focused block content with only a 4
px margin and no shadow / panel separation. The focus border wraps
both toolbar and block, suggesting the toolbar is part of the block.

### Fix

Give the toolbar its own background panel with a subtle shadow and
explicit separation from the block:

```rust
// In toolbar.rs, the button_row style:
.style(|s| {
    s.padding_horiz(8.)
        .padding_vert(4.)
        .gap(4.)
        .background(Color::rgb8(252, 252, 254))   // slightly lighter than block
        .border(1.)
        .border_color(Color::rgb8(220, 220, 226))
        .border_radius(6.)
        .margin_bottom(6.)                          // breathing room from block
        // Subtle drop shadow if floem supports it; if not, skip.
})
```

Combined with the Section 2 fix (toolbar moved outside the focus
border), the toolbar reads as a distinct floating affordance rather
than part of the block.

### Acceptance

- Focus a block; screenshot. The toolbar is visually distinct from the
  block beneath it: own background, own border, clear gap.
- The focus border on the block does not wrap the toolbar.

---

## Section 10 — False dirty marks (Finding #10)

### Diagnosis

The FocusLost commit (`crates/lopress-editor/src/ui/blocks/inline_editor.rs`,
commit `0efe124`) unconditionally fires `commit_from_editor` on every
blur, regardless of whether the editor buffer differs from the model.
The action sink's `mark_dirty` then fires unconditionally
(`action_sink.rs:98`), even when the underlying `apply_edit_block_body`
returned `None` (no recorded change).

### Fix

`action_sink::build_action_sink` already has `recorded:
Option<(BlockAction, BlockAction)>` from the apply call. Gate
`mark_dirty()` on `recorded.is_some()`:

```rust
// crates/lopress-editor/src/ui/editing/action_sink.rs:98
if recorded.is_some() {
    on_action_mark_dirty();
}
```

Plus the focus / slash-menu housekeeping already runs unconditionally
above this line; that stays.

### Acceptance

- Click into a block, then click outside any block (canvas margin).
  Status bar stays `saved`.
- Click into a block and actually edit text. Status bar flips to
  `unsaved` after the edit and back to `saved` after the debounced
  save.

---

## Section 11 — Welcome recents dedup (Finding #11)

### Diagnosis

`welcome.rs` reads the recents list from `Settings.recents`. A
workspace path can appear twice when its on-disk form differs in
casing or trailing-slash (`C:\Users\...` vs `c:\users\...`,
`C:\Users\X\foo` vs `C:\Users\X\foo\`).

### Fix

Canonicalize each recent path before display and dedup:

```rust
// In welcome.rs (or the helper that produces the recents Vec):
let canonical_recents: Vec<PathBuf> = settings_signal
    .with_untracked(|s| s.recents.clone())
    .into_iter()
    .filter_map(|p| p.canonicalize().ok().or(Some(p)))  // fall back to raw
    .fold(Vec::new(), |mut acc, p| {
        if !acc.contains(&p) { acc.push(p); }
        acc
    });
```

Also canonicalize on insertion in the existing recents-push site (when
a workspace is opened) so the persisted recents stop accumulating
duplicates over time. `Path::canonicalize` may fail (path deleted /
unmounted); fall back to the raw path in that case so legitimate
recents don't disappear.

### Acceptance

- Welcome screen shows each workspace at most once.
- A path that no longer exists on disk still appears (so the user can
  see and clear it) but is not duplicated.

---

## Section 12 — `/open` ctrl endpoint (Finding #12)

### Diagnosis

The debug ctrl server has `/ping`, `/state`, `/action`, `/input`,
`/click`, `/screenshot`. Opening a document requires driving the
welcome / sidebar UI by hand via `/click`. Tests and Claude both need a
direct way to load a file.

### Fix

Add `POST /open { "path": "..." }` to
`crates/lopress-editor/src/ctrl/mod.rs`. The body shape:

```json
{ "path": "C:\\path\\to\\post.md" }
```

The path may be:
- An absolute path to a `.md` file inside a known workspace — the
  server resolves which workspace it belongs to, opens that workspace
  if not already open, and opens the file.
- A path relative to the currently-open workspace — `posts/foo.md` /
  `pages/foo.md`.

Behavior:
- 200 `{"status":"opened","path":"..."}` on success.
- 404 `{"status":"not_found"}` when the path doesn't resolve to a doc.
- 409 `{"status":"no_workspace"}` when no workspace is open and the
  path is relative.

The handler dispatches through the same `on_open` chain that the
welcome view uses (see `ui/mod.rs:165-176` — the on_open closure for
`DocumentRef`). The simplest implementation: introduce a
`CtrlOpenRequest` channel parallel to the existing
`CtrlActionEnvelope` channel; the ctrl effect reads it, builds a
`DocumentRef`, and calls into `editing.open_document` /
`current_doc.set`.

Symmetric `POST /close` (no body, sets `current_doc` to `None`,
returns 200) is in scope and unblocks repro flows that need a clean
starting state.

Both endpoints are `#[cfg(debug_assertions)]` like the rest of the
ctrl server.

### Acceptance

- Start the editor (no document open). `POST /open` with a valid
  absolute path. `/state` now shows the doc.
- `POST /close`. `/state` reports `doc_open: false`.
- `POST /open` with a relative path before any workspace open: 409.
- `POST /open` with a nonexistent path: 404.
- Update `.claude/skills/driving-lopress-editor/SKILL.md` to document
  the new endpoints. (See "Cross-cutting concerns" below.)

---

## Cross-cutting concerns

### The `driving-lopress-editor` skill doc must be updated

After Section 12 lands, add `/open` and `/close` to the endpoints table
in `.claude/skills/driving-lopress-editor/SKILL.md`. Other tests in
this spec may reference behaviors that change the existing endpoint
docs (none currently identified — the existing endpoints stay
unchanged).

### Verification ordering

Stages are sequenced so each can be verified independently:

1. **Section 7** (toolbar ordering) — trivial vector reorder, no
   dependencies.
2. **Section 11** (recents dedup) — isolated to welcome.rs / Settings.
3. **Section 10** (false dirty marks) — one-line change in
   action_sink, no other dependencies.
4. **Section 12** (`/open`+`/close`) — new ctrl endpoints; unblocks
   automated repro for everything below.
5. **Section 1** (slash menu) — fixes a regression introduced by the
   2026-05-18 rewire; isolated to the inline_editor KeyDown handler.
6. **Section 4** (lang Enter) — isolated to code_editor.rs.
7. **Section 5** (lang undo) — builds on Section 4; routes through the
   existing `EditAttrs` apply path.
8. **Section 3** (front-matter undo) — adds a new `BlockAction` variant
   plus an apply arm, plus inspector wiring.
9. **Section 2** (toolbar clicks) — investigation step first; expected
   fix is a `block_view` structural tweak.
10. **Section 9** (toolbar visual separation) — builds on Section 2
    landing (the toolbar moves outside the block's focus border).
11. **Section 6** (layout jump) — reserves toolbar height on every
    block; touches `block_view`, builds on Section 2's structural move.
12. **Section 8** (empty list items) — UX polish on `list.rs`; no
    dependency on earlier sections.

### Performance contract

None of these fixes should regress the hot paths called out in the
memory-optimization review (`commit_from_editor`,
`apply_edit_block_body`, `canonicalize_body`). In particular: the
front-matter undo (Section 3) introduces a new action variant whose
payload is `FrontMatter`. Front-matter is small (KB at most) so a
direct clone is fine; do not box it just for variant-size parity. The
memory review's Section 13 will reshape the enum holistically if
needed.

### Test coverage

Each section's acceptance criteria are testable via the
`driving-lopress-editor` skill (after Section 12 lands, `/open` makes
this much easier). Existing unit tests in `actions.rs`, `undo.rs`, and
the per-module test files cover the model-level changes; add new tests
for:

- Section 3: `apply_edit_front_matter` produces correct inverse;
  no-op when front matter unchanged.
- Section 1: a unit-level test of the KeyDown handler's
  short-circuit, if testable without a real floem editor.
- Section 7: snapshot test of `block_toolbar_for`'s kind ordering.

The toolbar-buttons-actually-fire test for Section 2 must be a manual
verification step using the driving skill — pointer event hit-testing
isn't unit-testable here.

---

## Non-goals

- The toolbar floating-overlay redesign (full Option A for Section 2)
  is a future stage. This spec uses the smaller structural fix.
- Front-matter coalescing in the undo stack (typing in the Title field
  shouldn't generate 20 undo entries) — out of scope; add if Section 3
  produces noticeable noise.
- The memory-optimization findings are a separate spec
  (`docs/superpowers/ideas/2026-05-26-editor-memory-review.md`).
- A full investigation into why the toolbar click bug landed in the
  first place (Section 2 root cause) — fix it, add the manual
  acceptance, and move on. A regression test for it is out of scope.

---

## Resolved decisions

### Decision: Treat all 12 findings as one plan, one stage per finding

Each finding is small enough to land as a single stage / commit. They
don't share enough infrastructure to need multi-stage choreography.
Pi planner should produce one task per section; each task is one
commit.

### Decision: Section 2 uses the smaller structural fix (Option B)

The user wants the toolbar buttons working; they don't want a UI
redesign in the same pass. Move the toolbar outside the focus-border
container in `block_view`. Floating-overlay anchored toolbar (Option
A) is a future change tracked in non-goals.

### Decision: Section 6 reserves toolbar height rather than overlay-anchoring

Smallest change that fixes the visible jump. The whitespace cost is
acceptable; we can revisit if the design calls for overlay later.

### Decision: Section 3 uses a single `EditFrontMatter` action

One variant carries the whole new struct instead of per-field
variants. Less code, easier reasoning, undo at the right granularity
(one undo per Title-blur, not per character).

---

## Open questions

None. The Section 2 investigation step builds the diagnosis evidence
into the first task of that stage, so pi can branch based on what it
finds.

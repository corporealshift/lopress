# Editor Assessment — 2026-05-15

Findings from a live inspection session using the debug HTTP control API (`127.0.0.1:7878`).
The document used was `common-ai-skill-problems.md` (41 blocks).

---

## Critical

**1. Scroll doesn't follow the cursor** (`ui/editor_pane.rs:60`)

`scroll(column)` is a static container. When keyboard navigation moves the cursor to a block below the viewport, the view doesn't scroll. For a 41-block document roughly only the top 10 blocks are accessible by keyboard. There is no `ensure_visible` / scroll-to-focused-block hook anywhere.

**2. Ctrl API edits don't trigger save** (`ctrl/mod.rs:398-408`)

The `/action` HTTP handler calls `crate::actions::apply(doc, ...)` directly, bypassing the `on_action` chokepoint in `ui/mod.rs`. `mark_dirty()` is never called, so the 500 ms debounced save never fires. Documents edited via the API lose changes on close. `focus_target` is also not updated after API mutations, so focus management is broken for API-driven structural changes.

---

## Functional gaps

**3. Slash menu is unreachable from the keyboard** (`ui/blocks/inline_editor.rs`)

The `_slash_eligible: bool` parameter has an underscore prefix confirming the `/` key path is stubbed out. The slash menu widget exists but there is no keyboard trigger wired to it. It cannot be opened.

**4. Links have no URL** (`ui/toolbar.rs:104-111`)

`Ctrl+K` and the "Link" toolbar button toggle `InlineFlag::Link` on the selected text, but there is no URL input dialog or field anywhere. Links are styled text with no `href`. Not usable for publishing.

**5. Enter in a List block is a no-op** (`actions.rs`)

`apply_split` matches on `BlockBody::List` and falls through to `_ => {}`. Pressing Enter inside a list does nothing — no new item is created, no split occurs. List content is effectively append-only from the action API.

**6. No undo / redo**

No undo stack exists anywhere in the codebase. Every edit is permanent until the file is overwritten.

---

## Navigation

**7. No document-level keyboard shortcuts**

`Ctrl+End`, `Page Down`, `Ctrl+Home` are not handled. Combined with issue #1 this makes long documents nearly unusable by keyboard alone. The only navigation is line-by-line `Up`/`Down` within and between blocks.

---

## Inspector (front-matter panel)

**8. Title field and H1 block can diverge**

The inspector's "Title" text field and the document's first `Heading(1)` block are completely independent. Nothing syncs or validates them. It is easy to publish with a front-matter title that does not match the rendered heading.

**9. No description / excerpt field**

A summary or excerpt field is standard blog front-matter and is absent from the inspector.

---

## Toolbar

**10. H4–H6 not exposed**

The type-selector row shows P / H1 / H2 / H3 / Code / UL / OL. Heading levels 4–6 are supported in the model but not reachable from the toolbar.

---

## Priority order (suggested)

1. Scroll-to-cursor (#1) — unblocks keyboard editing of any real document
2. Ctrl API save fix (#2) — unblocks automated testing via the API
3. Enter in List (#5) — list authoring is broken without it
4. Undo (#6) — safety net before deeper editing features
5. Slash menu keyboard trigger (#3)
6. Link URL dialog (#4)
7. Title sync / inspector gaps (#8, #9)
8. Navigation shortcuts (#7)
9. H4–H6 toolbar (#10)

# Editor Assessment — 2026-05-15

Findings from a live inspection session using the debug HTTP control API (`127.0.0.1:7878`).
The document used was `common-ai-skill-problems.md` (41 blocks).

> **Status sweep 2026-05-23:** #1–#7 are all resolved (see per-finding notes
> below). Remaining open: #8–#10 (inspector/toolbar gaps that were never in
> the list-editor-unification scope).

---

## Critical

**1. Scroll doesn't follow the cursor** (`ui/editor_pane.rs:60`)

`scroll(column)` is a static container. When keyboard navigation moves the cursor to a block below the viewport, the view doesn't scroll. For a 41-block document roughly only the top 10 blocks are accessible by keyboard. There is no `ensure_visible` / scroll-to-focused-block hook anywhere.

> **Resolved.** The `focus_target` effect in `mount_block_editor`
> (`ui/blocks/inline_editor.rs:350`) calls `view_id.scroll_to(None)` on the
> editor view whenever a block receives programmatic focus, which bubbles up
> to the outer scroll container. Same mechanism in
> `ui/blocks/list.rs:261` for list items.

**2. Ctrl API edits don't trigger save** (`ctrl/mod.rs:398-408`)

The `/action` HTTP handler calls `crate::actions::apply(doc, ...)` directly, bypassing the `on_action` chokepoint in `ui/mod.rs`. `mark_dirty()` is never called, so the 500 ms debounced save never fires. Documents edited via the API lose changes on close. `focus_target` is also not updated after API mutations, so focus management is broken for API-driven structural changes.

> **Resolved** in the list-editor-unification refactor. The ctrl consumer
> effect at `ui/mod.rs:516` translates each `CtrlAction` to a `BlockAction`
> and routes it through `on_action_for_ctrl` (a clone of the UI's
> `on_action`), so `mark_dirty`, `focus_target`, and the undo stack all fire
> uniformly whether the action came from the UI or `/action`.

---

## Functional gaps

**3. Slash menu is unreachable from the keyboard** (`ui/blocks/inline_editor.rs`)

The `_slash_eligible: bool` parameter has an underscore prefix confirming the `/` key path is stubbed out. The slash menu widget exists but there is no keyboard trigger wired to it. It cannot be opened.

> **Resolved.** Typing `/` on an empty paragraph emits
> `BlockAction::OpenSlashMenu { block_id }` (`ui/blocks/inline_editor.rs:472`),
> which the action handler in `ui/mod.rs:252` routes to the slash-menu
> widget. List items pass `slash_eligible = false` and so don't open it.

**4. Links have no URL** (`ui/toolbar.rs:104-111`)

`Ctrl+K` and the "Link" toolbar button toggle `InlineFlag::Link` on the selected text, but there is no URL input dialog or field anywhere. Links are styled text with no `href`. Not usable for publishing.

> **Resolved.** The toolbar opens a URL row (`text_input` +
> Remove button) when a link is active on the selection
> (`ui/toolbar.rs:204`). Enter commits the URL by emitting `EditBlockBody`
> with the updated inline runs. Driven by the `link_url_sig` published from
> `mount_block_editor`.

**5. Enter in a List block is a no-op** (`actions.rs`)

`apply_split` matches on `BlockBody::List` and falls through to `_ => {}`. Pressing Enter inside a list does nothing — no new item is created, no split occurs. List content is effectively append-only from the action API.

> **Resolved** in stage 4 of the list-editor refactor. The list
> structural-key callback (`ui/blocks/list.rs:366`) handles Enter by
> building a new `BlockBody::List` with the focused item split at the
> caret and emitting `EditBlockBody`. `apply_split`'s list arm is no
> longer the path — list splits flow through the generic body swap.

**6. No undo / redo**

No undo stack exists anywhere in the codebase. Every edit is permanent until the file is overwritten.

> **Resolved** in stage 1 of the list-editor refactor.
> `crate::undo::UndoStack` is constructed in `ui/mod.rs:192`, populated via
> `push_after_apply(canonical, inverse)` at `ui/mod.rs:286` (the inverses
> come from the new return value of `actions::apply`), and consumed by
> `on_undo`/`on_redo` closures wired to Ctrl+Z/Y in `mount_block_editor`.

---

## Navigation

**7. No document-level keyboard shortcuts**

`Ctrl+End`, `Page Down`, `Ctrl+Home` are not handled. Combined with issue #1 this makes long documents nearly unusable by keyboard alone. The only navigation is line-by-line `Up`/`Down` within and between blocks.

> **Resolved.** `mount_block_editor` handles Ctrl+Home, Ctrl+End, PageUp,
> and PageDown (`ui/blocks/inline_editor.rs:449,457,561,576`), each
> committing the current block before jumping `focus_target`. List items
> inherit the behavior because they mount through the same path.

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

1. ~~Scroll-to-cursor (#1)~~ — done
2. ~~Ctrl API save fix (#2)~~ — done
3. ~~Enter in List (#5)~~ — done
4. ~~Undo (#6)~~ — done
5. ~~Slash menu keyboard trigger (#3)~~ — done
6. ~~Link URL dialog (#4)~~ — done
7. Title sync / inspector gaps (#8, #9) — open
8. ~~Navigation shortcuts (#7)~~ — done
9. H4–H6 toolbar (#10) — open

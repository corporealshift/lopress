# Plugin textarea: grow to fit, markdown shortcuts

**Status:** design approved
**Date:** 2026-10-20
**Issue:** #54

## Problem

Plugin attrs with `ui = "textarea"` (the callout body, for example) render as a
fixed ~60px floem `text_editor` with its own scroll. Three complaints:

1. You can't see more than two lines at once.
2. Typing on the last line snaps the box's inner scroll back to the top. Live
   repro: wrapped body, Ctrl+End, type; the second keystroke shows the first
   lines again while the caret stays at the end. The inner scroll offset resets
   during the post-edit relayout.
3. There's no way to add bold/italic other than typing the markdown by hand.

## Design

**Grow to fit.** The textarea is always exactly as tall as its wrapped content
(minimum two lines), like a paragraph block. With no overflow there is nothing
to scroll, so the inner scroll can't jump: (2) goes away with (1). The height
comes from a memo over `last_vline`, with the same guards as the paragraph
editor's memo (`mount_block_editor`): subscribe to `screen_lines` and not
`viewport`, read the width guard untracked, and reject stale collapsed counts
via `accept_visual_lines`. `scroll_beyond_last_line` is turned off, or the
editor would keep a scrollable overhang.

**Markdown shortcuts.** The body is stored and rendered as markdown, so
formatting stays markdown text. Ctrl+B / Ctrl+I / Ctrl+E toggle `**`, `_` and
`` ` `` around the selection, the same keys as the paragraph editor:

- With a selection, wrap it. If it's already wrapped (markers just outside it,
  or as its first/last characters), unwrap it. The selection stays on the text.
- With a collapsed caret, insert an empty pair and put the caret between them.

The text logic is a pure function in the model (`markdown_wrap`), unit-tested.
The field uses floem's `text_editor_keys` to catch the three chords and passes
every other key to floem's default handler.

## Rejected

- *Drag handle to resize:* keeps the inner scroll and its jump, which would then
  need a fix inside floem's scroll view; the size also wouldn't survive rebuilds.
- *Expand only while focused:* still needs a collapsed state that scrolls.
- *Rich-text body using the paragraph machinery:* far larger, and markdown the
  runs model can't express (lists, headings inside a callout) would be lost.
- *Toolbar B/I buttons for plugin fields:* possible later; shortcuts cover the
  ask without touching the toolbar's focus plumbing.

## Verification

- Unit tests for `markdown_wrap`: wrap, unwrap from outside, unwrap from
  inside, collapsed caret, and a multi-byte selection.
- Live, on a scratch site: a long wrapped callout body shows every line, typing
  at the end doesn't jump, Ctrl+B wraps the selection, and the saved `.md`
  holds the markdown.

# Editor Control-Verification Findings — 2026-05-16

Live verification of the Phase 3 Floem editor, driven through the debug
control API (`127.0.0.1:7878`: `/state`, `/screenshot`, `/action`, `/input`,
`/click`). A throwaway workspace was used: `test-workspace/` with
`src/posts/editor-test.md` (H1 + paragraphs + H2/H3 sections).

## Verified working

- Document open / parse / render — H1/H2/H3 + paragraphs render with the
  expected size gradient.
- **Task 2** — ctrl `/action` `EditInline` updates `/state` *and* persists to
  disk; the save-debounce fires through `on_action`.
- **Task 7** — the title/H1 divergence warning appears when front-matter title
  ≠ first H1; "Sync from H1" copies the H1 into the title and clears the
  warning; the Description field is present.
- **Task 9** — H4/H5/H6 toolbar buttons are present and functional
  (`ChangeType` to Heading 4 and Heading 6 confirmed via `/state`).
- Block split (Enter), `ChangeType` via toolbar, and autosave to disk.

## Bugs to fix

### Bug 1 — Heading with a soft line break does not round-trip (data integrity)

**Severity:** Medium.

`crates/lopress-core/src/serializer.rs` `write_block` for `"heading"` emits
`#`×level + ` ` + `b.text` + `\n`. When the heading's text contains an
embedded `\n` (a soft line break), the second line is written with **no
heading prefix**:

```
#### A second paragraph ... enough words
to wrap across more than one line in the editor pane.
```

On reload, line 2 parses as a **separate paragraph**. Reproduced: a two-line
paragraph was `ChangeType`d to Heading 4, saved, and reopened — the single H4
block came back as `Heading4` + `Paragraph`.

**Fix direction:** either the heading serializer must escape/reject embedded
newlines (e.g. write each continuation line with the same `#`×level prefix, or
collapse `\n`→space), or `ChangeType` to a heading should flatten soft breaks
in the body so headings never hold a `\n`.

### Bug 2 — Every save expands front matter with explicit `null`/`[]` fields

**Severity:** Low (cosmetic file churn, noisy VCS diffs).

`serializer.rs::serialize` calls `serde_yaml::to_string(&doc.front_matter)`,
which serialises **all** `FrontMatter` fields. A file authored with only
`title`/`date`/`draft` is rewritten on first save as:

```yaml
title: ...
slug: null
date: ...
tags: []
draft: true
description: null
image: null
```

`is_default_frontmatter` only gates whether the block is emitted at all — not
which fields. Note `image` is written even though it has no inspector field.

**Fix direction:** add `#[serde(skip_serializing_if = "Option::is_none")]` to
the optional `FrontMatter` fields and `skip_serializing_if = "Vec::is_empty"`
to `tags`/`extra` (in `lopress-core/src/types.rs`).

### Bug 3 — Block toolbar overlaps the preceding block

**Severity:** Low (cosmetic).

When a block is focused, the floating toolbar mounts above it and visually
overlaps the bottom of the block above — the H1 descenders were clipped, and a
focused block below an H3 left the toolbar covering "Section Three". The
toolbar's vertical space is not fully reserved in layout (or it has a negative
offset), so it draws over the previous block.

**Fix direction:** review the toolbar slot in `ui/blocks/mod.rs` — ensure the
toolbar row contributes its full height to layout so the preceding block is
not occluded.

## Could not verify through the control harness

> **Resolved 2026-05-16.** The harness input injection was rewritten to use
> `SendInput` (real Windows input pipeline) with foreground activation, in
> place of `PostMessage`. Live-verified: `/input` text now types into the
> editor and `ctrl+…` chords register (e.g. `ctrl+a` selects all). `parse_key`
> gained `pageup`/`pagedown` and layout-aware single-character keys. The
> limitations below stood at the time of the original run.

The harness's input injection has gaps that blocked live testing of several
Phase 3 keyboard features. These are **harness limitations**, not confirmed
editor bugs — the affected features should be exercised by hand, and the
harness improved so they can be regression-tested.

| Limitation | Blocks verification of |
|---|---|
| `/input {type:"text"}` posts bare `WM_CHAR`; winit needs a `WM_KEYDOWN`+`WM_CHAR` pair, so no characters are inserted | All free-text typing; coalesced-edit undo |
| `/input {type:"keys"}` with a Ctrl modifier had no observable effect (`ctrl+z`, `ctrl+home` tested) — the modifier does not appear to register | **Task 4** undo/redo (Ctrl+Z/Y), **Task 6** link URL row (Ctrl+K), **Task 8** Ctrl+Home/End |
| `ctrl/input.rs::parse_key` has no `pageup`/`pagedown` cases (and no `VK_PRIOR`/`VK_NEXT` imports) | **Task 8** Page Up / Page Down |
| `parse_key` maps any 1-char string to `VIRTUAL_KEY(uppercased char)`, wrong for non-letters like `/` | **Task 5** slash-menu `/` trigger |

### Suggested harness fixes (to make Phase 3 testable)

- `send_text`: pair each `WM_CHAR` with a preceding synthetic `WM_KEYDOWN`
  (and `WM_KEYUP`) so winit produces a `KeyEvent` carrying `text`.
- `send_keys`: verify the Ctrl/Shift/Alt modifier key-down reaches winit's
  modifier state before the main key — consider `SendInput` (as `/click`
  already uses) instead of `PostMessage` for modified combos.
- `parse_key`: add `pageup`/`pagedown` (`VK_PRIOR`/`VK_NEXT`) and a named
  `slash` case.

## Not exercised

- **Task 1** (scroll-to-cursor) — needs a 20+ block document; the test doc was
  short enough to fit on screen.
- **Tasks 4 / 6 / 8** keyboard paths — see harness limitations above; covered
  by the 8 passing `undo_tests` and prior code review, but not by a live run.

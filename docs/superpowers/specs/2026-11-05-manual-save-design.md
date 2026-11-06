# Manual save and refresh from disk

**Status:** design approved, implementation plan in `docs/superpowers/plans/2026-11-05-manual-save.md`
**Date:** 2026-11-05

## Problem

The editor autosaves 500 ms after every edit, and nothing tells it when a file
changes on disk. When Claude edits a post that is open in lopress, the next
autosave silently replaces those edits with the editor's older copy.

We want saving to be a deliberate act, and we want the editor to notice outside
changes and offer to load them.

## What it does

- Ctrl+S or a Save button saves the open post and its notes together.
- Nothing is written while typing. The preview updates on save.
- Switching to another document with unsaved changes asks first.
- A bar appears when the open post or notes change on disk, with a Refresh
  button.
- Saving over an outside change asks first.
- Closing the app saves without asking, unless the file also changed on disk.

This replaces the autosave described in
`docs/superpowers/specs/2026-11-04-post-notes-design.md`.

### Out of scope

- A warning on close. floem 0.2.0 destroys the window on `CloseRequested` with
  no hook. `WindowCloseRequested` exists on floem main, so this becomes possible
  after the planned migration.
- Refreshing automatically. Refresh is always a button.
- Merging, or showing a diff of, the two versions.
- Crash recovery. With autosave gone, a crash loses anything unsaved. This is a
  new risk, accepted on purpose.

## Unsaved state

A post is unsaved when `dirty` is set or its notes are dirty. Today `dirty` is
only set when an action changes the document, and typed text doesn't reach the
document until blur or a structural key. Under autosave that didn't matter.
Under manual save the footer would say "Saved" while someone types.

So typing has to mark the post unsaved directly. Every block editor is built by
`build_block_editor` and mounted by `mount_block_editor`, so one effect on
`text_sig` in `mount_block_editor` covers paragraphs, headings, lists, tables
and code. `text_sig` is only written for edits made through an editor.

Plugin attribute fields, image alt and caption fields, and the code language box
commit on blur instead. Each one marks the post unsaved as it changes and registers
its commit in `active_commit`, so Ctrl+S reaches its text. Without this, Ctrl+S in
a pull quote would save nothing and still report "Saved". Clicking in and out of
one of these fields changes nothing, so it doesn't mark the post unsaved.

Typing something and deleting it still counts as unsaved. Comparing buffers to
catch that isn't worth the code.

The window title stays `lopress`, because the control server finds the window by
that exact title. Unsaved state shows in the footer.

## Saving

One function, used by Ctrl+S, the Save button, the dialog and `/save`:

1. Run `active_commit`, so a focused block's buffer reaches the document.
2. If nothing is unsaved, stop. A clean Save writes nothing, unless the file is
   missing, which counts as unsaved.
3. If the post or notes changed on disk, ask (see below) and stop.
4. Save the notes if they're dirty.
5. Save the post if it's dirty, rename the file if the slug changed, and
   rebuild the site.

A notes-only save does not rebuild: notes don't affect the site. The slug rename
now happens only on save, so a file never moves while someone is typing.

If any write fails, the error shows, the post stays unsaved, and switching
documents stays blocked until a save succeeds or the user chooses Don't save.

### Ctrl+S routing

floem delivers a key to the focused view first, and only unprocessed keys reach
the main view. Block editors stop every key, so Ctrl+S needs three entry points:

| Focus | Handler |
|---|---|
| A block editor | `handle_key`, beside Ctrl+Z |
| The notes editor, or a plugin textarea | A key handler on the `text_editor` |
| Inspector fields, or nothing | A `KeyDown` listener on the root view, the main view that receives unprocessed keys |

### Keeping the caret

Committing a block rebuilds the block column, which would throw the caret away
on every Ctrl+S. `handle_key` records the selection in the existing
`restore_selection` slot before saving, and the rebuilt editor takes focus back.
The slot is cleared when the commit changed nothing, because no rebuild will
consume it.

A slug rename rebuilds the inspector, so focus in an inspector field can drop on
a save that renames the file. Autosave had the same effect, at a worse moment.

## Disk changes

`EditingState` keeps a baseline: the exact text of the post and notes as last
read from or written to disk. Comparing content rather than timestamps means
lopress's own saves never look like outside changes.

The existing 250 ms status poll checks the open files. It stats each one and
reads it only when the modified time or size moved. It polls because the watcher
classifies `notes/` as ignored, so the watcher can't see notes change.

The result is one of three states:

| State | Meaning |
|---|---|
| `Unchanged` | Both files match the baseline |
| `Changed` | The post or notes differ from the baseline |
| `Missing` | The post file is gone |

A missing notes file reads as empty, matching `notes::load`, so deleting notes
counts as `Changed`, not `Missing`.

Comparison is exact. Reformatting a file counts as a change.

## Refresh

A bar above the footer shows while the state isn't `Unchanged`.

- **`Changed`:** "Changed on disk" with a Refresh button.
- **`Missing`:** "This file is no longer on disk", with no button. Save writes
  the file back.

Refresh reloads the post and notes, and resets the caret and undo history. With
unsaved edits it asks first, because those edits are discarded.

If the file on disk no longer parses, Refresh keeps the open document, leaves
the bar up, and shows the parse error in it. Replacing the document with an
error screen would lose the unsaved work for nothing.

## Dialogs

One in-app modal, `ui/confirm_dialog.rs`, drawn like the site settings modal.

| Trigger | Buttons |
|---|---|
| Switching document or creating a new one, with unsaved changes | Save, Don't save, Cancel |
| Save while the file changed on disk | Overwrite, Cancel |
| Refresh with unsaved changes | Discard and refresh, Cancel |

Choosing Save in the first dialog when the file changed on disk leads to the
Overwrite dialog, and the switch only continues if that save succeeds.

Clicking the open post's own sidebar row with unsaved changes does nothing, rather
than asking about a switch that isn't happening.

## Closing

`WindowClosed` runs `active_commit`, then applies one rule:

| Unsaved? | Disk state | Result |
|---|---|---|
| No | Any | Nothing |
| Yes | `Unchanged` or `Missing` | Save, including any slug rename, with no rebuild |
| Yes | `Changed` | Nothing is written. Unsaved edits are lost |

The last row was agreed: losing the unsaved edits is better than replacing the
newer file.

## State

`dirty` and the disk state move to `root_view`, in a `Copy` bundle beside
`NotesState`. `/state` and `/open` are wired at root scope and both need them.
Without this, debug `/open` would leave `dirty` set on the newly opened post.

Every path that loads a document clears `dirty`, resets the baseline and resets
the undo stack. `make_new_doc_action` doesn't reset the undo stack today, so
undo after creating a post could apply to the wrong document. The shared path
fixes that.

## Control server

- `/state` adds `dirty` and `disk` (`unchanged`, `changed` or `missing`).
- `POST /save` runs the same save as Ctrl+S and replies `saved`, `nothing_to_save`,
  `confirm_overwrite` (the dialog is showing), `failed` or `no_document`.
- `/action` and typed input no longer reach disk until a save.

The rule against driving real sites stays: closing the window still saves.

## Failure behaviour

| Failure | Result |
|---|---|
| Save fails | Error shown, post stays unsaved, switching blocked unless Don't save is chosen |
| Refresh can't parse the file | Document kept, error in the bar |
| Refresh can't read the file | Same |
| Poll can't read a file | State unchanged until the next successful read. A file briefly absent during an outside atomic write can flash `Missing` for one tick |
| Slug rename fails | Logged, content already saved under the old name, as today |

## Known limitations

- An edit that keeps the same modified time and size is missed by the poll, so
  the bar doesn't appear. Save still catches it, because it compares content
  rather than relying on the poll. For the same reason, the poll's delay can't
  cause an overwrite.
- Debug `/close` and `/open` drop unsaved changes without asking. They exist only
  in debug builds.

## Rejected alternatives

| Alternative | Why not |
|---|---|
| Keep autosave and only add conflict detection | It keeps writing while Claude is editing, and Kyle asked to drop it |
| Use the file watcher for detection | It ignores `notes/`, and would need its own way to skip lopress's own writes |
| Compare modified times only | lopress's own saves would look like outside changes |
| Refresh automatically when the post is clean | Kyle asked for a button, and the page shouldn't change under someone reading it |
| Save notes separately from the post | Two save states for one post is confusing |
| Silently save when switching documents | The point of the change is that nothing is written without being asked |
| Discard unsaved work on every close | Loses work far more often than the agreed rule |
| Warn after the window has closed | The window is already gone, so there can be no Cancel |
| Fork floem for `WindowCloseRequested` | Declined. It comes with the planned migration |
| Native `rfd` dialogs | They can't match the Paper theme, and they block the event loop |
| Show unsaved state in the window title | The control server finds the window by its exact title |

## Verification

- **`lopress-gui-host` tests:** content comparison, missing post, missing notes
  read as empty, `Session::save` returns what it wrote.
- **`save_rules` tests:** every row of the save, close and switch decisions.
- **`ctrl` tests:** `/state` reports `dirty` and `disk`.
- **Live, on a scratch site:** typing shows Unsaved and writes nothing; Ctrl+S
  from a block, the notes and an inspector field; the caret survives Ctrl+S;
  outside edits to the post and notes raise the bar; Refresh with and without
  unsaved edits; Overwrite and Cancel; all three switch choices; a file with
  broken front matter; a deleted file; close with each disk state, confirming
  each from the files on disk.
- **Gate:** `bash scripts/check.sh`.

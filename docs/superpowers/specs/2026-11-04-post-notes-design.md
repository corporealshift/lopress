# Per-post notes drawer

**Status:** design approved, implementation plan in `docs/superpowers/plans/2026-11-04-post-notes.md`
**Date:** 2026-11-04
**Issue:** #64

## Problem

There is nowhere to think about a post before it is an article. Ideas, links,
outlines and reminders either go in the post body, where they risk being
published, or somewhere outside lopress, where they are separated from the
post they belong to.

We want a notes area per post, sized for extensive notes, visible alongside the
article while it is being written, and guaranteed never to reach the site.

## What it does

Each post and page gets a collapsible **Notes** drawer across the bottom of the
window, above the footer. Notes are plain text, autosaved, and stored in a file
of their own.

Pages are included because they share the document path with posts; excluding
them would take extra code.

### Out of scope for v1

- Markdown preview. Notes are raw text.
- A draggable drawer height. The drawer is a fixed share of the window.
- Remembering the drawer's open state across restarts.
- Picking up edits made outside the editor while the post is open. The file is
  read when the post opens, so the next save overwrites an external edit made
  in the meantime.

## Storage

`src/posts/my-idea.md` pairs with `notes/posts/my-idea.md`; pages use
`notes/pages/`.

- `notes/` is a top-level workspace folder. `discover` only reads `src/posts`
  and `src/pages`, and `lopress_watch::classify` already returns
  `Bucket::Ignored` for an unknown top-level name, so notes neither build nor
  trigger rebuilds.
- The notes path is computed from the workspace root plus the document's parent
  folder name (`posts` or `pages`) and file name. `discover` walks at depth 1,
  so documents are always directly inside one of those folders. Comparing
  folder names rather than stripping a full prefix keeps the mapping working
  for paths that arrive in a different form through the debug `/open` route.
- No file exists until there is text. Saving text that is empty or only
  whitespace deletes the file.
- Notes are tracked by git by default; the motivation for a separate file is
  keeping front matter clean, not privacy. Adding `/notes` to `.gitignore`
  keeps them out of a repo. `lopress new` does not change.

## Editor state

A single `Copy` bundle of signals, created in `root_view` so the debug ctrl
wiring at root scope can reach it:

```rust
#[derive(Clone, Copy)]
pub struct NotesState {
    /// Bumped when a different document's notes are loaded. Keys the drawer's
    /// rebuild. Deliberately not `current_path`: see "Slug renames".
    pub doc_generation: RwSignal<u64>,
    pub open: RwSignal<bool>,
    /// Live buffer, written on every edit. Single source for saves.
    pub text: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    /// Bumped on every edit; the debounced save watches it.
    pub edits: RwSignal<u64>,
    pub load_error: RwSignal<Option<String>>,
    pub save_error: RwSignal<Option<String>>,
}
```

Holding the text in a signal, rather than reading a live editor through a stored
closure, means a flush never touches a disposed view.

## Loading

One function, `reload_notes`, reads the notes for `EditingState::current_ref`,
resets the signals, and bumps `doc_generation`. All four places that change the
open document call it: sidebar open, new post/page, debug `/open`, debug
`/close`.

## Saving

Notes have their own small pipeline, separate from the post's:

- **Debounced:** 500 ms after the last edit, with no site rebuild. Notes do not
  affect the site, so a long writing session should not trigger builds.
- **On blur:** through the inner editor's `editor_view_focus_lost` Trigger,
  because the outer `text_editor` never receives `FocusLost`.
- **On document switch:** `flush_pending_edits` saves notes first. Sidebar rows
  are not focusable, so clicking one never blurs the editor. A failed save
  aborts the switch, matching post behaviour.
- **On window close:** the `WindowClosed` handler saves notes before its
  existing early return for a clean post.

The save target is read from `current_ref` at save time, so it always follows a
rename.

## Drawer UI

`ui/notes_drawer.rs`, mounted in normal flow:
`stack((columns, notes_drawer, link_bar, footer, nav_modal))`. In-flow layout
avoids floem's overlay hit-testing problems.

- A 28 px header bar toggles the drawer and is always visible. It shows a marker
  when the post has notes, and any load or save error.
- Open, the drawer takes 40% of the window height. The editor is a plain
  `text_editor_keys` with no gutter and its own internal scroll.
- The body is **hidden, not unmounted**, when collapsed. The header is not
  focusable, so collapsing does not blur the editor; unmounting would skip the
  blur save and throw away the caret and undo history.
- The editor rebuilds only when `doc_generation` changes.
- With no document open, the drawer renders nothing.

Undo inside the drawer is the text editor's own and is independent of the
article's undo stack.

## Slug renames

Renaming a post must carry its notes, without disturbing someone typing in them.

- A rename changes `current_path`, so a drawer keyed on the path would rebuild
  mid-typing. Keying on `doc_generation` avoids that.
- `EditingState::sync_filename` renames the notes file after the post file.
- The post's rename always stands once it has happened. A notes rename failure
  is returned as a warning alongside the new path, not as an `Err`, because
  returning an error would leave `current_ref` and `current_path` pointing at a
  file that no longer exists.
- `rename_notes` never overwrites. `std::fs::rename` replaces its target on
  Windows, so it checks first.
- `resolve_target` treats a name as taken if the post **or** its notes file
  exists, so a leftover notes file pushes the post to `-2` rather than being
  overwritten or adopted. For a post with no explicit slug, the URL follows the
  filename and gets the `-2` too.

## Failure behaviour

| Failure | Result |
|---|---|
| Save fails | Error in the drawer header, text stays in the buffer, still dirty, switching documents is blocked |
| Load fails | Error in the drawer header, no editor is mounted, so an empty buffer cannot overwrite the real notes |
| Notes rename fails | Warning in the drawer header. Nothing is deleted. The post is renamed and the old notes file stays under the old name. The buffer still holds the text, so the next edit writes it under the new name |

Notes errors use their own signal rather than the footer's `save_error_sig`. A
successful post save clears that shared signal, which would hide a failed notes
save.

## Known limitations

- A new post created under a name that already has a leftover notes file picks
  up those notes. Nothing is lost, and lopress has no delete action, so this
  needs a post file removed outside the editor.
- Debug `/close` drops the session without flushing, as it already does for
  posts. It exists only in debug builds.

## Rejected alternatives

| Alternative | Why not |
|---|---|
| `notes:` key in front matter | Extensive notes would pollute the front matter |
| `my-idea.notes.md` beside the post | `discover` publishes every `*.md` in the folder, so it would become a real post |
| `my-idea.notes.txt` beside the post | The watcher treats all of `src/` as source, so each save would trigger a rebuild |
| Right drawer | At the default 1200 px window, sidebar 220 + inspector 280 + a 320 px drawer leaves the article about 380 px |
| Swapping the center pane | Most room, but notes and article cannot be seen together |
| Separate OS window | The app is built around one window; following the open post, closing and saving would all be new work |
| Block editor for notes | Far heavier than scratch notes need, and they would have to be stored as blocks |
| Reusing the post save pipeline | Rebuilds the site on every notes save |
| Keying the drawer on `current_path` | A slug rename would rebuild the editor mid-typing |

## Verification

- **`lopress-gui-host` tests:** path mapping for posts and pages, non-document
  paths, missing file reads as empty, save creates folders, clearing deletes the
  file, rename moves the file, rename refuses to overwrite.
- **`filename_sync` tests:** a leftover notes file makes a name taken.
- **Build test:** notes text never appears anywhere in `www/`.
- **Watcher test:** paths under `notes/` classify as ignored.
- **Live, on a scratch site:** type notes, switch posts straight away, rename
  the slug while the caret is in the notes, collapse and reopen the drawer,
  close and reopen the app, confirming each from the files on disk. `/state`
  reports the notes text, dirty flag and open state.
- **Gate:** `bash scripts/check.sh`.

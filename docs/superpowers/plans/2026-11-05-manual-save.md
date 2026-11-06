# Manual save and refresh from disk: implementation plan

**Spec:** `docs/superpowers/specs/2026-11-05-manual-save-design.md`
**Branch:** `feature/manual-save`, from `main`

**Approach:** build everything manual save needs while autosave still runs, and
remove autosave last. Every step before step 6 only adds behaviour, so the tree
stays green and usable, and step 6 is the one behaviour switch.

Every step ends with `bash scripts/check.sh` passing, after touching the changed
`.rs` files so clippy re-lints. Live checks run on a `lopress new` scratch site
only. Don't add tests that call `Session::open` from the editor crate: they hit
the Windows CI hang in #63.

## Step 1: Disk comparison in the host

**Depends on:** nothing.

**Changes:**

- New `crates/lopress-gui-host/src/disk.rs`:
  - `pub enum DiskState { Unchanged, Changed, Missing }`
  - `pub struct DiskBaseline { post: String, notes: String }`
  - `pub fn compare(post: &Path, notes: Option<&Path>, baseline: &DiskBaseline) -> std::io::Result<DiskState>`,
    the content check. A missing post is `Missing`. A missing notes file reads as
    empty, through `notes::load`.
  - `pub struct DiskStamp`, holding modified time and size for both files, with
    `pub fn read(...)`, so the poll can skip reading unchanged files.
- `lib.rs`: `pub mod disk;`
- `document.rs`: `LoadedDocument` gains `pub raw: String`, the text read from disk.
- `session.rs`: `load_document` fills `raw`. `Session::save` returns
  `Result<String, SaveError>`, the text it wrote.
- `state.rs`: fill the new field, and map the save result so `save_doc` still
  returns `Result<(), String>` for now.
- `tests/session_integration.rs`: keep compiling.

**Proof:** new tests in `disk.rs` pass: unchanged, post differs, notes differ,
post missing, notes missing with an empty baseline is `Unchanged`, notes missing
with a non-empty baseline is `Changed`. A `session_integration` test asserts
`save` returns exactly the bytes on disk. `cargo test -p lopress-gui-host` then
the gate.

## Step 2: Save rules and shared status

**Depends on:** step 1.

**Changes:**

- New `ui/editing/save_rules.rs`, pure with no floem types:
  - `decide_save(unsaved, disk) -> SaveStep`: `NothingToDo`, `ConfirmOverwrite`,
    `Write`.
  - `decide_close(unsaved, disk) -> CloseStep`: `Nothing`, `Save`.
  - `decide_switch(unsaved) -> SwitchStep`: `Proceed`, `Confirm`.
  - `decide_refresh(unsaved, disk) -> RefreshStep`: `Unavailable`, `Reload`,
    `ConfirmDiscard`.
- New `DocStatus` in `ui/editing/save_pipeline.rs`: a `Copy` bundle holding
  `dirty: RwSignal<bool>` and `disk: RwSignal<DiskState>`. Create it in
  `root_view` beside `NotesState`, and pass it to `editing_view` and
  `wire_ctrl_root`. `SavePipeline::dirty_sig` becomes `status.dirty`.
- `state.rs`: `EditingState` gains `baseline: Option<DiskBaseline>`, set by
  `open_document` from `raw` plus the loaded notes. `save_doc` and `save_notes`
  update it with what they wrote.
- New `after_doc_loaded(status, notes, undo_stack, editing)` helper: clears
  `dirty`, sets `disk` to `Unchanged`, resets the undo stack, calls
  `reload_notes`. Used by `on_open`, `make_new_doc_action`, and the ctrl
  `/open`. `/open` runs at root scope, so lift `undo_stack` to `root_view` too.
- `ctrl/mod.rs`, `ctrl_wire.rs`: `/state` reports `dirty` and `disk`.

**Proof:** `save_rules` unit tests cover every row of the spec's tables. The
`serialize_state` test asserts `dirty` and `disk`. Behaviour is otherwise
unchanged. Live: on a scratch site, `/state` shows `dirty:true` right after an
`/action`, then `false` once autosave fires.

## Step 3: Save action, Ctrl+S and the Save button

**Depends on:** step 2.

**Changes:**

- `save_pipeline.rs`: new `save_now(...) -> SaveOutcome` following the spec's
  five steps, with `SaveOutcome` being `Saved`, `NothingToSave`,
  `NeedsOverwrite`, `Failed`, `NoDocument`. `NeedsOverwrite` does nothing yet
  beyond returning; step 5 adds the dialog. Check the disk by content here, not
  from the poll's state. Expose it as a `Rc<dyn Fn(Option<SelectionRestore>) -> SaveOutcome>`
  called `on_save`.
- `edit_rev`: keep `dirty_counter` as a public revision signal so `save_now` can
  tell whether the commit changed anything. Record the `SelectionRestore` before
  committing, and clear the slot when `edit_rev` didn't move.
- `ui/blocks/env.rs`: `BlockEnv` gains `on_save` and `on_buffer_edit: Rc<dyn Fn()>`.
- `ui/blocks/inline_editor.rs`:
  - `handle_key`: `"s" | "S"` with Ctrl or Cmd reads the cursor's selection and
    calls `on_save`.
  - `mount_block_editor`: an effect on `text_sig`, skipping its first run, calls
    `on_buffer_edit`, which sets `status.dirty`.
- `ui/notes_drawer.rs`: switch to `text_editor_keys` and catch Ctrl+S before the
  default handler.
- `ui/mod.rs`: `save_requested: Trigger` at root scope, notified by a `KeyDown`
  listener on the `dyn_container` returned from `root_view`, which is the main
  view. `editing_view` runs `on_save(None)` on it.
- `ui/footer.rs`: a Save button beside the status label, dimmed when clean.
  Unsaved reads from `status.dirty` or `notes.dirty`.
- `ctrl/mod.rs`, `ctrl_wire.rs`: `POST /save`, wired at editing scope like
  `wire_ctrl_action`, replying with the outcome as JSON.

**Proof:** live, with autosave still running, so watch `/state` and the file
straight after the key:

1. Type in a paragraph. `/state` shows `dirty:true` before any blur.
2. Ctrl+S from a paragraph, a list item, a table cell and a code block: each
   file holds the text, and more typing lands after the same caret position.
3. Ctrl+S from the notes editor and from the inspector's Title field.
4. Save button with a focused paragraph.
5. `POST /save` replies `saved`, then `nothing_to_save`.

If Ctrl+S in the notes or an inspector field doesn't arrive, record what floem
did with the key before changing the routing.

## Step 4: Disk polling, the bar and Refresh

**Depends on:** step 3.

**Changes:**

- `save_pipeline.rs`: extend the 250 ms poll with a stamp check. When the stamp
  moved, run `disk::compare` and set `status.disk`. Keep the last stamp in the
  closure, and reset it in `after_doc_loaded`. Read errors leave the state alone.
- `state.rs`: `reload_current() -> Result<(), String>`. It parses first and
  replaces `current_doc` only on success, so a broken file keeps the open
  document.
- New `ui/disk_bar.rs`: mounted in flow above the footer, collapsed when
  `Unchanged`, showing the message, the Refresh button when `Changed`, and any
  refresh error. No confirmation yet: with unsaved changes Refresh is disabled
  until step 5.
- `ui/mod.rs`: `stack((columns, notes_drawer, link_bar, disk_bar, footer, nav_modal))`.
  Refresh calls `reload_current`, then `current_doc.set`, `current_path.set` with
  the same path so the inspector reseeds, then `after_doc_loaded`.

**Proof:** live, writing to the files from PowerShell:

1. Edit the post on disk. `/state` shows `disk:"changed"` within a second, and a
   screenshot shows the bar.
2. Refresh. Blocks, inspector title and word count show the new text, and
   `disk` returns to `unchanged`.
3. The same for a notes file, and for deleting one.
4. Ctrl+S after an edit in lopress: `disk` stays `unchanged`.
5. Delete the post: `missing`, no Refresh button. Save writes it back.
6. Break the front matter: Refresh keeps the document and shows the error.

## Step 5: Dialogs

**Depends on:** step 4.

**Changes:**

- New `ui/confirm_dialog.rs`: a `ConfirmRequest` with a message and a list of
  labelled `Rc<dyn Fn()>` buttons, in an `RwSignal<Option<ConfirmRequest>>`, drawn
  as a full-height absolute layer so the backdrop takes clicks.
- `ui/editing/save_pipeline.rs`: replace `flush_pending_edits` with
  `guard_switch(then: Rc<dyn Fn()>)`: commit, then use `decide_switch` to either
  run `then` or show Save, Don't save and Cancel. Save continues only on `Saved`.
- `save_now`: on `NeedsOverwrite`, show Overwrite and Cancel. Overwrite runs the
  write with the disk check skipped, and calls any pending continuation.
- `ui/mod.rs`, `new_doc.rs`: `on_open` and `make_new_doc_action` go through
  `guard_switch`.
- `ui/disk_bar.rs`: Refresh uses `decide_refresh`, and shows Discard and refresh
  with Cancel when unsaved.

While autosave still runs in this step, dialogs appear only inside its 500 ms
window. That's expected, and step 6 makes them the normal path.

**Proof:** `cargo test` passes, and in the live run with typing and a click inside
500 ms, the switch dialog appears. Full dialog checks wait for step 6, where
timing doesn't interfere.

## Step 6: Remove autosave

**Depends on:** step 5.

**Changes:**

- `save_pipeline.rs`: delete both `debounce_action` blocks. `mark_dirty` sets
  `status.dirty` and bumps `edit_rev`.
- `ui/editing/notes.rs`: remove `edits`.
- `ui/notes_drawer.rs`: remove the blur save and the `edits` bump.
- `ui/mod.rs`: `WindowClosed` runs `active_commit`, then `decide_close`. On
  `Save` it writes the post and notes and syncs the filename, with no rebuild.
- Update the stale comments on save paths, including the claim that sidebar
  clicks never blur the editor.

**Proof:** live, confirmed from the files on disk:

1. Type, wait 3 seconds: the file is unchanged, `dirty:true`, and `www/` is
   untouched. Ctrl+S: file and `www/` update.
2. Notes behave the same, and a notes-only save doesn't touch `www/`.
3. Switch with unsaved changes: Cancel stays; Don't save drops the edits; Save
   writes them. Repeat for New post.
4. Edit on disk with unsaved changes: Save offers Overwrite; Cancel leaves the
   outside edit on disk; Overwrite replaces it. Refresh offers discard.
5. Switch with Save while the file changed on disk: Overwrite dialog, and Cancel
   stays on the post.
6. A save failure: make the file read-only, Ctrl+S shows the error, and switching
   still asks.
7. Slug change: nothing renames until Save, then the post and notes both move.
8. Close with unsaved changes after typing in a focused block: the text is on
   disk after relaunch. Close again with the file changed on disk: the outside
   edit survives. Use a real window close, not `/close`.

## Step 7: Docs

**Depends on:** step 6.

**Changes:**

- `README.md`: saving, Refresh, and the crash risk.
- `docs/architecture.md`: the save section, including the filename sync paragraph
  around line 800.
- `CLAUDE.md`, `.claude/skills/driving-lopress-editor/SKILL.md`,
  `.claude/skills/verifying-lopress-work/SKILL.md`: replace the autosave wording.
  Keep the scratch-site rule, since closing still saves. Add `/save` and the new
  `/state` fields.
- `.claude/skills/building-on-floem/SKILL.md`: add the key routing rule, that only
  unprocessed keys reach the main view.
- `docs/superpowers/specs/2026-11-04-post-notes-design.md`: add one line at the
  top pointing to this spec for saving.

**Proof:** `git grep -i autosave -- ':!docs/superpowers'` returns nothing that
describes current behaviour, and the gate passes on the final commit with
`rustup check` showing `stable` up to date.

## Seams

- `save_pipeline.rs` changes in steps 2 to 6. Steps 3 and 5 add to it, and step 6
  only deletes, so the debounce blocks stay untouched until then.
- `ui/mod.rs` changes in steps 2 to 6. `editing_view` is already long, so put new
  wiring in `ui/editing/` modules and keep `mod.rs` to the calls.
- `notes_drawer.rs` changes in steps 3 and 6. The `text_editor_keys` switch must
  keep the existing `cache_rev` and focus effects.
- `flush_pending_edits` lasts until step 5, and `WindowClosed` keeps its current
  code until step 6, so saves stay safe in between.

## Not doing

- Anything in the spec's out-of-scope list.
- The Paper theme or the floem migration. Each gets its own spec and PR after
  this merges. New UI here uses plain styling for Paper to restyle.
- Opening a PR. That's a separate decision once this is verified.

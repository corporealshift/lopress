# Per-Post Notes Drawer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each post and page a collapsible bottom drawer for extensive notes, stored in `notes/` outside `src/`, so they never reach the site (issue #64).

**Spec:** `docs/superpowers/specs/2026-11-04-post-notes-design.md`

**Architecture:** File operations are free functions in a new `lopress_gui_host::notes` module, with thin `Session` delegates. `EditingState` applies them to the open document. A `Copy` bundle of signals (`NotesState`), created in `root_view`, carries the live buffer. Notes have their own debounced save with no site rebuild, plus explicit flushes on document switch and window close. The drawer is an in-flow view above the footer.

**Tech stack:** Rust, floem 0.2, `std::fs`, `tempfile` (tests).

**Branch:** `feature/64-post-notes`. All tasks land here in order; no stacked branches.

## Rules for every task

- Each task ends with `bash scripts/check.sh` passing. After any other cargo command, `touch` the changed `.rs` files first, or clippy can report a false green.
- No `.unwrap()`, `.expect()`, indexing, or integer division outside tests.
- Drive the live editor only on a scratch site: `cargo run --quiet -- new $env:TEMP\lopress-scratch`. Read the `driving-lopress-editor` skill first. Report results per the `verifying-lopress-work` skill.
- Do not add tests that call `Session::open` from the editor crate. It starts the watcher and server, and those tests already hang intermittently on Windows CI (#63). Keep file logic in `notes.rs`, where a `tempdir` is enough.

## Seams

| File | Tasks | Order matters because |
|---|---|---|
| `crates/lopress-editor/src/state.rs` | 2, 3 | Task 2 changes the `sync_filename` return type that Task 3 builds on |
| `ui/editing/filename_sync.rs` | 2, 3 | Task 2 logs the warning with `eprintln!`; Task 3 routes it to `NotesState` |
| `ui/editing/save_pipeline.rs` | 2, 3, 4 | Task 2 adapts to the new return type, Task 3 adds the flush, Task 4 adds the debounce |
| `ui/mod.rs` | 3, 4 | Task 3 creates `NotesState` and wires it, Task 4 mounts the view |

---

### Task 1: Notes file operations

**Depends on:** nothing.

**Files:**
- Create: `crates/lopress-gui-host/src/notes.rs`
- Modify: `crates/lopress-gui-host/src/lib.rs` (add `pub mod notes;`)
- Modify: `crates/lopress-gui-host/src/session.rs` (`atomic_write` becomes `pub(crate)`; add delegates)
- Modify: `crates/lopress-watch/src/classify.rs` (test only)
- Modify: `crates/lopress-build/tests/build_integration.rs` (test only)

- [ ] **Step 1: Write the module with its tests**

```rust
//! Per-document notes files.
//!
//! `src/posts/my-idea.md` pairs with `notes/posts/my-idea.md`. Notes live
//! outside `src/` so neither the build nor the watcher sees them.

use crate::session::atomic_write;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// The notes file for `doc`, or `None` when `doc` is not directly inside a
/// `posts` or `pages` folder.
///
/// Compares the parent folder's name rather than stripping a full prefix, so
/// paths that arrive in a different form (debug `/open`) still map.
pub fn notes_path(root: &Path, doc: &Path) -> Option<PathBuf> {
    let folder = doc.parent()?.file_name()?.to_str()?;
    if !matches!(folder, "posts" | "pages") {
        return None;
    }
    Some(root.join("notes").join(folder).join(doc.file_name()?))
}

/// Read notes. A missing file is empty notes, not an error.
///
/// # Errors
/// Returns the I/O error for anything other than a missing file.
pub fn load(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Write notes, creating folders as needed. Text that is empty or only
/// whitespace deletes the file, so an empty note leaves nothing on disk.
///
/// # Errors
/// Returns the underlying I/O error.
pub fn save(path: &Path, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, text.as_bytes())
}

/// Move a notes file. A missing source is a no-op.
///
/// # Errors
/// Returns `AlreadyExists` rather than overwriting `to` (`std::fs::rename`
/// replaces its target on Windows), or the underlying I/O error.
pub fn rename(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    std::fs::rename(from, to)
}
```

Tests in the same file, using `tempfile::tempdir()`:

- `notes_path` maps a post, maps a page, returns `None` for `src/other/x.md`
- `load` of a missing file returns `""`
- `save` creates missing folders and round-trips through `load`
- `save` of `""` and of `"  \n"` deletes an existing file
- `save` of `""` with no file present is `Ok`
- `rename` moves the file; `rename` from a missing file is `Ok`
- `rename` onto an existing file returns `AlreadyExists` and leaves both files unchanged

- [ ] **Step 2: Add `Session` delegates**

In `session.rs`, change `fn atomic_write` to `pub(crate) fn atomic_write`, then add:

```rust
/// The notes file for `doc`, if it is a post or page.
pub fn notes_path_for(&self, doc: &Path) -> Option<PathBuf> {
    crate::notes::notes_path(&self.workspace.root, doc)
}

/// Read the notes for `doc`. A document with no notes reads as empty.
///
/// # Errors
/// Returns the I/O error when the notes file exists but can't be read.
pub fn load_notes(&self, doc: &Path) -> std::io::Result<String> {
    match self.notes_path_for(doc) {
        Some(path) => crate::notes::load(&path),
        None => Ok(String::new()),
    }
}

/// Save the notes for `doc`.
///
/// # Errors
/// Returns `InvalidInput` when `doc` is not a post or page, so text is never
/// silently dropped, or the underlying I/O error.
pub fn save_notes(&self, doc: &Path, text: &str) -> std::io::Result<()> {
    let Some(path) = self.notes_path_for(doc) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "document is not in posts or pages",
        ));
    };
    crate::notes::save(&path, text)
}

/// Move the notes that follow a document rename.
///
/// # Errors
/// Returns the error from [`crate::notes::rename`].
pub fn rename_notes(&self, from_doc: &Path, to_doc: &Path) -> std::io::Result<()> {
    match (self.notes_path_for(from_doc), self.notes_path_for(to_doc)) {
        (Some(from), Some(to)) => crate::notes::rename(&from, &to),
        _ => Ok(()),
    }
}
```

- [ ] **Step 3: Add the isolation tests**

In `classify.rs`:

```rust
#[test]
fn notes_folder_is_ignored() {
    assert_eq!(
        classify(&ws(), &ws().join("notes/posts/hello.md")),
        Bucket::Ignored
    );
}
```

In `build_integration.rs`, a test that copies the `minimal` fixture, writes `notes/posts/hello.md` containing a unique marker such as `NOTES-MUST-NOT-PUBLISH`, runs `build(&root)`, then walks `www/` with `walkdir` and asserts that no file's bytes contain the marker and no path contains `notes`.

- [ ] **Step 4: Prove it**

Run: `cargo test -p lopress-gui-host notes` — all notes tests pass.
Run: `cargo test -p lopress-watch notes_folder` and `cargo test -p lopress-build --test build_integration notes` — pass.
Run: `bash scripts/check.sh` — exits 0.

---

### Task 2: Notes follow slug renames

**Depends on:** Task 1.

**Files:**
- Modify: `crates/lopress-editor/src/ui/editing/filename_sync.rs`
- Modify: `crates/lopress-editor/src/state.rs`

- [ ] **Step 1: Inject the "taken" check**

Change `rename_to_slug` to take it, and update the two existing tests to pass `|p| p.exists()`:

```rust
pub fn rename_to_slug(
    fm: &FrontMatter,
    current: &Path,
    taken: impl Fn(&Path) -> bool,
) -> std::io::Result<Option<PathBuf>> {
    let Some(target) = resolve_target(fm, current, taken) else {
        return Ok(None);
    };
    std::fs::rename(current, &target)?;
    Ok(Some(target))
}
```

Add a test: with a `taken` closure that also reports `hello.md` as taken though no such file exists, a post titled `Hello` renames to `hello-2.md`.

- [ ] **Step 2: Return a warning instead of failing**

In `state.rs`:

```rust
/// A completed rename of the open document.
pub struct FilenameSync {
    pub new_path: PathBuf,
    /// Set when the post moved but its notes could not. Not an `Err`: the
    /// post's rename has already happened, and `current_ref` must follow it.
    pub notes_warning: Option<String>,
}
```

Rewrite `sync_filename` to return `Result<Option<FilenameSync>, String>`:

```rust
pub fn sync_filename(
    &mut self,
    front_matter: &FrontMatter,
) -> Result<Option<FilenameSync>, String> {
    let Some(current) = self.current_ref.as_ref().map(|r| r.path.clone()) else {
        return Ok(None);
    };
    // A leftover notes file makes a name unavailable, so it is never
    // overwritten or adopted by this post.
    let session = &self.session;
    let taken = |p: &Path| {
        p.exists() || session.notes_path_for(p).is_some_and(|n| n.exists())
    };
    match crate::ui::editing::filename_sync::rename_to_slug(front_matter, &current, taken) {
        Ok(Some(new_path)) => {
            if let Some(r) = self.current_ref.as_mut() {
                r.path = new_path.clone();
            }
            let notes_warning = self
                .session
                .rename_notes(&current, &new_path)
                .err()
                .map(|e| format!("notes not moved: {e}"));
            Ok(Some(FilenameSync { new_path, notes_warning }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
```

Add `use std::path::Path;` to `state.rs`.

- [ ] **Step 3: Adapt the caller**

In `sync_filename_and_update`, change the `Ok(Some(new_path))` arm to destructure `FilenameSync`. For now, log the warning with `eprintln!` gated on `#[cfg(debug_assertions)]`; Task 3 replaces this.

- [ ] **Step 4: Prove it**

Run: `cargo test -p lopress-editor filename_sync` — all pass, including the new test.
Run: `bash scripts/check.sh` — exits 0.

---

### Task 3: Notes state, loading, flushing, and `/state`

**Depends on:** Task 2.

**Files:**
- Create: `crates/lopress-editor/src/ui/editing/notes.rs`
- Modify: `crates/lopress-editor/src/ui/editing/mod.rs` (add `pub mod notes;`)
- Modify: `crates/lopress-editor/src/state.rs`
- Modify: `crates/lopress-editor/src/ui/editing/save_pipeline.rs`
- Modify: `crates/lopress-editor/src/ui/editing/filename_sync.rs`
- Modify: `crates/lopress-editor/src/ui/editing/new_doc.rs`
- Modify: `crates/lopress-editor/src/ui/editing/ctrl_wire.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`
- Modify: `crates/lopress-editor/src/ctrl/mod.rs`

- [ ] **Step 1: `EditingState` methods**

```rust
/// Read the open document's notes. No open document reads as empty.
pub fn load_notes(&self) -> Result<String, String> {
    match &self.current_ref {
        Some(r) => self.session.load_notes(&r.path).map_err(|e| e.to_string()),
        None => Ok(String::new()),
    }
}

/// Save notes for the open document. The path comes from `current_ref` at
/// call time, so a save always follows a rename.
pub fn save_notes(&self, text: &str) -> Result<(), String> {
    let path = self
        .current_ref
        .as_ref()
        .map(|r| r.path.as_path())
        .ok_or_else(|| "no document open".to_string())?;
    self.session.save_notes(path, text).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: `ui/editing/notes.rs`**

```rust
//! Reactive state and persistence for the per-document notes drawer.

use crate::state::EditingState;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use std::cell::RefCell;
use std::rc::Rc;

/// Signals behind the notes drawer. Created in `root_view` so the debug ctrl
/// wiring at root scope can reach them.
#[derive(Clone, Copy)]
pub struct NotesState {
    /// Bumped when a different document's notes load. Keys the drawer's
    /// rebuild. Not `current_path`: a slug rename changes that, and would
    /// rebuild the editor mid-typing.
    pub doc_generation: RwSignal<u64>,
    pub open: RwSignal<bool>,
    /// Live buffer, written on every edit. The single source for saves.
    pub text: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    /// Bumped on every edit; the debounced save watches it.
    pub edits: RwSignal<u64>,
    pub load_error: RwSignal<Option<String>>,
    /// Separate from the footer's `save_error_sig`, which a successful post
    /// save clears.
    pub save_error: RwSignal<Option<String>>,
}

impl NotesState {
    pub fn new() -> Self {
        Self {
            doc_generation: RwSignal::new(0),
            open: RwSignal::new(false),
            text: RwSignal::new(String::new()),
            dirty: RwSignal::new(false),
            edits: RwSignal::new(0),
            load_error: RwSignal::new(None),
            save_error: RwSignal::new(None),
        }
    }
}

impl Default for NotesState {
    fn default() -> Self {
        Self::new()
    }
}

/// Load the open document's notes into `notes` and rebuild the drawer.
///
/// Call after the document changes, and with no `editing` borrow held.
pub fn reload_notes(notes: NotesState, editing: &Rc<RefCell<Option<EditingState>>>) {
    let loaded = editing
        .borrow()
        .as_ref()
        .map_or_else(|| Ok(String::new()), EditingState::load_notes);
    match loaded {
        Ok(text) => {
            notes.text.set(text);
            notes.load_error.set(None);
        }
        Err(e) => {
            notes.text.set(String::new());
            notes.load_error.set(Some(e));
        }
    }
    notes.dirty.set(false);
    notes.save_error.set(None);
    notes.doc_generation.update(|n| *n = n.wrapping_add(1));
}

/// Save the buffer if it has unsaved edits. Returns `false` when the save
/// failed; the text stays in the buffer and the error in `save_error`.
pub fn save_notes(notes: NotesState, editing: &Rc<RefCell<Option<EditingState>>>) -> bool {
    if !notes.dirty.get_untracked() {
        return true;
    }
    let text = notes.text.get_untracked();
    let result = editing.borrow().as_ref().map(|s| s.save_notes(&text));
    match result {
        // No session: nothing to save into.
        None => true,
        Some(Ok(())) => {
            notes.dirty.set(false);
            notes.save_error.set(None);
            true
        }
        Some(Err(msg)) => {
            notes.save_error.set(Some(format!("notes not saved: {msg}")));
            false
        }
    }
}
```

- [ ] **Step 3: Wire flushing and the rename warning**

- `FlushSignals` gains `pub notes: NotesState`.
- `flush_pending_edits`: after running `active_commit` and **before** `if !dirty_sig.get_untracked()`, add `if !notes::save_notes(notes, editing) { return false; }`. It must come before that early return, or a clean post skips the notes.
- `sync_filename_and_update` gains a `notes: NotesState` parameter; replace Task 2's `eprintln!` with `notes.save_error.set(Some(warning))`. Update both call sites.
- `start_save_pipeline` gains a `notes: NotesState` parameter, passed through to the debounced `sync_filename_and_update` call.

- [ ] **Step 4: Wire `reload_notes` into all four document changes**

`on_open` in `ui/mod.rs` and the closure in `new_doc.rs` both hold a `borrow_mut` guard to the end of the closure. `reload_notes` borrows `editing`, so calling it under that guard panics at runtime with no compile error. Scope the guard in a block first:

```rust
{
    let mut guard = editing_for_open.borrow_mut();
    let Some(state) = guard.as_mut() else {
        return;
    };
    state.open_document(&doc_ref);
    current_doc.set(state.current_doc.clone());
    current_path.set(Some(doc_ref.path));
}
undo_stack.update(|s| *s = crate::undo::UndoStack::new());
notes::reload_notes(notes, &editing_for_open);
```

- `ui/mod.rs`: create `let notes = NotesState::new();` in `root_view` beside `current_path`. Pass it to `editing_view`, `wire_ctrl_root`, and `FlushSignals`.
- `new_doc.rs`: scope the guard the same way, then call `reload_notes`. The closure gets `notes` from `flush_signals.notes`.
- `ctrl_wire.rs`: `open_document_by_path` takes `notes` and calls `reload_notes` after `current_path.set`, with no borrow held. The close effect calls `reload_notes` after `*editing_for_close.borrow_mut() = None`, which resets the buffer to empty.

- [ ] **Step 5: Save on window close**

In the `WindowClosed` handler in `ui/mod.rs`, add `let _ = notes::save_notes(notes, &editing_for_close);` as the **first** statement, before `if !save.dirty_sig.get_untracked() { return; }`.

- [ ] **Step 6: Report notes in `/state`**

In `ctrl/mod.rs`:

```rust
/// The notes fields reported by `/state`.
pub(crate) struct NotesSnapshot {
    pub open: bool,
    pub dirty: bool,
    pub text: String,
    pub load_error: Option<String>,
    pub save_error: Option<String>,
}
```

Give `serialize_state` a third parameter, `notes: &NotesSnapshot`, and add this key to the final `json!`. Leave the no-document return unchanged:

```rust
"notes": {
    "open": notes.open,
    "dirty": notes.dirty,
    "text": notes.text,
    "load_error": notes.load_error,
    "save_error": notes.save_error,
},
```

In `wire_ctrl_root`, build the snapshot inside the effect with tracked `.get()` reads of all five signals, so `/state` updates when notes change.

Add a unit test in `ctrl/mod.rs`: `serialize_state` with an empty `EditorDoc` includes `notes.text` and `notes.dirty`.

- [ ] **Step 7: Prove it**

Run: `bash scripts/check.sh` — exits 0.

Live, on a scratch site:

1. Write `notes/posts/hello.md` containing `seed notes` by hand.
2. `cargo run`, then `POST /open` for `src/posts/hello.md`.
3. `GET /state` — must show `"notes":{"open":false,"dirty":false,"text":"seed notes\n",...}`.
4. Make `notes/posts/hello.md` a folder instead of a file, `POST /open` again, and `GET /state` — `load_error` must be set and `text` empty.

---

### Task 4: The drawer

**Depends on:** Task 3.

**Files:**
- Create: `crates/lopress-editor/src/ui/notes_drawer.rs`
- Modify: `crates/lopress-editor/src/ui/mod.rs`
- Modify: `crates/lopress-editor/src/ui/editing/save_pipeline.rs`

Read the `building-on-floem` skill first.

- [ ] **Step 1: Debounced save**

In `start_save_pipeline`, beside the existing debounce:

```rust
// Notes save on their own timer, with no rebuild: they don't affect the site.
{
    let editing_for_notes = Rc::clone(&editing);
    debounce_action(notes.edits, Duration::from_millis(500), move || {
        crate::ui::editing::notes::save_notes(notes, &editing_for_notes);
    });
}
```

- [ ] **Step 2: Build the view**

`pub fn notes_drawer_view(notes: NotesState, current_path: RwSignal<Option<PathBuf>>, editing: Rc<RefCell<Option<EditingState>>>) -> impl IntoView`, a `v_stack((header, body))`:

**Outer style**, reactive:

```rust
.style(move |s| {
    let open = notes.open.get();
    s.width_full()
        .flex_shrink(0.0f32)
        .border_top(1.)
        .border_color(BORDER)
        .background(BG)
        .apply_if(open, |s| s.height_pct(40.))
        .apply_if(!open, |s| s.height(HEADER_HEIGHT))
        .apply_if(current_path.get().is_none(), |s| s.hide())
})
```

**Header**, 28 px, with `.on_click_stop(move |_| notes.open.update(|o| *o = !*o))`:

- A label reading `▾ Notes` when open and `▸ Notes` when closed
- A `●` marker shown when `notes.text` is not blank, so a collapsed drawer shows the post has notes
- An error label, `flex_grow(1.0f32)`, showing `load_error` or else `save_error`, in `Color::rgb8(170, 40, 40)`

**Body** keyed on the generation, and hidden rather than removed when collapsed:

```rust
let body = dyn_container(
    move || notes.doc_generation.get(),
    move |_| notes_editor(notes, Rc::clone(&editing)).into_any(),
)
.style(move |s| {
    s.width_full()
        .flex_grow(1.0f32)
        .min_height(0.)
        .apply_if(!notes.open.get(), |s| s.hide())
});
```

**`notes_editor`:** when `load_error` is set, return `empty()` with no editor, so an empty buffer can't overwrite real notes. Otherwise:

```rust
let initial = notes.text.get_untracked();
let text_ed = text_editor(Rope::from(initial.as_str()))
    .placeholder("Notes for this post…");
let editor = text_ed.editor().clone();

// Mirror every edit into the buffer. `cache_rev` is the same trigger the
// plugin textarea uses; `doc().text()` reads untracked. The comparison makes
// the effect's first run a no-op.
let doc_rev = text_ed.doc().cache_rev();
let ed_for_text = editor.clone();
create_effect(move |_| {
    doc_rev.get();
    let live = String::from(&ed_for_text.doc().text());
    if notes.text.with_untracked(|t| *t != live) {
        notes.text.set(live);
        notes.dirty.set(true);
        notes.edits.update(|n| *n = n.wrapping_add(1));
    }
});

// Save on blur. `FocusLost` on the outer view never fires; use the inner
// editor's trigger. Skip the effect's first run.
let focus_lost = editor.editor_view_focus_lost;
create_effect(move |prev: Option<()>| {
    focus_lost.track();
    if prev.is_some() {
        crate::ui::editing::notes::save_notes(notes, &editing);
    }
});

text_ed
    .editor_style(|s| s.hide_gutter(true))
    .style(|s| s.width_full().height_full().font_size(13.).padding(8.))
```

Add `pub mod notes_drawer;` to `ui/mod.rs`.

- [ ] **Step 3: Mount it**

In `editing_view`, build the drawer and mount it between `columns` and `link_bar`:

```rust
stack((columns, notes_drawer, link_bar, footer, nav_modal))
```

- [ ] **Step 4: Prove it**

Run: `bash scripts/check.sh` — exits 0.

Live, on a scratch site with two posts, confirming each from `/state`, `/screenshot` and the files on disk. Use `/click` to focus before `/input`; Windows foreground lock makes `/input` flaky otherwise.

1. **Toggle:** click the header. The drawer opens to about 40% height and the article still scrolls above it. Click again and it collapses, with no gap left behind.
2. **Autosave:** type text. Within a second `notes/posts/<stem>.md` holds it and `notes.dirty` is `false`.
3. **No rebuild:** note the mtime of `www/posts/<slug>/index.html`, type more notes, wait 2 seconds. The mtime has not changed.
4. **Switch straight away:** type, then click another post in the sidebar immediately, inside the 500 ms window. The first post's notes file holds the final text and the drawer shows the second post's notes.
5. **Empty:** clear the text. The notes file is gone.
6. **Rename:** with notes saved, change the title, click into the notes, and keep typing through the rename. The notes file moved to the new stem, the caret stayed, and no characters were lost.
7. **Leftover file:** create `notes/posts/taken.md` by hand, then title a post `Taken`. The post becomes `taken-2.md` and `taken.md` is unchanged.
8. **Collapse mid-edit:** type, then collapse within 500 ms. The text still saves, and reopening shows the text with the caret intact.
9. **Undo:** Ctrl+Z in the drawer changes notes only; article blocks in `/state` are unchanged.
10. **Save failure:** make `notes/posts` unwritable (for example, replace the folder with a file), type, and see the error in the header. Switching posts is blocked and the text stays in the drawer.
11. **Close and reopen:** type, close the window straight away, relaunch. The text is on disk and in the drawer.
12. **Link bar:** open the link bar with the drawer open and confirm it is still usable. It is positioned relative to the footer, so check that it doesn't cover the drawer in a way that blocks work.

If any check fails, use the `debugging` skill, fix it, and rerun the full list.

---

### Task 5: Documentation and final gate

**Depends on:** Task 4.

**Files:**
- Modify: `docs/architecture.md` (UI component table, the save pipeline section, and a note that `notes/` is outside the build)
- Modify: `.claude/skills/driving-lopress-editor/SKILL.md` (`/state` shape gains `notes`)
- Modify: `README.md` (one line on the notes drawer and the optional `/notes` gitignore entry)

- [ ] **Step 1: Update the docs above**

- [ ] **Step 2: Prove it**

Run: `rustup update stable`, then `touch` every changed `.rs` file, then `bash scripts/check.sh` — exits 0 on the toolchain CI will use.

Run: `git diff --stat main...HEAD` — the changed files match this plan, and `.claude/settings.local.json`, `docs/pi-workflow-tooling.md` and `no-width.png` are not included.

---

## Not doing

- Everything in the spec's "Out of scope for v1" section.
- Changing `lopress new` or the scaffolded `.gitignore`.
- Fixing the #63 Windows test hang.
- Flushing on debug `/close`.
- Opening a PR. That is a separate decision once the work is verified.

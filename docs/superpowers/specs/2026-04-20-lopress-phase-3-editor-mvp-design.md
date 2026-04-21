# Lopress Phase 3 — Editor MVP

Addendum to [`2026-04-18-lopress-design.md`](./2026-04-18-lopress-design.md) and
[`2026-04-19-lopress-phase-2-watch-serve-design.md`](./2026-04-19-lopress-phase-2-watch-serve-design.md).
This doc scopes the first slice of the egui editor, intentionally narrow so the
edit-save-rebuild loop is usable end-to-end.

## 1. Scope

One deliverable: a GUI that opens a workspace, edits a post's text blocks,
saves on debounce, and relies on the phase-2 watch/build/serve pipeline to
rebuild and live-reload an external browser.

Concretely, phase 3 adds:

1. **`lopress-editor` crate** — egui UI: welcome screen, post switcher sidebar,
   block editor (paragraph + heading only), inspector for front-matter.
2. **`lopress-gui-host` crate** — process-scoped glue that owns the workspace
   session, wires the watcher, build, and serve machinery, and exposes a small
   facade the editor crate calls.
3. **`src/main.rs` dispatch change** — running `lopress` with no args opens the
   GUI on the welcome screen; `lopress <path>` opens the GUI with that
   workspace preloaded; explicit subcommands (`build`, `new`, `serve`) keep
   their current behaviour.

All other editor features in §7 of the original design are explicitly deferred
(see §8 below).

## 2. Crate layout and dependency direction

```
crates/
  lopress-editor/     # egui GUI (depends on lopress-core, lopress-gui-host)
  lopress-gui-host/   # session + IO glue (depends on lopress-build,
                      # lopress-watch, lopress-serve, lopress-core)
src/main.rs           # CLI dispatch; launches the editor for no-args / path
```

- `lopress-editor` does **not** depend on `lopress-build`, `lopress-watch`,
  `lopress-serve`, or on `std::fs` for anything content-related. It talks to
  the host through the facade below.
- `lopress-gui-host` does **not** depend on `eframe` or `egui`. It is a plain
  library usable from a non-GUI context (this is what keeps integration tests
  cheap).

### 2.1 Host facade (stable surface between crates)

```rust
pub struct Session { /* opaque */ }

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,            // derived from lopress.toml [site].title
    pub posts: Vec<DocumentRef>, // src/posts/*.md
    pub pages: Vec<DocumentRef>, // src/pages/*.md
}

#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,           // absolute
    pub title: String,           // from front-matter, falls back to filename
    pub is_draft: bool,
    pub has_parse_error: bool,
}

#[derive(Debug, Clone)]
pub enum BuildStatus {
    Idle,
    Building,
    Ok { pages_rendered: usize, pages_skipped: usize, duration_ms: u64 },
    Failed { message: String },
}

#[derive(Debug, Clone)]
pub enum ServeStatus {
    Unavailable { reason: String },
    Listening { url: String },   // e.g. "http://127.0.0.1:8080"
}

pub enum LoadError {
    Io(io::Error),
    Parse { raw: String, diagnostic: ParseDiagnostic },
}

impl Session {
    pub fn open(workspace: &Path) -> Result<Session, OpenError>;

    /// Current workspace snapshot. The host refreshes this snapshot whenever
    /// the watcher reports a change under `src/posts/` or `src/pages/` — the
    /// editor reads `workspace()` each frame and picks up new/deleted files
    /// without explicit rescan.
    pub fn workspace(&self) -> &WorkspaceSummary;

    pub fn load_document(&self, path: &Path) -> Result<LoadedDocument, LoadError>;
    pub fn save(&self, doc: &LoadedDocument) -> Result<(), SaveError>;

    pub fn build_status(&self) -> BuildStatus;
    pub fn serve_status(&self) -> ServeStatus;
    pub fn preview_url_for(&self, doc: &DocumentRef) -> Option<String>;

    pub fn close(self); // blocks for final flush + graceful serve shutdown
}
```

`LoadedDocument` owns `front_matter: FrontMatter`, `blocks: Vec<Block>`, a
`path`, and bookkeeping (`dirty: bool`, `dirty_at: Option<Instant>`,
`last_written: Option<SystemTime>`). The editor mutates it directly.
Debounce timing lives in the editor, not the host (see §6.2); the host only
exposes the synchronous `save` primitive. `LoadError::Parse` carries the
raw file bytes so the editor can render the read-only fallback view without
re-reading the file itself.

## 3. CLI entry

`src/main.rs` dispatches as follows:

| Invocation                 | Behaviour                                              |
|----------------------------|--------------------------------------------------------|
| `lopress`                  | Launch GUI → welcome screen.                           |
| `lopress <path>`           | Launch GUI → editing mode at `<path>`.                 |
| `lopress build <workspace>`| Existing behaviour (CLI build).                        |
| `lopress new <name> ...`   | Existing behaviour (scaffold workspace).               |
| `lopress serve <workspace>`| Existing behaviour (CLI dev server).                   |

Argument parsing: if the first non-flag argument matches a known subcommand,
route to the existing CLI; otherwise treat it as a workspace path and launch
the GUI. A single lone argument `--help` or `-h` prints the CLI help (same as
today). This rule means the "open with" OS integration (passing a folder as
argv[1]) works without a subcommand.

## 4. GUI structure

### 4.1 Top-level states

1. **Welcome** — shown when launched with no workspace or after Close
   Workspace. Buttons: *Open Workspace…* (directory picker via `rfd`) and a
   list of up to five *Recent* workspaces. *Quit* exits.
2. **Editing** — shown when a workspace is loaded.

Recent workspaces persist to `<config-dir>/lopress/recents.json` where
`<config-dir>` is resolved via the `directories` crate (XDG on Linux,
`%APPDATA%` on Windows, `~/Library/Application Support` on macOS). Entries
that no longer exist on disk are pruned on next load.

### 4.2 Editing layout

Three regions plus menu and status footer. No embedded preview pane in
phase 3.

```
+--- Menu: File -----------------------------------------------+
| [Open Workspace…] [Save] [Close Workspace] [Quit]            |
+-------------------+------------------------------------------+
| Posts sidebar     | Block editor                             |
| ───────────────   |                                          |
| posts/            | H1 My first post                         |
|  • hello          | ¶ intro paragraph                        |
|  • rust-notes     | H2 section                               |
| pages/            | ¶ body                                   |
|  • about          |                                          |
|                   |        [+ Add block]                     |
| [Preview URL ↗]   |                                          |
+-------------------+--------------------+---------------------+
                                         | Inspector           |
                                         | title:      [____]  |
                                         | slug:       [____]  |
                                         | date:       [____]  |
                                         | draft:      [ ]     |
                                         | description:[____]  |
                                         +---------------------+
+--- Status footer --------------------------------------------+
| Built 14 pages in 42 ms · saved · http://127.0.0.1:8080      |
+--------------------------------------------------------------+
```

- Left: `egui::SidePanel::left`, ~220 px, resizable.
- Right: `egui::SidePanel::right`, ~260 px, collapsible.
- Center: `egui::CentralPanel` — the block editor.
- Top: menu bar via `egui::TopBottomPanel::top`.
- Bottom: status footer via `egui::TopBottomPanel::bottom`.

**Window title:** `<site-title> — lopress`, prefixed with `• ` when any
document is dirty.

### 4.3 Posts sidebar

Flat list with two sub-headings (`posts/`, `pages/`). Each entry shows the
front-matter title, a small draft chip if `draft: true`, and a warning icon if
`has_parse_error`. Click selects the document. There is no search, no
multi-select, no context menu in phase 3.

"Preview URL" under the list is a button whose behaviour depends on
`ServeStatus`:

- `Listening { url }` with a document selected → opens
  `{url}/posts/<slug>/` (or `/<slug>/` for pages) via the `open` crate.
- `Listening { url }` with no document selected → opens `{url}/`.
- `Unavailable { reason }` → button is disabled; hover shows `reason`.

### 4.4 Inspector

Single collapsible section titled **Post** with form fields:

| Field         | Widget                                    |
|---------------|-------------------------------------------|
| `title`       | single-line `TextEdit`                    |
| `slug`        | single-line `TextEdit` (validated as slug)|
| `date`        | single-line `TextEdit` (YYYY-MM-DD)       |
| `draft`       | checkbox                                  |
| `description` | multi-line `TextEdit`, 3 rows             |

Slug validation is non-blocking: invalid chars get a red hint but do not
prevent saving. Date field parses lazily on save; an unparseable value leaves
the existing date untouched and surfaces a toast.

### 4.5 Status footer

One line, three regions:

- **Left**: last build result. `BuildStatus::Building` animates a spinner;
  `Ok` shows `Built N pages in Xms` (dimmed if skipped == total); `Failed`
  shows `Build failed: <first-error>` in red, clickable for the full error
  message in a scrollable popover.
- **Middle**: current document's save state — `saved` (dirty == false),
  `unsaved changes` (dirty == true), or `save failed: <reason>` (red, when
  `last_save_error` is set).
- **Right**: serve URL, click to copy.

## 5. Block editor

### 5.1 Editable block types

Paragraph and heading (H1–H6). Everything else from `lopress-core`'s block set
(list, quote, code fence, image, link, horizontal rule, any `lopress:*`
plugin block) renders as a **read-only opaque placeholder card**:

```
┌──────────────────────────────────────────┐
│ [list]                                   │
│ - first item                             │
│ - second item                            │
└──────────────────────────────────────────┘
```

The card shows the block's type name and the raw serialized markdown of the
block in a mono font. The placeholder does not capture keyboard focus. Delete
removes the whole block (including its children for containers). No edits to
its contents are possible in phase 3; the goal is round-trip safety, not
editability.

### 5.2 Per-block controls

On focus or hover, each editable block shows:

- A **type dropdown** reading `¶`, `H1`, `H2`, …, `H6`. Selecting a new value
  mutates the block's type in place, preserving text.
- A small **delete** button (×). Deleting the last block replaces it with a
  fresh empty paragraph so the editor always has at least one block.

Opaque placeholder blocks get only the delete button.

### 5.3 Keyboard

- **Enter** in a paragraph or heading → split at the caret. The left half stays
  in the current block; the right half becomes a new block of the **same
  type** inserted after.
- **Backspace** at offset 0 of any editable block → merge with the previous
  block. Text is appended; the incoming block's type wins. No-op if the
  current block is the first.
- Everything else inside a block: egui `TextEdit` defaults.
- Crossing block boundaries with arrow keys is **not** supported in phase 3;
  click to move between blocks.

### 5.4 "+ Add block" button

Sits below the last block. Clicking appends an empty paragraph and moves
focus into it. No menu, no block-type choice at add time; change type via the
new block's dropdown.

### 5.5 Text rendering inside a block

Plain `egui::TextEdit` with no inline formatting. Markdown characters
(`**bold**`, `*italic*`, `` `code` ``, `[label](url)`) display literally. The
external browser preview is where rendered output appears. Inline WYSIWYG is
a later-phase concern.

### 5.6 Pure editing operations

All mutations go through pure functions in `lopress-editor::ops`:

```rust
pub fn split_block_at_caret(blocks: &mut Vec<Block>, idx: usize, offset: usize);
pub fn merge_with_previous(blocks: &mut Vec<Block>, idx: usize);
pub fn change_block_type(blocks: &mut Vec<Block>, idx: usize, new_type: BlockType);
pub fn add_paragraph_at_end(blocks: &mut Vec<Block>);
pub fn delete_block(blocks: &mut Vec<Block>, idx: usize);
```

These operate on `lopress-core` types with no egui dependency, so they unit
test cleanly.

## 6. Save and rebuild loop

### 6.1 In-memory state

```rust
pub struct LoadedDocument {
    pub path: PathBuf,
    pub front_matter: FrontMatter,
    pub blocks: Vec<Block>,
    pub dirty: bool,
    pub dirty_at: Option<Instant>,
    pub last_written: Option<SystemTime>,
    pub last_save_error: Option<String>,
}
```

Exactly one document is "current". Switching selection in the sidebar
flushes any pending save on the outgoing document **synchronously** before
loading the new one.

### 6.2 Debounce

Debounce lives on the UI thread, not in a background thread. Each edit sets
`doc.dirty = true`, stamps `doc.dirty_at = Some(Instant::now())`, and calls
`ctx.request_repaint_after(Duration::from_millis(500))`. On each subsequent
frame the editor checks: if `doc.dirty && now - dirty_at >= 500 ms`, call
`Session::save(&doc)` synchronously and clear `dirty`. Serialization for a
single post is cheap (string assembly), the write is atomic (`tmp + fsync
+ rename`, performed inside `save`), and the UI thread blocks for the
microseconds it takes.

Forced flush (File → Save / Ctrl-S, or switching documents) calls
`Session::save` immediately, bypassing the timer check.

### 6.3 Rebuild

The write reaches disk. The `lopress-watch` instance already running inside
`Session` sees the event. The host calls
`lopress_build::build(&workspace_root)`, updates `BuildStatus`, and — because
the serve machinery is in the same process — broadcasts the SSE reload
event. Any browser tab open to the preview URL reloads.

### 6.4 Serve lifecycle

`Session::open`:

1. Validate workspace (`lopress.toml` parses, `src/` exists). Failure returns
   `OpenError`; the GUI shows an error on the welcome screen.
2. Spawn the watch + serve stack. Port preference: 8080 first; on
   `AddrInUse`, bind to port 0 and use the assigned port. On any other bind
   failure, `ServeStatus = Unavailable { reason }` and the session still
   opens — editing and saving still work.
3. Run the initial full build. The build runs on `Session::open`'s calling
   thread so we don't present an editing UI against a workspace that never
   built.
4. Scan `src/posts/` and `src/pages/` to populate `WorkspaceSummary.posts`
   and `.pages`.

`Session::close`:

1. Flush any dirty document.
2. Shut down the HTTP listener, drop the watcher.
3. Return.

Ctrl-C at the terminal triggers the same path (reusing whatever signal
handling `lopress-serve` already installs for its CLI subcommand).

### 6.5 External-edit handling

MVP does **not** detect or merge external edits. If a watcher event modifies
the currently-open document's path between load and save, the next flush
stomps whatever the external tool wrote and the footer logs a warning
("External changes to `<file>` overwritten on save"). A reload prompt and
three-way merge are phase 4+.

## 7. Error handling

Principle (carried from the original design §8): one failing part does not
block the rest.

| Condition                                         | Behaviour                                                                 |
|---------------------------------------------------|---------------------------------------------------------------------------|
| Invalid workspace (`lopress.toml` missing/parse)  | Welcome screen with error banner; workspace not loaded.                   |
| Serve bind fails on both 8080 and ephemeral       | Editing mode loads; preview button disabled; tooltip shows reason.        |
| Initial build fails                               | Editing mode loads; footer shows failure; saving retriggers a build.      |
| Post parse error                                  | Sidebar shows warning icon; clicking opens read-only raw view with diagnostic; save is disabled for that document. |
| Write failure on save                             | Toast with OS error; `dirty` stays true; next edit or manual Save retries.|
| UI thread panic                                   | `eframe` default handler: stderr dump + exit. No custom supervisor in MVP.|

## 8. Out of scope (deferred to phase 4+)

- Embedded webview preview pane (wry or otherwise).
- Editable block types beyond paragraph and heading (list, quote, code fence,
  image, link, horizontal rule, plugin declarative blocks).
- Block inserter UI (`/`), drag handles, Alt+Up/Down reordering.
- Undo/redo.
- Ctrl-P / Cmd-P fuzzy post switcher.
- Arrow-key cursor traversal across block boundaries.
- External-edit detection and reload prompt.
- New Post / New Page / New Site menu entries.
- Site Settings dialog.
- Plugins manager dialog.
- Image picker.
- Scroll-position preservation in the (deferred) preview pane.
- WASM block renderers and JS editor-UI escape hatches.
- Platform-native menu bar on macOS/Windows (egui's in-window menu applies
  everywhere in phase 3).
- Installers, signed releases, auto-update.

## 9. Testing

Mirrors phases 1 and 2: logic in pure library code gets heavy coverage; egui
rendering is manual.

- **`lopress-editor` unit tests** — `ops::split_block_at_caret`,
  `merge_with_previous`, `change_block_type`, `add_paragraph_at_end`,
  `delete_block` at edge cases (start/middle/end caret, empty blocks, merging
  across types, deleting the last block).
- **`lopress-editor` round-trip tests** — fixture `.md` files mixing editable
  and opaque blocks; load → touch one paragraph → save → assert the
  serialized bytes match a golden file (opaque blocks untouched).
- **`lopress-editor` session state machine** — welcome ↔ editing transitions,
  invalid workspace keeps welcome state.
- **`lopress-gui-host` integration tests** — `tempfile` workspace:
  - Open workspace → serve listens on ephemeral port → `GET /posts/<slug>/`
    returns built HTML.
  - Mark a document dirty → wait 600 ms → assert file on disk changed and
    `BuildStatus` cycles `Idle → Building → Ok`.
  - Close workspace → listener dropped, port freed.
- **No automated egui tests.** A manual smoke checklist lives in the phase 3
  plan's final task: open workspace, select a post, type, wait, browser tab
  reloads.

## 10. Dependencies added

Workspace `Cargo.toml` gains:

- `eframe` — egui window host.
- `rfd` — native file dialogs.
- `open` — cross-platform "open URL/file".
- `directories` — OS-appropriate config directory.

Exact versions are pinned during planning against whatever is current at
lock-in.

These are added to `[workspace.dependencies]` and pulled into
`lopress-editor` / `lopress-gui-host` as needed. No system libraries beyond
what egui already transitively requires.

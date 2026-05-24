# Stage 3 — `ui/mod.rs` decomposition

> **For the implementer (qwen):** execute this plan task-by-task in order.
> **Prerequisite:** Stages 0, 1, and 2 (plus the two follow-on fixes) must
> be fully committed on this branch before Task 1 starts. Verify with
> `git log --oneline | head -25` — you should see commits up through
> `327161b test(editor): add round-trip tests for all 8 ChangeType
> conversion directions`. If they aren't there, stop and report.
>
> This stage is a **pure refactor** — no behaviour changes. The whole test
> suite must stay green after every commit. If a commit causes any test
> to fail, the move was wrong; stop and report rather than rewriting the
> test.
>
> Commit per task. After every commit run `cargo test --workspace` and
> `cargo check --workspace`. Use heredoc commit messages with the
> `Co-Authored-By: Qwen <noreply@anthropic.com>` trailer.

**Goal:** Break the ~666-line `crates/lopress-editor/src/ui/mod.rs` (with
a ~400-line `editing_view` function) into small sibling modules under
`ui/editing/`, each owning one clear responsibility. `editing_view`
becomes ~80 lines that assembles the pieces. No behaviour changes; tests
prove correctness.

**Architecture:** Module-per-responsibility under a new `ui/editing/`
directory. Each module exports plain functions that take the signals they
need as arguments and return a closure or start an effect. Signals stay
owned by `editing_view`'s body — no shared state struct. Stage 2's
private `defer_focus` duplicate in `ui/blocks/code_editor.rs` is unified
with the list block's via the new `ui/editing/focus.rs::defer_focus` as
part of this stage.

**Tech stack:** Rust 2021, Floem reactive UI. Verification:
`cargo test --workspace`, `cargo check --workspace`.

---

## File structure map

### Files to create

| File | Lines | Change |
|---|---|---|
| `crates/lopress-editor/src/ui/editing/mod.rs` | ~80 | `editing_view` assembles the pieces; re-exports from sibling modules |
| `crates/lopress-editor/src/ui/editing/focus.rs` | ~50 | `focus_block_for`, `focus_after_apply`, `defer_focus` |
| `crates/lopress-editor/src/ui/editing/pane_key.rs` | ~30 | `KindTag`, `kind_tag`, `build_pane_key` |
| `crates/lopress-editor/src/ui/editing/action_sink.rs` | ~70 | `build_action_sink` |
| `crates/lopress-editor/src/ui/editing/undo_redo.rs` | ~60 | `build_undo`, `build_redo` |
| `crates/lopress-editor/src/ui/editing/save_pipeline.rs` | ~60 | `SavePipeline` struct + `start_save_pipeline` |
| `crates/lopress-editor/src/ui/editing/new_doc.rs` | ~70 | `DocKind`, `make_new_doc_action` |
| `crates/lopress-editor/src/ui/editing/ctrl_wire.rs` | ~45 | `wire_ctrl` (gated on `#[cfg(debug_assertions)]`) |

### Files to modify

| File | Line(s) | Change |
|---|---|---|
| `crates/lopress-editor/src/ui/mod.rs` | ~45-666 | Strip `editing_view` and all extracted items; retain `root_view`, `StateTag`, `MAX_RECENTS` (~80 lines) |
| `crates/lopress-editor/src/ui/blocks/list.rs` | 115-119 | Delete private `defer_focus`; add `use crate::ui::editing::focus::defer_focus;` |
| `crates/lopress-editor/src/ui/blocks/code_editor.rs` | 10-11, 234-238 | Update doc comment (remove defer_focus note); delete private `defer_focus`; add `use crate::ui::editing::focus::defer_focus;` |

### Files NOT to modify

- `crates/lopress-editor/src/ui/blocks/*` — except for the `defer_focus` unification callouts in Task 1.
- `crates/lopress-editor/src/ui/editor_pane.rs`, `footer.rs`, `inspector.rs`, `sidebar.rs`, `slash_menu.rs`, `toolbar.rs`, `welcome.rs` — untouched.
- `crates/lopress-editor/src/model/*`, `actions.rs`, `state.rs`, `settings.rs` — untouched.
- `Cargo.toml` or any dependency — untouched.

---

## Conventions

- **Test framework:** Built-in Rust `#[test]`. The bar is the existing test
  suite staying green. No new tests required for the move itself — a pure
  refactor leaves observable behaviour unchanged.
- **Run commands:** `cargo test --workspace` between every task;
  `cargo check --workspace` for fast incremental sanity.
- **Commit-message style:** Conventional commits, heredoc form, `Co-
  Authored-By: Qwen <noreply@anthropic.com>` trailer. Use
  `refactor(editor):` for the moves.

---

## Task 1: Create `ui/editing/` skeleton + extract `focus.rs` + unify `defer_focus`

**Why first:** This task creates the directory, the `editing/mod.rs` skeleton,
the `focus.rs` module, and unifies the `defer_focus` duplication in `list.rs`
and `code_editor.rs`. It is the only task that touches files outside
`ui/mod.rs` and `ui/editing/` (the two callers of `defer_focus`).

### Step 1.1: Verify baseline — all tests pass

```bash
cd C:\Users\corpo\Documents\projects\lopress
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 1.2: Create `ui/editing/` directory and `editing/mod.rs` skeleton

Create `crates/lopress-editor/src/ui/editing/mod.rs`:

```rust
//! Editing-mode view: assembles the pieces built by sibling modules.
//!
//! Each sibling module (`focus`, `pane_key`, `action_sink`, `undo_redo`,
//! `save_pipeline`, `new_doc`, `ctrl_wire`) owns a responsibility and
//! exports a free function that `editing_view` calls.

pub mod focus;
```

> Each subsequent task adds its own `pub mod <name>;` line to this file as
> the module is created. Declaring all seven modules upfront would fail
> `cargo check` until the last task lands.

### Step 1.3: Create `focus.rs` with `focus_block_for`, `focus_after_apply`, and `defer_focus`

Create `crates/lopress-editor/src/ui/editing/focus.rs`:

```rust
//! Focus resolution helpers for the editing view.
//!
//! `focus_block_for` derives the block to focus *before* an action is
//! applied (the target block). `focus_after_apply` resolves the block to
//! focus *after* the action is applied (the surviving block — in most
//! cases the same as the pre-focus, but `MergeWithPrev` deletes its
//! target and focus must land on the predecessor).
//!
//! `defer_focus` schedules a focus update on the next event-loop tick
//! rather than immediately, avoiding Floem's "set focus while already
//! processing focus" race.

use crate::actions::BlockAction;
use crate::model::types::{BlockId, EditorDoc};
use floem::reactive::{RwSignal, SignalUpdate};
use std::time::Duration;

/// The block a just-applied undo/redo action should restore focus to.
pub fn focus_block_for(action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
        BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
        BlockAction::Delete { .. } | BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditBlockBody { block_id, .. } => Some(*block_id),
    }
}

/// Which block should hold focus after `action` is applied to `doc`
/// (`doc` is the state *before* the apply). Most actions keep their target
/// block alive, so `focus_block_for` suffices — but `MergeWithPrev` deletes
/// its target (folds it into the predecessor), so focus must land on the
/// surviving predecessor, looked up here while the target still exists.
pub fn focus_after_apply(doc: Option<&EditorDoc>, action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::MergeWithPrev { block_id } => {
            let d = doc?;
            let i = d.blocks.iter().position(|b| b.id == *block_id)?;
            i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
        }
        _ => focus_block_for(action),
    }
}

/// Set `focus_target` on the next event-loop tick rather than immediately.
///
/// Used in the action sink, undo/redo builders, and the list/code widgets
/// to avoid Floem's "set focus while already processing focus" race.
pub fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
    floem::action::exec_after(Duration::from_millis(0), move |_| {
        focus_target.set(Some(target_id));
    });
}
```

### Step 1.4: Update `list.rs` — import shared `defer_focus`, delete private copy

In `crates/lopress-editor/src/ui/blocks/list.rs`:

Find the import block (around line 11-20):
```rust
use crate::model::types::{BlockBody, BlockId, EditorDoc, InlineRun, ListItem};
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
    StructuralKey,
};
```

Add after it:
```rust
use crate::ui::editing::focus::defer_focus;
```

Find (lines 115-119):
```rust
fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
    floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
        focus_target.set(Some(target_id));
    });
}
```

Delete this entire block (the function body). The import from Step 1.4a
now provides `defer_focus`.

### Step 1.5: Update `code_editor.rs` — import shared `defer_focus`, delete private copy

In `crates/lopress-editor/src/ui/blocks/code_editor.rs`:

Update the doc comment (lines 10-11). The old comment says:
```
//! `defer_focus` is a private duplicate of `list.rs`'s version. It will be
//! unified with the shared `focus::defer_focus` in Stage 3.
```

Remove those two lines from the doc comment (they are no longer true).

Find the import block (lines 13-20):
```rust
use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
    StructuralKey,
};
use crate::ui::blocks::paragraph::MONO_FAMILY;
```

Add after it:
```rust
use crate::ui::editing::focus::defer_focus;
```

Find (lines 234-238):
```rust
fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
    floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
        focus_target.set(Some(target_id));
    });
}
```

Delete this entire block.

### Step 1.6: Extract `focus_block_for` and `focus_after_apply` from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 137-164):

```rust
fn focus_block_for(action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
        BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
        BlockAction::Delete { .. } | BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditBlockBody { block_id, .. } => Some(*block_id),
    }
}

/// Which block should hold focus after `action` is applied to `doc`
/// (`doc` is the state *before* the apply). Most actions keep their target
/// block alive, so `focus_block_for` suffices — but `MergeWithPrev` deletes
/// its target (folds it into the predecessor), so focus must land on the
/// surviving predecessor, looked up here while the target still exists.
fn focus_after_apply(doc: Option<&EditorDoc>, action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::MergeWithPrev { block_id } => {
            let d = doc?;
            let i = d.blocks.iter().position(|b| b.id == *block_id)?;
            i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
        }
        _ => focus_block_for(action),
    }
}
```

Delete these two functions. In `editing_view`, replace the two calls:
- `focus_block_for(&action)` → `focus::focus_block_for(&action)`
- `focus_after_apply(m.as_ref(), &action)` → `focus::focus_after_apply(m.as_ref(), &action)`

In `ui/mod.rs`, add a `pub mod editing;` declaration. The existing module
block looks like:
```rust
pub mod blocks;
pub mod dnd;
pub mod editor_pane;
pub mod footer;
pub mod inspector;

pub mod sidebar;
pub mod slash_menu;
pub mod toolbar;
pub mod welcome;
```

Insert `pub mod editing;` so it reads:
```rust
pub mod blocks;
pub mod dnd;
pub mod editing;
pub mod editor_pane;
pub mod footer;
pub mod inspector;

pub mod sidebar;
pub mod slash_menu;
pub mod toolbar;
pub mod welcome;
```

Then add the `use crate::ui::editing::focus;` import at the top of the
file (after the existing `use` block). Subsequent tasks reuse this module
declaration; each adds its own `use crate::ui::editing::<name>;` line.

### Step 1.7: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass. The `defer_focus` unification must not change behaviour.

### Step 1.8: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 1.9: Commit

```bash
git add crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/editing/focus.rs crates/lopress-editor/src/ui/mod.rs crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/code_editor.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract focus helpers to ui/editing/focus.rs and unify defer_focus

Moves focus_block_for and focus_after_apply from ui/mod.rs into the new
ui/editing/focus.rs module. Adds defer_focus to the same module and
unifies the previously duplicated private copies in list.rs and
code_editor.rs — both callers now import from crate::ui::editing::focus.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Extract `pane_key.rs`

**Why:** `KindTag`, `kind_tag`, and the `pane_key` closure are a self-contained
unit — they key the `dyn_container` that drives editor-pane rebuilds. Moving
them out shrinks `editing_view` by ~20 lines.

### Step 2.1: Create `pane_key.rs`

Create `crates/lopress-editor/src/ui/editing/pane_key.rs`:

```rust
//! Pane-rebuild key: a lightweight discriminant for `BlockKind` and per-block
//! metadata used to key the `dyn_container` in `editing_view`.
//!
//! Within-block text edits (which fire `EditInline` → `current_doc.update`)
//! must NOT tear down the per-block widgets, otherwise focus is lost every
//! time the user commits runs. The per-block widgets own their own
//! `runs_sig` reactive copies; structural changes (split, delete, insert,
//! reorder) change the id list and trigger a rebuild. Block-kind changes
//! (toolbar P/H1/H2/Code/UL/OL buttons) do too — discriminant comparison
//! covers `Heading(1)` vs `Heading(2)`, `List{ordered:false}` vs
//! `ordered:true`, etc.

use crate::model::types::{BlockId, BlockKind, EditorDoc};
use floem::reactive::{RwSignal, SignalWith};

/// Compact equality tag for `BlockKind` used by the editor-pane rebuild key.
/// `Eq` is fine; this is just a discriminator (Heading(1) vs Heading(2),
/// List{ordered:false} vs ordered:true, etc.) so we trigger a pane rebuild
/// when the toolbar's type buttons swap a block's kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum KindTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Opaque,
}

pub fn kind_tag(k: &BlockKind) -> KindTag {
    match k {
        BlockKind::Paragraph => KindTag::Paragraph,
        BlockKind::Heading(level) => KindTag::Heading(*level),
        BlockKind::Code { .. } => KindTag::Code,
        BlockKind::List { ordered } => KindTag::List { ordered: *ordered },
        BlockKind::Opaque { .. } => KindTag::Opaque,
    }
}

/// Build the closure that keys the editor-pane `dyn_container`.
///
/// Returns a closure that, when called, produces the current block id
/// sequence + per-block kind tag + plugin presence. This closure is passed
/// as the key function to `dyn_container`.
pub fn build_pane_key(current_doc: RwSignal<Option<EditorDoc>>) -> impl Fn() -> Option<Vec<(BlockId, KindTag, bool)>> + Copy {
    move || {
        current_doc.with(|d| {
            d.as_ref().map(|d| {
                d.blocks
                    .iter()
                    .map(|b| (b.id, kind_tag(&b.kind), b.plugin.is_some()))
                    .collect::<Vec<_>>()
            })
        })
    }
}
```

### Step 2.2: Extract from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 580-600):

```rust
#[derive(Clone, PartialEq)]
enum StateTag {
    Welcome,
    Editing,
}

/// Compact equality tag for `BlockKind` used by the editor-pane rebuild key.
/// `Eq` is fine; this is just a discriminator (Heading(1) vs Heading(2),
/// List{ordered:false} vs ordered:true, etc.) so we trigger a pane rebuild
/// when the toolbar's type buttons swap a block's kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum KindTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Opaque,
}

fn kind_tag(k: &crate::model::types::BlockKind) -> KindTag {
    use crate::model::types::BlockKind;
    match k {
        BlockKind::Paragraph => KindTag::Paragraph,
        BlockKind::Heading(level) => KindTag::Heading(*level),
        BlockKind::Code { .. } => KindTag::Code,
        BlockKind::List { ordered } => KindTag::List { ordered: *ordered },
        BlockKind::Opaque { .. } => KindTag::Opaque,
    }
}
```

Delete the `KindTag` enum and `kind_tag` function (lines 580-597). Keep
`StateTag` — it stays in `mod.rs`.

Find (lines 396-405):
```rust
    let pane_key = move || {
        current_doc.with(|d| {
            d.as_ref().map(|d| {
                d.blocks
                    .iter()
                    .map(|b| (b.id, kind_tag(&b.kind), b.plugin.is_some()))
                    .collect::<Vec<_>>()
            })
        })
    };
```

Replace with:
```rust
    let pane_key = pane_key::build_pane_key(current_doc);
```

Add `use crate::ui::editing::pane_key;` to the top of `mod.rs` (after the
existing `use` block).

### Step 2.3: Register the new module in `editing/mod.rs`

Open `crates/lopress-editor/src/ui/editing/mod.rs` and add `pub mod pane_key;`
under the existing `pub mod focus;` line. The file should now read:

```rust
pub mod focus;
pub mod pane_key;
```

### Step 2.4: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 2.5: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 2.6: Commit

```bash
git add crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/editing/pane_key.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract KindTag, kind_tag, and pane_key closure to pane_key.rs

Moves the pane-rebuild key types and closure from ui/mod.rs into
ui/editing/pane_key.rs. editing_view now calls pane_key::build_pane_key
instead of inline.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Extract `new_doc.rs`

**Why:** `DocKind` and `make_new_doc_action` are a self-contained unit that
the sidebar uses. Moving them out is straightforward — no new dependencies,
just a copy-paste with a function wrapper.

### Step 3.1: Create `new_doc.rs`

Create `crates/lopress-editor/src/ui/editing/new_doc.rs`:

```rust
//! "+ New post" / "+ New page" sidebar actions.

use crate::model::types::EditorDoc;
use crate::state::EditingState;
use crate::ui::sidebar::{new_doc_stub, unique_untitled_path};
use floem::reactive::{RwSignal, SignalUpdate, SignalWith};
use lopress_gui_host::{DocumentRef, WorkspaceSummary};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Whether a "+ New …" sidebar action targets the Posts or Pages directory.
#[derive(Clone, Copy)]
pub enum DocKind {
    Post,
    Page,
}

impl DocKind {
    pub fn default_title(self) -> &'static str {
        match self {
            DocKind::Post => "New Post",
            DocKind::Page => "New Page",
        }
    }
}

/// Build the closure the sidebar invokes for "+ New post" / "+ New page".
///
/// The closure: picks a fresh `untitled-N.md` filename, writes the stub
/// markdown, rescans the workspace, then opens the new doc through
/// `EditingState::open_document` so the editor pane and current_path signal
/// stay in sync with the sidebar.
pub fn make_new_doc_action(
    editing: Rc<RefCell<Option<EditingState>>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    kind: DocKind,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        let dir = match kind {
            DocKind::Post => state.session.posts_dir(),
            DocKind::Page => state.session.pages_dir(),
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("failed to create {}: {e}", dir.display());
            return;
        }
        let path = unique_untitled_path(&dir);
        if let Err(e) = std::fs::write(&path, new_doc_stub(kind.default_title())) {
            eprintln!("failed to write {}: {e}", path.display());
            return;
        }

        let summary = state.session.rescan();
        let doc_ref = summary
            .posts
            .iter()
            .chain(summary.pages.iter())
            .find(|d| d.path == path)
            .cloned()
            .unwrap_or_else(|| DocumentRef {
                path: path.clone(),
                title: kind.default_title().to_string(),
                is_draft: true,
                has_parse_error: false,
            });

        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        workspace_signal.set(summary);
    })
}
```

### Step 3.2: Extract from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 601-666):

```rust
/// Whether a "+ New …" sidebar action targets the Posts or Pages directory.
#[derive(Clone, Copy)]
enum DocKind {
    Post,
    Page,
}

impl DocKind {
    fn default_title(self) -> &'static str {
        match self {
            DocKind::Post => "New Post",
            DocKind::Page => "New Page",
        }
    }
}

/// Build the closure the sidebar invokes for "+ New post" / "+ New page".
///
/// The closure: picks a fresh `untitled-N.md` filename, writes the stub
/// markdown, rescans the workspace, then opens the new doc through
/// `EditingState::open_document` so the editor pane and current_path signal
/// stay in sync with the sidebar.
fn make_new_doc_action(
    editing: Rc<RefCell<Option<EditingState>>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    kind: DocKind,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        let dir = match kind {
            DocKind::Post => state.session.posts_dir(),
            DocKind::Page => state.session.pages_dir(),
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("failed to create {}: {e}", dir.display());
            return;
        }
        let path = unique_untitled_path(&dir);
        if let Err(e) = std::fs::write(&path, new_doc_stub(kind.default_title())) {
            eprintln!("failed to write {}: {e}", path.display());
            return;
        }

        let summary = state.session.rescan();
        let doc_ref = summary
            .posts
            .iter()
            .chain(summary.pages.iter())
            .find(|d| d.path == path)
            .cloned()
            .unwrap_or_else(|| DocumentRef {
                path: path.clone(),
                title: kind.default_title().to_string(),
                is_draft: true,
                has_parse_error: false,
            });

        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        workspace_signal.set(summary);
    })
}
```

Delete the entire `DocKind` enum, its `impl`, and the `make_new_doc_action`
function (lines 601-666).

In `editing_view`, the calls to `make_new_doc_action` (lines 206-220) need
to be updated to use the module path. The current code:

```rust
    let on_new_post = make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        DocKind::Post,
    );
    let on_new_page = make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        DocKind::Page,
    );
```

Replace with:
```rust
    let on_new_post = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Post,
    );
    let on_new_page = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Page,
    );
```

Add `use crate::ui::editing::new_doc;` to the top of `mod.rs`.

### Step 3.3: Register the new module in `editing/mod.rs`

Append `pub mod new_doc;` to `crates/lopress-editor/src/ui/editing/mod.rs`.
The file should now read:

```rust
pub mod focus;
pub mod pane_key;
pub mod new_doc;
```

### Step 3.4: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 3.5: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 3.6: Commit

```bash
git add crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/editing/new_doc.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract DocKind and make_new_doc_action to new_doc.rs

Moves the DocKind enum and the make_new_doc_action closure factory from
ui/mod.rs into ui/editing/new_doc.rs. editing_view calls
new_doc::make_new_doc_action and new_doc::DocKind directly.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Extract `save_pipeline.rs`

**Why:** The save-debounce signals (`build_status_sig`, `dirty_sig`,
`save_error_sig`, `serve_status_sig`, `dirty_counter`), the `mark_dirty`
closure, the debounce call, and the status-poll spawners form a cohesive
unit. Extracting them into a `SavePipeline` struct + `start_save_pipeline`
function removes ~55 lines from `editing_view`.

### Step 4.1: Create `save_pipeline.rs`

Create `crates/lopress-editor/src/ui/editing/save_pipeline.rs`:

```rust
//! Save pipeline: debounce signals, dirty tracking, status polling, and
//! the debounced save+rebuild closure.
//!
//! `SavePipeline` is a plain bag of signals (no methods that hold state).
//! `start_save_pipeline` bundles the signal creation, starts the debounce
//! timer, and kicks off the build/serve status polls.

use crate::model::types::EditorDoc;
use crate::state::EditingState;
use floem::action::debounce_action;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use lopress_gui_host::{BuildStatus, DocumentRef, ServeStatus};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Bag of save-pipeline signals exposed to `editing_view` for the footer
/// and the debounced save closure.
pub struct SavePipeline {
    pub mark_dirty: Rc<dyn Fn()>,
    pub dirty_sig: RwSignal<bool>,
    pub save_error_sig: RwSignal<Option<String>>,
    pub build_status_sig: RwSignal<BuildStatus>,
    pub serve_status_sig: RwSignal<ServeStatus>,
}

/// Create the save-pipeline signals, start the debounce timer, and kick off
/// the build/serve status polls.
///
/// Returns a `SavePipeline` that `editing_view` passes to the footer and
/// uses for the `on_action` mark_dirty callback.
pub fn start_save_pipeline(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> SavePipeline {
    // ── Save-debounce signals ────────────────────────────────────────
    // `dirty_counter` bumps on every legitimate edit; `debounce_action`
    // watches it and runs the save closure 500 ms after the last bump.
    // `dirty_sig` / `save_error_sig` drive the footer's status display.
    let build_status_sig: RwSignal<BuildStatus> = RwSignal::new(BuildStatus::Idle);
    let dirty_sig: RwSignal<bool> = RwSignal::new(false);
    let save_error_sig: RwSignal<Option<String>> = RwSignal::new(None);
    let dirty_counter: RwSignal<u64> = RwSignal::new(0);

    let mark_dirty: Rc<dyn Fn()> = Rc::new(move || {
        dirty_sig.set(true);
        dirty_counter.update(|n| *n = n.wrapping_add(1));
    });

    // Status polls — read session status and update the signals.
    {
        let editing_for_poll = Rc::clone(&editing);
        let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.build_status())
                .unwrap_or(BuildStatus::Idle)
        });
        crate::ui::footer::start_build_status_poll(session_reader, build_status_sig);
    }

    let serve_status_sig: RwSignal<ServeStatus> = RwSignal::new(ServeStatus::Starting);

    {
        let editing_for_poll = Rc::clone(&editing);
        let serve_reader: Rc<dyn Fn() -> ServeStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.serve_status())
                .unwrap_or(ServeStatus::Starting)
        });
        crate::ui::footer::start_serve_status_poll(serve_reader, serve_status_sig);
    }

    // Debounced save+rebuild. `debounce_action` resets its internal timer on
    // every counter bump and fires the closure 500 ms after the last bump.
    {
        let editing_for_save = Rc::clone(&editing);
        let dc = dirty_counter;
        let ds = dirty_sig;
        let ses = save_error_sig;
        let bs = build_status_sig;
        debounce_action(dc, Duration::from_millis(500), move || {
            let doc = match current_doc.with_untracked(|d| d.clone()) {
                Some(d) => d,
                None => return,
            };
            let result = {
                let guard = editing_for_save.borrow();
                match guard.as_ref() {
                    Some(state) => state.save_doc(&doc),
                    None => return,
                }
            };
            match result {
                Ok(()) => {
                    ds.set(false);
                    ses.set(None);
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
                Err(msg) => {
                    ses.set(Some(msg));
                }
            }
        });
    }

    SavePipeline {
        mark_dirty,
        dirty_sig,
        save_error_sig,
        build_status_sig,
        serve_status_sig,
    }
}
```

### Step 4.2: Extract from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 234-490) — the entire
save-debounce block from the comment through the footer call:

```rust
    // ── Save-debounce signals ────────────────────────────────────────────
    // `dirty_counter` bumps on every legitimate edit; `debounce_action`
    // watches it and runs the save closure 500 ms after the last bump.
    // `dirty_sig` / `save_error_sig` drive the footer's status display.
    let build_status_sig: RwSignal<BuildStatus> = RwSignal::new(BuildStatus::Idle);
    let dirty_sig: RwSignal<bool> = RwSignal::new(false);
    let save_error_sig: RwSignal<Option<String>> = RwSignal::new(None);
    let dirty_counter: RwSignal<u64> = RwSignal::new(0);

    let mark_dirty: Rc<dyn Fn()> = Rc::new(move || {
        dirty_sig.set(true);
        dirty_counter.update(|n| *n = n.wrapping_add(1));
    });

    // Chokepoint: every block-tree mutation routes through here. Pre/post
    // lookups derive the block to focus after structural actions.
    let on_action_mark_dirty = Rc::clone(&mark_dirty);
```

And find (lines 435-490):
```rust
    let serve_status_sig: RwSignal<ServeStatus> = RwSignal::new(ServeStatus::Starting);

    {
        let editing_for_poll = Rc::clone(&editing);
        let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.build_status())
                .unwrap_or(BuildStatus::Idle)
        });
        start_build_status_poll(session_reader, build_status_sig);
    }

    {
        let editing_for_poll = Rc::clone(&editing);
        let serve_reader: Rc<dyn Fn() -> ServeStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.serve_status())
                .unwrap_or(ServeStatus::Starting)
        });
        start_serve_status_poll(serve_reader, serve_status_sig);
    }

    // Debounced save+rebuild. `debounce_action` resets its internal timer on
    // every counter bump and fires the closure 500 ms after the last bump.
    {
        let editing_for_save = Rc::clone(&editing);
        debounce_action(dirty_counter, Duration::from_millis(500), move || {
            let doc = match current_doc.with_untracked(|d| d.clone()) {
                Some(d) => d,
                None => return,
            };
            let result = {
                let guard = editing_for_save.borrow();
                match guard.as_ref() {
                    Some(state) => state.save_doc(&doc),
                    None => return,
                }
            };
            match result {
                Ok(()) => {
                    dirty_sig.set(false);
                    save_error_sig.set(None);
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
                Err(msg) => {
                    save_error_sig.set(Some(msg));
                }
            }
        });
    }

    let footer = footer_view(
        build_status_sig,
        dirty_sig,
        save_error_sig,
        current_doc,
        serve_status_sig,
    );
```

Replace the entire block (lines 234-490) with:

```rust
    // ── Save pipeline ────────────────────────────────────────────────────
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);

    // ── Chokepoint: every block-tree mutation routes through on_action ────
    let on_action_mark_dirty = Rc::clone(&save.mark_dirty);
```

And replace the footer call with:
```rust
    let footer = footer_view(
        save.build_status_sig,
        save.dirty_sig,
        save.save_error_sig,
        current_doc,
        save.serve_status_sig,
    );
```

Add `use crate::ui::editing::save_pipeline;` to the top of `mod.rs`.

Update the `on_action` closure to use `save.mark_dirty` instead of
`on_action_mark_dirty` where it's called (the `on_action_mark_dirty`
variable is now just a clone for the action sink — this is unchanged).

### Step 4.3: Register the new module in `editing/mod.rs`

Append `pub mod save_pipeline;` to `crates/lopress-editor/src/ui/editing/mod.rs`.
The file should now read:

```rust
pub mod focus;
pub mod pane_key;
pub mod new_doc;
pub mod save_pipeline;
```

### Step 4.4: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 4.5: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 4.6: Commit

```bash
git add crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/editing/save_pipeline.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract save pipeline signals and debounce into save_pipeline.rs

Defines SavePipeline (a plain bag of signals) and start_save_pipeline()
that creates the signals, starts the debounce timer, and kicks off the
build/serve status polls. editing_view now calls
save_pipeline::start_save_pipeline and passes the result's signals to
the footer.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Extract `action_sink.rs` and `undo_redo.rs`

**Why:** The `on_action` closure (~70 lines) is the chokepoint for all
block-tree mutations. The `on_undo` and `on_redo` closures (~30 lines each)
are tightly coupled to it (they share the same focus-computation helpers
and the `mark_dirty` callback). Extracting all three into two modules
removes ~130 lines from `editing_view`.

### Step 5.1: Create `action_sink.rs`

Create `crates/lopress-editor/src/ui/editing/action_sink.rs`:

```rust
//! Action sink: the chokepoint for all block-tree mutations.
//!
//! Every `BlockAction` routes through the closure returned by
//! `build_action_sink`. It handles the slash menu toggle, pre/post focus
//! computation, dispatches to `apply`, pushes undo/redo entries, and
//! triggers the dirty flag.

use crate::actions::{apply, BlockAction};
use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::inline_editor::ActionSink;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use std::rc::Rc;
use std::time::Duration;

/// Build the `on_action` closure that every block-tree mutation routes through.
///
/// Parameters:
/// - `current_doc`: the reactive document model.
/// - `focus_target`: signal set by focus resolution after each action.
/// - `slash_menu_open`: signal tracking the open slash-menu block id.
/// - `undo_stack`: the undo/redo stack.
/// - `mark_dirty`: callback to mark the document dirty (triggers save debounce).
pub fn build_action_sink(
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    undo_stack: RwSignal<crate::undo::UndoStack>,
    mark_dirty: Rc<dyn Fn()>,
) -> ActionSink {
    let on_action_mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move |action: BlockAction| {
        let _t = lopress_core::perf::span("editor.on_action");
        if let BlockAction::OpenSlashMenu { block_id } = action {
            slash_menu_open.set(Some(block_id));
            return;
        }
        if slash_menu_open.get_untracked().is_some() {
            slash_menu_open.set(None);
        }

        // Pre-focus must read pre-apply state (the block before the one
        // being merged into its predecessor). Capture it before the apply
        // mutates the doc.
        let pre_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::MergeWithPrev { block_id }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .filter(|&i| i > 0)
                .and_then(|i| d.blocks.get(i - 1))
                .map(|b| b.id),
            _ => None,
        });

        // Apply the action; capture the returned (canonical, inverse) pair
        // and push it onto the undo stack. apply returns None for
        // unrecordable cases (UI-only, no-op, or stage-1-unrecordable
        // structural splits / first-block delete).
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
        }

        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            _ => None,
        });
        let change_type_focus = match &action {
            BlockAction::ChangeType { block_id, .. } => Some(*block_id),
            _ => None,
        };
        // A freshly inserted block (e.g. the empty-document "add block"
        // button) should take focus so the caret lands in it immediately.
        let insert_focus = match &action {
            BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
            _ => None,
        };
        if let Some(id) = pre_focus
            .or(post_focus)
            .or(change_type_focus)
            .or(insert_focus)
        {
            floem::action::exec_after(Duration::from_millis(0), move |_| {
                focus_target.set(Some(id));
            });
        }
        on_action_mark_dirty();
    })
}
```

### Step 5.2: Create `undo_redo.rs`

Create `crates/lopress-editor/src/ui/editing/undo_redo.rs`:

```rust
//! Undo/redo builders for the editing view.
//!
//! Each builder takes the signals it needs and returns an `Rc<dyn Fn()>`
/// closure. The closures share the same focus-computation pattern: pop
/// from the stack, resolve the focus target from the pre-apply doc, apply
/// the inverse action, and mark dirty.

use crate::actions::{apply, BlockAction};
use crate::model::types::{BlockId, EditorDoc};
use crate::ui::editing::focus::focus_after_apply;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use std::rc::Rc;
use std::time::Duration;

/// Build the `on_undo` closure.
///
/// Pops an entry from the undo stack, resolves the focus target from the
/// pre-apply doc (MergeWithPrev deletes its target so focus must land on
/// the predecessor), applies the inverse action, and marks dirty.
pub fn build_undo(
    undo_stack: RwSignal<crate::undo::UndoStack>,
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    mark_dirty: Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let mut popped = None;
        undo_stack.update(|s| {
            popped = s.pop_undo();
        });
        if let Some(action) = popped {
            // Compute focus from the pre-apply doc — MergeWithPrev
            // deletes its target, so focus must resolve to the
            // surviving predecessor before the apply runs.
            let focus_id =
                current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
            let action_for_apply = action.clone();
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    let _ = apply(d, action_for_apply);
                }
            });
            // No post-apply id surgery: Split / SplitListItem in stored
            // entries carry new_block_id: Some(...), so re-applying them
            // is id-stable without patching the redo entry.
            if let Some(id) = focus_id {
                floem::action::exec_after(Duration::from_millis(0), move |_| {
                    focus_target.set(Some(id));
                });
            }
            mark_dirty();
        }
    })
}

/// Build the `on_redo` closure.
///
/// Same pattern as `build_undo`: pop from the redo stack, resolve focus,
/// apply the canonical action, mark dirty.
pub fn build_redo(
    undo_stack: RwSignal<crate::undo::UndoStack>,
    current_doc: RwSignal<Option<EditorDoc>>,
    focus_target: RwSignal<Option<BlockId>>,
    mark_dirty: Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let mark_dirty = Rc::clone(&mark_dirty);
    Rc::new(move || {
        let mut popped = None;
        undo_stack.update(|s| {
            popped = s.pop_redo();
        });
        if let Some(action) = popped {
            let focus_id =
                current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
            let action_for_apply = action.clone();
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    let _ = apply(d, action_for_apply);
                }
            });
            // No post-apply id surgery for the same reason as on_undo.
            if let Some(id) = focus_id {
                floem::action::exec_after(Duration::from_millis(0), move |_| {
                    focus_target.set(Some(id));
                });
            }
            mark_dirty();
        }
    })
}
```

### Step 5.3: Extract from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 249-377):

```rust
    // Chokepoint: every block-tree mutation routes through here. Pre/post
    // lookups derive the block to focus after structural actions.
    let on_action_mark_dirty = Rc::clone(&mark_dirty);
    let on_action: ActionSink = Rc::new(move |action: BlockAction| {
        let _t = perf::span("editor.on_action");
        if let BlockAction::OpenSlashMenu { block_id } = action {
            slash_menu_open.set(Some(block_id));
            return;
        }
        if slash_menu_open.get_untracked().is_some() {
            slash_menu_open.set(None);
        }

        // Pre-focus must read pre-apply state (the block before the one
        // being merged into its predecessor). Capture it before the apply
        // mutates the doc.
        let pre_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::MergeWithPrev { block_id }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .filter(|&i| i > 0)
                .and_then(|i| d.blocks.get(i - 1))
                .map(|b| b.id),
            _ => None,
        });

        // Apply the action; capture the returned (canonical, inverse) pair
        // and push it onto the undo stack. apply returns None for
        // unrecordable cases (UI-only, no-op, or stage-1-unrecordable
        // structural splits / first-block delete).
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
        }

        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            _ => None,
        });
        let change_type_focus = match &action {
            BlockAction::ChangeType { block_id, .. } => Some(*block_id),
            _ => None,
        };
        // A freshly inserted block (e.g. the empty-document "add block"
        // button) should take focus so the caret lands in it immediately.
        let insert_focus = match &action {
            BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
            _ => None,
        };
        if let Some(id) = pre_focus
            .or(post_focus)
            .or(change_type_focus)
            .or(insert_focus)
        {
            floem::action::exec_after(Duration::from_millis(0), move |_| {
                focus_target.set(Some(id));
            });
        }
        on_action_mark_dirty();
    });

    let on_undo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_undo();
            });
            if let Some(action) = popped {
                // Compute focus from the pre-apply doc — MergeWithPrev
                // deletes its target, so focus must resolve to the
                // surviving predecessor before the apply runs.
                let focus_id =
                    current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                // No post-apply id surgery: Split / SplitListItem in stored
                // entries carry new_block_id: Some(...), so re-applying them
                // is id-stable without patching the redo entry.
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };

    let on_redo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_redo();
            });
            if let Some(action) = popped {
                let focus_id =
                    current_doc.with_untracked(|m| focus_after_apply(m.as_ref(), &action));
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                // No post-apply id surgery for the same reason as on_undo.
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };
```

Delete this entire block (lines 249-377).

Replace with:
```rust
    // ── Action sink + undo/redo closures ───────────────────────────────
    let on_action = action_sink::build_action_sink(
        current_doc, focus_target, slash_menu_open, undo_stack, Rc::clone(&save.mark_dirty),
    );
    let on_undo = undo_redo::build_undo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));
    let on_redo = undo_redo::build_redo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));
```

Add `use crate::ui::editing::{action_sink, undo_redo};` to the top of
`mod.rs`.

### Step 5.4: Register the new modules in `editing/mod.rs`

Append `pub mod action_sink;` and `pub mod undo_redo;` to
`crates/lopress-editor/src/ui/editing/mod.rs`. The file should now read:

```rust
pub mod focus;
pub mod pane_key;
pub mod new_doc;
pub mod save_pipeline;
pub mod action_sink;
pub mod undo_redo;
```

### Step 5.5: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 5.6: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 5.7: Commit

```bash
git add crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/editing/action_sink.rs crates/lopress-editor/src/ui/editing/undo_redo.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract action_sink and undo_redo closures from editing_view

Moves the on_action closure into action_sink::build_action_sink (taking
signals as arguments and returning an ActionSink), and the on_undo/on_redo
closures into undo_redo::build_undo/build_redo (each returning Rc<dyn Fn()>).
editing_view assembles these builders after creating the signals.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Extract `ctrl_wire.rs` + final cleanup

**Why:** The debug ctrl wiring block is `#[cfg(debug_assertions)]` gated and
self-contained. Moving it into `wire_ctrl` is the last extraction. After
this, `editing_view` matches the spec's sketch (~80 lines).

### Step 6.1: Create `ctrl_wire.rs`

Create `crates/lopress-editor/src/ui/editing/ctrl_wire.rs`:

```rust
//! Debug ctrl wiring: serialises the editor state over HTTP for the
//! debug control server.
//!
//! Gated on `#[cfg(debug_assertions)]` — not compiled in release builds.

use crate::ctrl::{CtrlActionEnvelope, CtrlActionResult, CtrlHandle, serialize_state};
use crate::model::types::EditorDoc;
use floem::ext_event::create_signal_from_channel;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::keypress::press::KeyPress;
use std::path::PathBuf;
use std::rc::Rc;

use crate::actions::BlockAction;
use crate::ui::blocks::inline_editor::ActionSink;

/// Wire the debug ctrl handle to the editor state.
///
/// Creates a signal from the ctrl action channel and an effect that
/// serialises the current doc state on every reactive update.
#[cfg(debug_assertions)]
pub fn wire_ctrl(
    ctrl: (CtrlHandle, crossbeam_channel::Receiver<CtrlActionEnvelope>),
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: ActionSink,
) {
    let (ctrl_handle, ctrl_action_rx) = ctrl;
    use floem::ext_event::create_signal_from_channel;
    use floem::reactive::create_effect;

    let snap = ctrl_handle.snapshot.clone();
    create_effect(move |_| {
        let json = current_doc.with(|maybe| {
            serialize_state(
                maybe.as_ref(),
                current_path.get_untracked().as_deref(),
            )
        });
        *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
    });

    let action_read = create_signal_from_channel(ctrl_action_rx);
    create_effect(move |_| {
        if let Some((ctrl_action, reply_tx)) = action_read.get() {
            let block_id = ctrl_action.block_id();
            // Translate against the current doc. into_block_action's
            // only failure mode is an unknown block id; a missing doc
            // is detected separately so the caller gets a precise
            // result. on_action MUST run outside with_untracked — it
            // calls current_doc.update() and would re-borrow the signal.
            let translated: Result<BlockAction, CtrlActionResult> = current_doc
                .with_untracked(|maybe| match maybe.as_ref() {
                    None => Err(CtrlActionResult::NoDocument),
                    Some(doc) => ctrl_action
                        .into_block_action(doc)
                        .ok_or(CtrlActionResult::BlockNotFound { block_id }),
                });
            let result = match translated {
                Ok(action) => {
                    on_action(action);
                    CtrlActionResult::Dispatched
                }
                Err(failure) => failure,
            };
            let _ = reply_tx.send(result);
        }
    });
}
```

### Step 6.2: Extract from `ui/mod.rs`

In `crates/lopress-editor/src/ui/mod.rs`, find (lines 379-543):

```rust
    // Cloned for the debug ctrl wiring near the end of this function;
    // `on_action` itself is moved into the dyn_container view closure.
    #[cfg(debug_assertions)]
    let on_action_for_ctrl = on_action.clone();
```

And find (lines 500-543):
```rust
    // ── Debug ctrl wiring ────────────────────────────────────────────────────
    #[cfg(debug_assertions)]
    if let Some((ctrl_handle, ctrl_action_rx)) = ctrl {
        use floem::ext_event::create_signal_from_channel;
        use floem::reactive::create_effect;

        let snap = ctrl_handle.snapshot.clone();
        create_effect(move |_| {
            let json = current_doc.with(|maybe| {
                crate::ctrl::serialize_state(
                    maybe.as_ref(),
                    current_path.get_untracked().as_deref(),
                )
            });
            *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
        });

        let action_read = create_signal_from_channel(ctrl_action_rx);
        create_effect(move |_| {
            if let Some((ctrl_action, reply_tx)) = action_read.get() {
                let block_id = ctrl_action.block_id();
                // Translate against the current doc. into_block_action's
                // only failure mode is an unknown block id; a missing doc
                // is detected separately so the caller gets a precise
                // result. on_action MUST run outside with_untracked — it
                // calls current_doc.update() and would re-borrow the signal.
                let translated: Result<BlockAction, crate::ctrl::CtrlActionResult> = current_doc
                    .with_untracked(|maybe| match maybe.as_ref() {
                        None => Err(crate::ctrl::CtrlActionResult::NoDocument),
                        Some(doc) => ctrl_action
                            .into_block_action(doc)
                            .ok_or(crate::ctrl::CtrlActionResult::BlockNotFound { block_id }),
                    });
                let result = match translated {
                    Ok(action) => {
                        on_action_for_ctrl(action);
                        crate::ctrl::CtrlActionResult::Dispatched
                    }
                    Err(failure) => failure,
                };
                let _ = reply_tx.send(result);
            }
        });
    }
```

Delete the `on_action_for_ctrl` clone (lines 379-386) and the entire
debug ctrl wiring block (lines 498-543).

Replace with:
```rust
    // ── Debug ctrl wiring ────────────────────────────────────────────────────
    #[cfg(debug_assertions)]
    if let Some(c) = ctrl {
        ctrl_wire::wire_ctrl(c, current_doc, current_path, on_action.clone());
    }
```

Add `use crate::ui::editing::ctrl_wire;` to the top of `mod.rs`.

### Step 6.3: Register the new module in `editing/mod.rs`

Append `pub mod ctrl_wire;` to `crates/lopress-editor/src/ui/editing/mod.rs`.
The file should now read:

```rust
pub mod focus;
pub mod pane_key;
pub mod new_doc;
pub mod save_pipeline;
pub mod action_sink;
pub mod undo_redo;
pub mod ctrl_wire;
```

### Step 6.4: The resulting `editing_view`

After all extractions, `editing_view` should look like this (approximately
80 lines):

```rust
/// Three-column scaffold: sidebar (left) + editor pane (center) + inspector (right),
/// with a footer pinned at the bottom.
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    #[cfg(debug_assertions)] ctrl: Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
    )>,
) -> impl IntoView {
    // 1. Workspace + path signals.
    let initial_ws: WorkspaceSummary = editing
        .borrow()
        .as_ref()
        .map(|s| s.session.workspace())
        .unwrap_or_else(|| WorkspaceSummary {
            root: PathBuf::new(),
            name: String::new(),
            posts: Vec::new(),
            pages: Vec::new(),
        });
    let workspace_signal: RwSignal<WorkspaceSummary> = RwSignal::new(initial_ws);
    let current_path: RwSignal<Option<PathBuf>> = RwSignal::new(None);

    let undo_stack: RwSignal<crate::undo::UndoStack> = RwSignal::new(crate::undo::UndoStack::new());

    // 2. Open + new-doc sidebar wiring.
    let editing_for_open = Rc::clone(&editing);
    let on_open: Rc<dyn Fn(DocumentRef)> = Rc::new(move |doc_ref: DocumentRef| {
        let mut guard = editing_for_open.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        undo_stack.update(|s| *s = crate::undo::UndoStack::new());
    });

    let on_new_post = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Post,
    );
    let on_new_page = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Page,
    );

    let sidebar = sidebar_view(
        workspace_signal,
        current_path,
        on_open,
        on_new_post,
        on_new_page,
    );

    // 3. Focus + slash + dnd signals.
    let focus_target: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let slash_menu_open: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let dnd = DndState::new();

    // 4. Save pipeline (signals + polling + debounce).
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);

    // 5. Action sink + undo/redo closures.
    let on_action = action_sink::build_action_sink(
        current_doc, focus_target, slash_menu_open, undo_stack, Rc::clone(&save.mark_dirty),
    );
    let on_undo = undo_redo::build_undo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));
    let on_redo = undo_redo::build_redo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));

    // 6. Sidebar + new-doc actions (sidebar already built above).

    // 7. Editor pane — keyed on block id sequence + kind tag + plugin presence.
    let pane_key = pane_key::build_pane_key(current_doc);
    let editor = dyn_container(pane_key, move |maybe_ids| match maybe_ids {
        Some(_ids) => match current_doc.with_untracked(|d| d.clone()) {
            Some(doc) => editor_pane::editor_pane(
                &doc,
                on_action.clone(),
                focus_target,
                slash_menu_open,
                dnd,
                current_doc,
                on_undo.clone(),
                on_redo.clone(),
            )
            .into_any(),
            None => empty().into_any(),
        },
        None => label(|| "No document open. Pick one from the sidebar.")
            .style(|s| {
                s.width_full()
                    .height_full()
                    .items_center()
                    .justify_center()
                    .color(Color::rgb8(140, 140, 140))
            })
            .into_any(),
    })
    .style(|s| s.flex_grow(1.0).height_full().min_height(0.));

    // 8. Inspector + footer.
    let inspector = inspector_view(current_doc, current_path, Rc::clone(&save.mark_dirty));
    let footer = footer_view(
        save.build_status_sig,
        save.dirty_sig,
        save.save_error_sig,
        current_doc,
        save.serve_status_sig,
    );

    // 9. Debug ctrl wiring.
    #[cfg(debug_assertions)]
    if let Some(c) = ctrl {
        ctrl_wire::wire_ctrl(c, current_doc, current_path, on_action.clone());
    }

    // 10. Assembly.
    let columns = h_stack((sidebar, editor, inspector))
        .style(|s| s.width_full().flex_grow(1.0).min_height(0.));

    let editing_for_close = Rc::clone(&editing);
    stack((columns, footer))
        .style(|s| s.flex_col().width_full().height_full())
        .on_event_stop(EventListener::WindowClosed, move |_e: &Event| {
            // Force-flush any unsaved edits before the window dies.
            if !save.dirty_sig.get_untracked() {
                return;
            }
            let doc = match current_doc.with_untracked(|d| d.clone()) {
                Some(d) => d,
                None => return,
            };
            if let Some(state) = editing_for_close.borrow().as_ref() {
                let _ = state.save_doc(&doc);
            }
        })
}
```

### Step 6.5: Run tests

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

### Step 6.6: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 6.7: Verify `editing_view` line count

```bash
wc -l crates/lopress-editor/src/ui/mod.rs
```

Expected: ~80-90 lines for `editing_view` (the whole `mod.rs` should be
around 130 lines including `root_view`, `StateTag`, `MAX_RECENTS`, and
imports).

### Step 6.8: Commit

```bash
git add crates/lopress-editor/src/ui/editing/ctrl_wire.rs crates/lopress-editor/src/ui/editing/mod.rs crates/lopress-editor/src/ui/mod.rs
git commit -m "$(cat <<'EOF'
refactor(editor): extract debug ctrl wiring to ctrl_wire.rs and finalise editing_view

Moves the #[cfg(debug_assertions)] ctrl wiring block from ui/mod.rs into
ctrl_wire::wire_ctrl. editing_view is now ~80 lines that assembles the
pieces: signals → sidebar → save pipeline → action sink/undo/redo →
pane key → editor pane → inspector → footer → ctrl wire → assembly.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Final verification

### Step F.1: Full workspace test suite

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass across all crates.

### Step F.2: Workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean, no warnings.

### Step F.3: Verify file structure

```bash
find crates/lopress-editor/src/ui/editing -type f | sort
```

Expected output:
```
crates/lopress-editor/src/ui/editing/action_sink.rs
crates/lopress-editor/src/ui/editing/ctrl_wire.rs
crates/lopress-editor/src/ui/editing/focus.rs
crates/lopress-editor/src/ui/editing/mod.rs
crates/lopress-editor/src/ui/editing/new_doc.rs
crates/lopress-editor/src/ui/editing/pane_key.rs
crates/lopress-editor/src/ui/editing/save_pipeline.rs
crates/lopress-editor/src/ui/editing/undo_redo.rs
```

### Step F.4: Verify `mod.rs` size

```bash
wc -l crates/lopress-editor/src/ui/mod.rs
```

Expected: ~130 lines total (root_view ~60 lines, editing_view ~80 lines,
StateTag + MAX_RECENTS + imports).

### Step F.5: Verify `defer_focus` unification

```bash
grep -rn 'fn defer_focus' crates/lopress-editor/src/ui/
```

Expected: exactly one hit — `ui/editing/focus.rs`.

### Step F.6: Verify no stale `defer_focus` in list.rs or code_editor.rs

```bash
grep -n 'defer_focus' crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/code_editor.rs
```

Expected: only the import line (`use crate::ui::editing::focus::defer_focus;`),
no function definition.

### Step F.7: Sanity check — no `#[allow]` or `#[expect]` added

```bash
grep -rn '#\[allow\|#\[expect' crates/lopress-editor/src/ui/editing/
```

Expected: no output.

---

## Done when

- `cargo test --workspace` passes with zero failures
- `cargo check --workspace` is clean
- `grep -rn 'fn defer_focus' crates/lopress-editor/src/ui/` returns exactly one hit (`focus.rs`)
- `wc -l crates/lopress-editor/src/ui/mod.rs` is ~130
- `wc -l crates/lopress-editor/src/ui/editing/mod.rs` is ~80
- Six commits land in order:
  1. `refactor(editor): extract focus helpers to ui/editing/focus.rs and unify defer_focus`
  2. `refactor(editor): extract KindTag, kind_tag, and pane_key closure to pane_key.rs`
  3. `refactor(editor): extract DocKind and make_new_doc_action to new_doc.rs`
  4. `refactor(editor): extract save pipeline signals and debounce into save_pipeline.rs`
  5. `refactor(editor): extract action_sink and undo_redo closures from editing_view`
  6. `refactor(editor): extract debug ctrl wiring to ctrl_wire.rs and finalise editing_view`

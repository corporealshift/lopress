# Slug Filename Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep each post/page `.md` filename synchronized with its slug, so `untitled-N.md` never lingers and the filesystem matches the site's URLs.

**Architecture:** A new pure `slugify` in `lopress-core`. A new `filename_sync` module in the editor crate holds the pure rename-decision logic (`resolve_target`) plus the fs rename (`rename_to_slug`) and a signal-updating wiring helper. `EditingState::sync_filename` is a thin method the two save sites (debounced autosave + doc-switch flush) call after a successful save. New-doc creation seeds filenames from the slugified default title instead of `untitled-N`.

**Tech Stack:** Rust, Floem reactive signals, `std::fs`, `tempfile` (tests).

---

### Task 1: `slugify` in `lopress-core`

**Files:**
- Create: `crates/lopress-core/src/slug.rs`
- Modify: `crates/lopress-core/src/lib.rs:12-24`

- [ ] **Step 1: Write the failing test**

Create `crates/lopress-core/src/slug.rs`:

```rust
//! Slug derivation: turn a human title/slug string into a filesystem- and
//! URL-safe stem (lowercase ASCII alphanumerics separated by single hyphens).

/// Lowercase, collapse every run of non-ASCII-alphanumeric characters into a
/// single `-`, and strip leading/trailing `-`. The result may be empty (e.g.
/// the input was all punctuation) — callers must guard for that.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_cases() {
        assert_eq!(slugify("My First Post"), "my-first-post");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("already-slug"), "already-slug");
        assert_eq!(slugify("a---b"), "a-b");
        assert_eq!(slugify("!!!"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_is_idempotent() {
        let once = slugify("My First Post");
        assert_eq!(slugify(&once), once);
    }
}
```

- [ ] **Step 2: Register the module and re-export**

In `crates/lopress-core/src/lib.rs`, add `pub mod slug;` to the module list (after `pub mod serializer;`) and add `slug::slugify` to the re-exports. The two edited regions become:

```rust
pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod parser;
pub mod perf;
pub mod serializer;
pub mod slug;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use parser::{parse, render_inline_markdown, render_markdown};
pub use serializer::serialize;
pub use slug::slugify;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p lopress-core slug`
Expected: PASS (`slugify_cases`, `slugify_is_idempotent`).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-core/src/slug.rs crates/lopress-core/src/lib.rs
git commit -m "feat(core): add slugify"
```

---

### Task 2: `filename_sync` pure logic + fs rename

**Files:**
- Create: `crates/lopress-editor/src/ui/editing/filename_sync.rs`
- Modify: `crates/lopress-editor/src/ui/editing/mod.rs:11-13` (add `pub mod filename_sync;`)

- [ ] **Step 1: Write the module with its tests**

Create `crates/lopress-editor/src/ui/editing/filename_sync.rs`:

```rust
//! Keep a document's `.md` filename synchronized with its slug.
//!
//! The *effective slug* is `slugify(front_matter.slug)` when that is
//! non-empty, else `slugify(front_matter.title)`. The filename stem is kept
//! equal to it. Front matter is never mutated — only the file moves.

use lopress_core::{slugify, FrontMatter};
use std::path::{Path, PathBuf};

/// The slug a document's filename should track, or empty when neither the
/// slug field nor the title yields anything usable.
fn effective_slug(fm: &FrontMatter) -> String {
    let from_slug = fm
        .slug
        .as_deref()
        .map(slugify)
        .filter(|s| !s.is_empty());
    from_slug.unwrap_or_else(|| slugify(fm.title.as_deref().unwrap_or("")))
}

/// Resolve a unique `{base}.md` / `{base}-N.md` path within `dir`, treating
/// `exclude` (the file's own current path) as available so a file never
/// collides with itself. `exists` reports whether a candidate is already
/// taken — injected so this is testable without touching disk.
pub fn unique_stem(
    dir: &Path,
    base: &str,
    exclude: Option<&Path>,
    exists: &impl Fn(&Path) -> bool,
) -> PathBuf {
    let first = dir.join(format!("{base}.md"));
    if Some(first.as_path()) == exclude || !exists(&first) {
        return first;
    }
    for n in 2..=9999u32 {
        let cand = dir.join(format!("{base}-{n}.md"));
        if Some(cand.as_path()) == exclude || !exists(&cand) {
            return cand;
        }
    }
    // Defensive: 9998 same-named files in one dir is not a real workflow.
    dir.join(format!("{base}-{}.md", u32::MAX))
}

/// The path `current` should be renamed to, or `None` when no rename is
/// needed (empty effective slug, or the current stem already matches).
pub fn resolve_target(
    fm: &FrontMatter,
    current: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let base = effective_slug(fm);
    if base.is_empty() {
        return None;
    }
    let dir = current.parent()?;
    let target = unique_stem(dir, &base, Some(current), &exists);
    if target.as_path() == current {
        None
    } else {
        Some(target)
    }
}

/// Rename `current` on disk to match its slug. Returns the new path, or `None`
/// when no rename was needed.
///
/// # Errors
/// Returns the underlying I/O error if the rename fails.
pub fn rename_to_slug(fm: &FrontMatter, current: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(target) = resolve_target(fm, current, |p| p.exists()) else {
        return Ok(None);
    };
    std::fs::rename(current, &target)?;
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(title: Option<&str>, slug: Option<&str>) -> FrontMatter {
        FrontMatter {
            title: title.map(str::to_string),
            slug: slug.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn derives_target_from_title() {
        let cur = Path::new("/posts/untitled-1.md");
        let got = resolve_target(&fm(Some("My First Post"), None), cur, |_| false);
        assert_eq!(got, Some(PathBuf::from("/posts/my-first-post.md")));
    }

    #[test]
    fn explicit_slug_wins_over_title() {
        let cur = Path::new("/posts/untitled-1.md");
        let got = resolve_target(&fm(Some("My First Post"), Some("intro")), cur, |_| false);
        assert_eq!(got, Some(PathBuf::from("/posts/intro.md")));
    }

    #[test]
    fn stem_already_matches_is_noop() {
        let cur = Path::new("/posts/my-first-post.md");
        assert_eq!(resolve_target(&fm(Some("My First Post"), None), cur, |_| false), None);
    }

    #[test]
    fn empty_effective_slug_is_noop() {
        let cur = Path::new("/posts/untitled-1.md");
        assert_eq!(resolve_target(&fm(None, None), cur, |_| false), None);
        assert_eq!(resolve_target(&fm(Some("!!!"), Some("!!!")), cur, |_| false), None);
    }

    #[test]
    fn collision_appends_suffix() {
        let cur = Path::new("/posts/untitled-1.md");
        // "hello.md" is taken by a different file; "hello-2.md" is free.
        let taken = Path::new("/posts/hello.md");
        let got = resolve_target(&fm(Some("Hello"), None), cur, |p| p == taken);
        assert_eq!(got, Some(PathBuf::from("/posts/hello-2.md")));
    }

    #[test]
    fn current_path_counts_as_available() {
        // File already named hello-2.md; hello.md taken by someone else.
        let cur = Path::new("/posts/hello-2.md");
        let taken = Path::new("/posts/hello.md");
        assert_eq!(resolve_target(&fm(Some("Hello"), None), cur, |p| p == taken), None);
    }

    #[test]
    fn rename_to_slug_moves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("untitled-1.md");
        std::fs::write(&old, "x").unwrap();
        let new = rename_to_slug(&fm(Some("My First Post"), None), &old)
            .unwrap()
            .unwrap();
        assert_eq!(new, dir.path().join("my-first-post.md"));
        assert!(new.exists());
        assert!(!old.exists());
    }

    #[test]
    fn rename_to_slug_suffixes_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.md"), "a").unwrap();
        let old = dir.path().join("untitled-1.md");
        std::fs::write(&old, "b").unwrap();
        let new = rename_to_slug(&fm(Some("Hello"), None), &old).unwrap().unwrap();
        assert_eq!(new, dir.path().join("hello-2.md"));
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/lopress-editor/src/ui/editing/mod.rs`, add `pub mod filename_sync;` (keep alphabetical-ish with the neighbours):

```rust
pub mod new_doc;
```
becomes
```rust
pub mod filename_sync;
pub mod new_doc;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p lopress-editor filename_sync`
Expected: PASS (all 8 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/editing/filename_sync.rs crates/lopress-editor/src/ui/editing/mod.rs
git commit -m "feat(editor): filename-sync slug resolution + rename"
```

---

### Task 3: `EditingState::sync_filename`

**Files:**
- Modify: `crates/lopress-editor/src/state.rs:1-10` (imports), `crates/lopress-editor/src/state.rs:90` (add method after `save_doc`)

- [ ] **Step 1: Add the `FrontMatter` import**

In `crates/lopress-editor/src/state.rs`, the existing line:

```rust
use lopress_core::Document;
```
becomes:
```rust
use lopress_core::{Document, FrontMatter};
```

- [ ] **Step 2: Add the method**

Immediately after the `save_doc` method (which ends at line 90 with its closing `}`), inside `impl EditingState`, add:

```rust
    /// Rename the open document's file so its stem matches `front_matter`'s
    /// effective slug. Returns the new path when a rename happened, `None`
    /// when the filename already matched (or no doc is open).
    ///
    /// Updates `self.current_ref.path` on a successful rename; callers are
    /// responsible for reflecting the new path in the UI signals and
    /// re-scanning the workspace.
    pub fn sync_filename(&mut self, front_matter: &FrontMatter) -> Result<Option<PathBuf>, String> {
        let Some(current) = self.current_ref.as_ref().map(|r| r.path.clone()) else {
            return Ok(None);
        };
        match crate::ui::editing::filename_sync::rename_to_slug(front_matter, &current) {
            Ok(Some(new_path)) => {
                if let Some(r) = self.current_ref.as_mut() {
                    r.path = new_path.clone();
                }
                Ok(Some(new_path))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
```

Note: `PathBuf` is already in scope in `state.rs` via `lopress_gui_host` types; if the compiler reports it missing, add `use std::path::PathBuf;` to the imports.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/state.rs
git commit -m "feat(editor): EditingState::sync_filename"
```

---

### Task 4: Wire the sync into both save sites

**Files:**
- Modify: `crates/lopress-editor/src/ui/editing/filename_sync.rs` (add the wiring helper)
- Modify: `crates/lopress-editor/src/ui/editing/save_pipeline.rs` (signatures + call sites)
- Modify: `crates/lopress-editor/src/ui/mod.rs:252,268` (updated call sites)
- Modify: `crates/lopress-editor/src/ui/editing/new_doc.rs:46` (updated call site)

This whole task is one commit so the crate keeps compiling.

- [ ] **Step 1: Add the wiring helper to `filename_sync.rs`**

Append to `crates/lopress-editor/src/ui/editing/filename_sync.rs` (before the `#[cfg(test)]` module). Add these imports to the top of the file alongside the existing ones:

```rust
use crate::model::types::EditorDoc;
use crate::state::EditingState;
use floem::reactive::{RwSignal, SignalUpdate, SignalWith};
use lopress_gui_host::WorkspaceSummary;
use std::cell::RefCell;
use std::rc::Rc;
```

Then add:

```rust
/// After a successful save, rename the open document's file to match its slug
/// and, when a rename happened, push the new path into `current_path` and
/// re-scan so the sidebar row and inspector placeholder follow. A no-op when
/// nothing needs renaming; a rename failure is logged, not fatal (the content
/// is already safely saved under the old name).
pub fn sync_filename_and_update(
    editing: &Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
) {
    let Some(fm) = current_doc.with_untracked(|d| d.as_ref().map(|doc| doc.front_matter.clone()))
    else {
        return;
    };
    let outcome = {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        match state.sync_filename(&fm) {
            Ok(Some(new_path)) => Some((new_path, state.session.rescan())),
            Ok(None) => None,
            Err(e) => {
                eprintln!("filename sync failed: {e}");
                None
            }
        }
    };
    if let Some((new_path, summary)) = outcome {
        current_path.set(Some(new_path));
        workspace_signal.set(summary);
    }
}
```

(The `editing` borrow is dropped at the end of the block before the signals are set, so reactive effects triggered by `set` can freely borrow `editing`.)

- [ ] **Step 2: Thread signals through `save_pipeline.rs`**

In `crates/lopress-editor/src/ui/editing/save_pipeline.rs`:

Add imports near the top (with the existing `use` lines):

```rust
use lopress_gui_host::WorkspaceSummary;
use std::path::PathBuf;
```

Change `start_save_pipeline`'s signature to accept the two extra signals:

```rust
pub fn start_save_pipeline(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
) -> SavePipeline {
```

Inside the debounced save closure, the current success arm reads:

```rust
                Ok(()) => {
                    ds.set(false);
                    ses.set(None);
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
```

Replace it with (sync the filename before the rebuild so the build sees the new name):

```rust
                Ok(()) => {
                    ds.set(false);
                    ses.set(None);
                    crate::ui::editing::filename_sync::sync_filename_and_update(
                        &editing_for_save,
                        current_doc,
                        current_path,
                        workspace_signal,
                    );
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
```

Change `flush_pending_edits`'s signature to accept the two extra signals:

```rust
pub fn flush_pending_edits(
    signals: FlushSignals,
    editing: &Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
) -> bool {
```

In `flush_pending_edits`, the `Some(Ok(()))` arm currently reads:

```rust
        Some(Ok(())) => {
            dirty_sig.set(false);
            save_error_sig.set(None);
            if let Some(state) = editing.borrow().as_ref() {
                state.session.rebuild();
            }
            true
        }
```

Replace it with:

```rust
        Some(Ok(())) => {
            dirty_sig.set(false);
            save_error_sig.set(None);
            crate::ui::editing::filename_sync::sync_filename_and_update(
                editing,
                current_doc,
                current_path,
                workspace_signal,
            );
            if let Some(state) = editing.borrow().as_ref() {
                state.session.rebuild();
            }
            true
        }
```

- [ ] **Step 3: Update the `start_save_pipeline` call site in `mod.rs`**

In `crates/lopress-editor/src/ui/mod.rs`, the call at line 252:

```rust
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);
```
becomes:
```rust
    let save = save_pipeline::start_save_pipeline(
        Rc::clone(&editing),
        current_doc,
        current_path,
        workspace_signal,
    );
```

- [ ] **Step 4: Update the `flush_pending_edits` call site in `mod.rs`**

In the `on_open` closure (around line 268):

```rust
        if !save_pipeline::flush_pending_edits(flush_signals, &editing_for_open, current_doc) {
            return;
        }
```
becomes:
```rust
        if !save_pipeline::flush_pending_edits(
            flush_signals,
            &editing_for_open,
            current_doc,
            current_path,
            workspace_signal,
        ) {
            return;
        }
```

- [ ] **Step 5: Update the `flush_pending_edits` call site in `new_doc.rs`**

`make_new_doc_action` already receives `workspace_signal` and `current_path`. The call at `crates/lopress-editor/src/ui/editing/new_doc.rs:46`:

```rust
        if !crate::ui::editing::save_pipeline::flush_pending_edits(
            flush_signals,
            &editing,
            current_doc,
        ) {
            return;
        }
```
becomes:
```rust
        if !crate::ui::editing::save_pipeline::flush_pending_edits(
            flush_signals,
            &editing,
            current_doc,
            current_path,
            workspace_signal,
        ) {
            return;
        }
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: builds clean (no unused-import or arity errors).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/editing/filename_sync.rs crates/lopress-editor/src/ui/editing/save_pipeline.rs crates/lopress-editor/src/ui/mod.rs crates/lopress-editor/src/ui/editing/new_doc.rs
git commit -m "feat(editor): rename doc file to match slug on save"
```

---

### Task 5: Slug-based new-document filenames

**Files:**
- Modify: `crates/lopress-editor/src/ui/sidebar.rs:178-193` (replace `unique_untitled_path`)
- Modify: `crates/lopress-editor/src/ui/editing/new_doc.rs:5,65` (use the new helper)

- [ ] **Step 1: Replace `unique_untitled_path` in `sidebar.rs`**

In `crates/lopress-editor/src/ui/sidebar.rs`, replace the whole `unique_untitled_path` function (lines 178-193) with a slug-seeded helper that delegates to `filename_sync::unique_stem`:

```rust
/// Pick a unique `{base}.md` filename inside `dir`, where `base` is an
/// already-slugified stem (e.g. the slugified default title). Falls back to
/// `"untitled"` if `base` is empty.
pub fn unique_doc_path(dir: &Path, base: &str) -> PathBuf {
    let base = if base.is_empty() { "untitled" } else { base };
    crate::ui::editing::filename_sync::unique_stem(dir, base, None, &|p: &Path| p.exists())
}
```

- [ ] **Step 2: Update `new_doc.rs` to use it**

In `crates/lopress-editor/src/ui/editing/new_doc.rs`, change the import at line 5:

```rust
use crate::ui::sidebar::{new_doc_stub, unique_untitled_path};
```
becomes:
```rust
use crate::ui::sidebar::{new_doc_stub, unique_doc_path};
```

And change the path-picking line (line 65):

```rust
        let path = unique_untitled_path(&dir);
```
becomes:
```rust
        let path = unique_doc_path(&dir, &lopress_core::slugify(kind.default_title()));
```

- [ ] **Step 3: Update the stale doc comment**

In `new_doc.rs`, the doc comment on `make_new_doc_action` (lines ~28-34) says "picks a fresh `untitled-N.md` filename". Change that phrase to "picks a slug-based filename from the default title (e.g. `new-post.md`)".

- [ ] **Step 4: Verify it compiles and existing tests pass**

Run: `cargo build -p lopress-editor && cargo test -p lopress-editor`
Expected: builds clean; all tests pass. Confirm no remaining references to `unique_untitled_path`:
Run: `git grep -n unique_untitled_path` → expected: no matches in `crates/` (only historical `docs/` plans may match).

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/sidebar.rs crates/lopress-editor/src/ui/editing/new_doc.rs
git commit -m "feat(editor): name new docs from slugified default title"
```

---

### Task 6: Full gate + manual verification

**Files:** none (verification only)

- [ ] **Step 1: Run the workspace gate**

Run: `bash scripts/check.sh`
Expected: fmt clean, clippy clean (`-D warnings`), suppressions check clean, all tests pass.
(If clippy reports "up-to-date" without re-linting after the earlier `cargo build`/`test`, `touch` the changed `.rs` files or `cargo clean -p lopress-editor -p lopress-core` first — see the clippy-cache note in AGENTS.md.)

- [ ] **Step 2: Manual smoke test on a scratch workspace**

Per CLAUDE.md, never drive a real site. Scaffold and drive a scratch one:

```bash
cargo run --quiet -- new "$TEMP/lopress-scratch"
```

Then (via the `driving-lopress-editor` skill / `verify` skill): open the scratch workspace, click "+ New post" and confirm the created file on disk is `new-post.md` (not `untitled-1.md`); set the Title to "My First Post" in the inspector, let autosave fire, and confirm the file on disk is renamed to `my-first-post.md` and the sidebar row + URL follow. Create a second "New Post" and confirm it lands on `new-post-2.md`. Set an explicit Slug "intro" on a post and confirm the file becomes `intro.md`.

- [ ] **Step 3: Final commit if the gate applied fmt changes**

```bash
git add -A && git commit -m "chore: fmt" || echo "nothing to commit"
```

# Stage 2 — `editor = "code"` widget

> **For the implementer (qwen):** execute this plan task-by-task in order.
> **Prerequisite:** Stage 1 must be fully committed on this branch before
> Task 1 starts. Verify with `git log --oneline | head -10` — you should see
> the Stage 1 commits (manifest, registry, native plugin path, lang mirror,
> tests). If they aren't there, stop and report.
>
> You have full git and the cargo toolchain — commit per task, run the
> verification suite before each commit, and report back when all tasks are
> done. Treat me as a senior reviewer on call: if a test fails or a snippet
> here doesn't match the file you find, stop and report rather than improvising.
>
> The final task is a manual GUI verification — you cannot fully automate
> this stage. The plan tells you what to drive through the GUI; if you don't
> have GUI access, do the cargo verification and hand back a punch list of
> manual checks for the human reviewer.

**Goal:** Add an editable code block widget at `ui/blocks/code_editor.rs`,
register it in the editor registry under the `"code"` key, and re-point the
plugin.rs fallback so plugin-less code blocks (created via `ChangeType`)
also get the editable widget. Code-native keymap: Enter inserts `\n`, Tab
inserts two spaces, navigation jumps blocks at vline boundaries — all
mirroring the list editor's keyboard-isolation pattern but tuned for code.

**Architecture:** The widget builds a single `BlockEditorState` via the
existing `build_block_editor` (fed a synthetic single `InlineRun` carrying
the code body, with no style spans), then mounts via `mount_block_editor`
with two callbacks: a code-specific commit closure that flushes the editor
buffer back to the model as `BlockBody::Code(String)`, and a code-specific
structural-key callback implementing the code keymap. The view wraps the
mounted editor in a frame with a corner lang label, monospace font, and
height sized to the visual-line count — same shape as the list editor's
per-item view.

**Tech stack:** Rust 2021, Floem reactive UI, `lapce-xi-rope::Rope`,
`serde_json`. `cargo test`, `cargo check --workspace`, plus manual GUI
verification (run the editor binary; ideally via the `driving-lopress-editor`
skill / debug HTTP control server on 127.0.0.1:7878).

---

## File structure map

### Files to create

| File | Lines | Change |
|---|---|---|
| `crates/lopress-editor/src/ui/blocks/code_editor.rs` | ~280 | New module — `editable_code_view`, `make_code_commit`, `make_code_structural_key`, private `defer_focus` |

### Files to modify

| File | Line(s) | Change |
|---|---|---|
| `crates/lopress-editor/src/ui/blocks/mod.rs:7-15` | Add `pub mod code_editor;` alongside the existing `pub mod code;` |
| `crates/lopress-editor/src/ui/blocks/editor_registry.rs:13-40` | Import `code_editor`, add `"code"` arm to `editor_for`, define `code_editor_widget` |
| `crates/lopress-editor/src/ui/blocks/editor_registry.rs:44-48` | Extend the existing `editor_for_resolves_list_and_rejects_unknown` test (or add a sibling) |
| `crates/lopress-editor/src/ui/blocks/plugin.rs:351-353` | Re-point `BlockKind::Code` fallback from `code::render_code` to `code_editor::editable_code_view` |

### Files NOT to modify

- `crates/lopress-editor/src/ui/blocks/code.rs` — read-only renderer stays (spec Section 5 defers deletion).
- `crates/lopress-editor/src/ui/mod.rs` — Stage 3 territory.
- Anything in `lopress-core`, `lopress-plugin`, `lopress-build` — already done in Stage 0 and Stage 1.

### Planned temporary duplication

The spec's Section 3 notes that `defer_focus` (the `exec_after(Duration::ZERO)`
helper currently inline in `list.rs:115-119`) should be lifted to
`ui/editing/focus.rs` in Section 4. Since Section 4 is a later stage, **Stage 2
will inline `defer_focus` in `code_editor.rs` as a private helper**, duplicating
list.rs's version. Stage 3 will unify them. **Do NOT extract `defer_focus`
early** — keeping the change footprint small for this stage is more valuable
than the brief duplication.

---

## Conventions

- **Test framework:** Built-in Rust `#[test]`. Unit tests inside
  `#[cfg(test)] mod tests` blocks; integration tests under
  `crates/lopress-editor/tests/`.
- **What's testable vs. manual:** Floem widgets are GUI-driven and hard to
  unit-test for keyboard behavior. **Testable in cargo:**
  - Registry: `editor_for("code").is_some()`.
  - Widget construction does not panic when called with realistic inputs.
  - Existing `actions::apply` / model tests still pass.
  - `BlockKind::Code` round-trip still passes (the Stage 1 tests).

  **Manual verification (use the `driving-lopress-editor` skill or `cargo
  run`):**
  - Open a doc with a code block, type into the body, see characters appear.
  - Press Enter — `\n` inserted, no block split.
  - Press Tab — two spaces inserted at the caret.
  - Press Shift+Enter — same as Enter (newline, no split).
  - Press Backspace at offset 0 of an empty body — block deleted.
  - Press Backspace at offset 0 of a non-empty body — nothing happens
    (keyboard-isolated).
  - Press ArrowUp/Down at first/last vline — caret jumps to prev/next block.
  - Press Ctrl+Home/End — caret jumps to first/last block.
  - Press PageUp/PageDown — 10-block jump.
  - Edit the `lang` attr in the attr form (Stage 1 already wires this) — see
    the corner label update.
  - Save and reopen the document — body + lang preserved.

  Plan should specify all of the above as the Task N manual-verification
  checklist. The plan should NOT attempt to assert keyboard behavior in
  cargo tests — pretending to do so would be a placeholder.

- **Run commands:** `cargo test --workspace`, `cargo check --workspace`,
  `cargo run -p lopress-editor` (or however the editor binary launches —
  inspect `crates/lopress-editor/Cargo.toml` and `src/lib.rs` to confirm).
- **Commit-message style:** Conventional commits. Use `feat(editor):` for
  the widget code, `test(editor):` for test additions, and `refactor(editor):`
  for the plugin.rs re-point. Heredoc form for multi-line, `Co-Authored-By:
  Qwen <noreply@anthropic.com>` trailer.

---

## Task 1: Skeleton + registry wiring

**Why first:** Creates the widget file with a minimal no-op implementation and
wires it into the registry. The widget builds state, mounts with a no-op commit
closure and a no-op structural-key, and renders a basic frame with the correct
styling. Registry tests prove the dispatch works.

### Step 1.1: Verify baseline — Stage 1 tests pass

```bash
cd C:\Users\corpo\Documents\projects\lopress
cargo test -p lopress-editor 2>&1 | tail -5
```

Expected: all pass.

### Step 1.2: Create `code_editor.rs`

Create `crates/lopress-editor/src/ui/blocks/code_editor.rs` with the complete
module. This is the bulk of the work — ~280 lines. The file exports
`editable_code_view` and contains two private helpers and a private `defer_focus`
(duplicate of list.rs's version, to be unified in Stage 3).

```rust
//! Editable code block — the canonical `editor = "code"` implementation.
//!
//! Builds a single `BlockEditorState` via `build_block_editor` (fed a
//! synthetic single `InlineRun` carrying the code body, with no style spans),
//! then mounts via `mount_block_editor` with a code-specific commit closure
//! and a code-specific structural-key callback. The view wraps the mounted
//! editor in a frame with a corner lang label, monospace font, and height
//! sized to the visual-line count.
//!
//! `defer_focus` is a private duplicate of `list.rs`'s version. It will be
//! unified with the shared `focus::defer_focus` in Stage 3.

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
    StructuralKey,
};
use crate::ui::blocks::paragraph::MONO_FAMILY;
use floem::peniko::Color;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::Editor;
use floem::views::{empty, h_stack, label, stack, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Code-specific font size (logical px) for the code body.
const CODE_FONT_SIZE: usize = 13;

/// Commit closure for the code widget. Reads the editor buffer, compares
/// against the model's current body for `block_id`, and emits
/// `EditBlockBody { Code }` when they differ.
fn make_code_commit(
    block_id: BlockId,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> CommitClosure {
    let commit_on_action = on_action.clone();
    Rc::new(move || {
        let live_text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
        let differs = current_doc.with_untracked(|maybe| {
            maybe
                .as_ref()
                .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
                .map(|b| !matches!(&b.body, BlockBody::Code(s) if s == &live_text))
                .unwrap_or(false)
        });
        if differs {
            commit_on_action(BlockAction::EditBlockBody {
                block_id,
                new_body: BlockBody::Code(live_text),
            });
        }
    })
}

/// Code-specific structural-key callback. Implements the code-native keymap
/// table from the spec: Enter/Tab insert into the body, navigation keys
/// jump blocks at vline boundaries, Backspace at offset 0 of an empty body
/// deletes the block, Backspace at offset 0 of a non-empty body is
/// keyboard-isolated.
fn make_code_structural_key(
    block_id: BlockId,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    commit: CommitClosure,
) -> StructuralKey {
    use floem::keyboard::{Key, NamedKey};

    Rc::new(move |kp: &KeyPress, ms: floem::keyboard::Modifiers| {
        let shift = ms.shift();
        let ctrl_or_cmd = ms.control() || ms.meta();

        // Commit before any navigation action.
        let do_commit = || commit();

        // Ctrl/Cmd modifier paths that commit-then-navigate.
        if ctrl_or_cmd {
            match &kp.key {
                KeyInput::Keyboard(Key::Named(NamedKey::Home), _) => {
                    do_commit();
                    let first_id =
                        current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
                    if let Some(id) = first_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                KeyInput::Keyboard(Key::Named(NamedKey::End), _) => {
                    do_commit();
                    let last_id =
                        current_doc.with_untracked(|d| d.as_ref()?.blocks.last().map(|b| b.id));
                    if let Some(id) = last_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                _ => return None,
            }
        }

        // PageUp / PageDown — 10-block jump. Commit first.
        if matches!(
            &kp.key,
            KeyInput::Keyboard(Key::Named(NamedKey::PageUp | NamedKey::PageDown), _)
        ) {
            let forward = matches!(
                &kp.key,
                KeyInput::Keyboard(Key::Named(NamedKey::PageDown), _)
            );
            do_commit();
            let target_id = current_doc.with_untracked(|maybe| {
                let d = maybe.as_ref()?;
                let i = d.blocks.iter().position(|b| b.id == block_id)?;
                let j = if forward {
                    (i + 10).min(d.blocks.len().saturating_sub(1))
                } else {
                    i.saturating_sub(10)
                };
                d.blocks.get(j).map(|b| b.id)
            });
            if let Some(id) = target_id {
                defer_focus(focus_target, id);
            }
            return Some(CommandExecuted::Yes);
        }

        match &kp.key {
            // Enter (no mods) — insert newline, no block split.
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if !shift => {
                editor_sig.get_untracked().receive_char("\n");
                return Some(CommandExecuted::Yes);
            }

            // Shift+Enter — same as Enter (soft line break).
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => {
                editor_sig.get_untracked().receive_char("\n");
                return Some(CommandExecuted::Yes);
            }

            // Shift+Tab — consume, no-op (defer outdent to a follow-up).
            // Must come BEFORE the unguarded Tab arm so the shift guard
            // is evaluated first.
            KeyInput::Keyboard(Key::Named(NamedKey::Tab), _) if shift => {
                return Some(CommandExecuted::Yes);
            }

            // Tab — insert two spaces.
            KeyInput::Keyboard(Key::Named(NamedKey::Tab), _) => {
                editor_sig.get_untracked().receive_char("  ");
                return Some(CommandExecuted::Yes);
            }

            // Backspace.
            KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
                let offset =
                    editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
                if offset > 0 {
                    return None; // default handler deletes one char
                }
                // Offset is 0.
                let body_is_empty = editor_sig.with_untracked(|ed| ed.doc().text().is_empty());
                if body_is_empty {
                    // Empty body at offset 0 — delete the block.
                    do_commit();
                    on_action(BlockAction::Delete { block_id });
                    return Some(CommandExecuted::Yes);
                }
                // Non-empty body at offset 0 — keyboard isolation.
                return Some(CommandExecuted::Yes);
            }

            // ArrowUp at first vline — jump to previous block.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
                let on_first = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
                });
                if !on_first {
                    return None; // within-block navigation
                }
                do_commit();
                let prev_id = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
                });
                if let Some(id) = prev_id {
                    defer_focus(focus_target, id);
                }
                return Some(CommandExecuted::Yes);
            }

            // ArrowDown at last vline — jump to next block.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
                let on_last = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                    vline.0 == ed.last_vline().0
                });
                if !on_last {
                    return None;
                }
                do_commit();
                let next_id = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    d.blocks.get(i + 1).map(|b| b.id)
                });
                if let Some(id) = next_id {
                    defer_focus(focus_target, id);
                }
                return Some(CommandExecuted::Yes);
            }

            // Anything else — fall through to the shared default handler.
            _ => None,
        }
    })
}

/// Set `focus_target` on the next event-loop tick rather than immediately.
///
/// Private duplicate of `list.rs`'s version. Will be unified with the shared
/// `focus::defer_focus` in Stage 3.
fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
    floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
        focus_target.set(Some(target_id));
    });
}

/// Build the editable code block view.
///
/// Creates a single `BlockEditorState` from the code body (as a synthetic
/// `InlineRun`), mounts it via `mount_block_editor` with a code-specific
/// commit closure and structural-key callback, and wraps everything in a
/// styled frame with a corner lang label.
#[allow(clippy::too_many_arguments)]
pub fn editable_code_view(
    body: &str,
    lang: &str,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    let cx = Scope::current();

    // Build editor state from a single synthetic InlineRun carrying the body.
    // Code has no inline styles, so InlineRun::plain (default bold/italic/
    // code = false, link = None) is exactly right.
    let runs = vec![InlineRun::plain(body)];
    let state = build_block_editor(cx, &runs, CODE_FONT_SIZE);
    let editor_sig = state.editor_sig;
    let text_sig = state.text_sig;

    // Code-specific commit closure: read buffer, compare with model, emit
    // EditBlockBody { Code } on diff.
    let commit_on_action = on_action.clone();
    let commit = make_code_commit(
        block_id,
        editor_sig,
        commit_on_action,
        current_doc,
    );

    // Code-specific structural-key callback.
    let structural_key = make_code_structural_key(
        block_id,
        editor_sig,
        on_action.clone(),
        focus_target,
        current_doc,
        commit,
    );

    // Mount the editor. slash_eligible: false — "/" does not open the slash
    // menu inside a code body.
    let editor_view = mount_block_editor(
        state,
        block_id,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
        commit,
        structural_key,
        /* slash_eligible */ false,
    );

    // Lang label in the top-right corner.
    let lang_label_text = lang.to_string();
    let lang_label = label(move || lang_label_text.clone()).style(|s| {
        s.color(Color::rgb8(120, 120, 120))
            .font_size(11.)
            .padding_horiz(8.)
            .padding_vert(2.)
    });

    let header = h_stack((empty().style(|s| s.flex_grow(1.0)), lang_label));

    // Body: wrap the mounted editor in a stack that hides the gutter and
    // applies monospace font + padding. Height tracks the visual line count.
    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    let body_view = stack((editor_view,))
        .style(move |s| {
            let lines = text_sig.get().split('\n').count().max(1) as f32;
            s.class(GutterClass, |s| s.hide())
                .font_family(MONO_FAMILY.to_string())
                .font_size(13.)
                .padding(10.)
                .width_full()
                .height(lines * line_height)
        });

    // Outer frame: same styling as the read-only `code::render_code`.
    stack((header, body_view))
        .style(|s| {
            s.flex_col()
                .width_full()
                .background(Color::rgb8(245, 245, 245))
                .border_radius(4.)
                .border(1.)
                .border_color(Color::rgb8(220, 220, 220))
                .margin_vert(8.)
        })
        .into_any()
}
```

### Step 1.3: Add `pub mod code_editor;` to `mod.rs`

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add `pub mod code_editor;`
after `pub mod code;` (around line 7). Do NOT remove `pub mod code;` — the
read-only renderer stays for now per spec Section 5.

Current (lines 7-15):
```rust
pub mod code;
pub mod editor_registry;
pub mod heading;
pub mod inline_editor;
pub mod list;
pub mod opaque;
pub mod paragraph;
pub mod plugin;
pub mod style_span;
```

Replace with:
```rust
pub mod code;
pub mod code_editor;
pub mod editor_registry;
pub mod heading;
pub mod inline_editor;
pub mod list;
pub mod opaque;
pub mod paragraph;
pub mod plugin;
pub mod style_span;
```

### Step 1.4: Wire the code widget into `editor_registry.rs`

In `crates/lopress-editor/src/ui/blocks/editor_registry.rs`, add the import
and the `"code"` arm.

Find (line 13):
```rust
use crate::ui::blocks::list;
```

Replace with:
```rust
use crate::ui::blocks::{code_editor, list};
```

Find (lines 33-37):
```rust
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        _ => None,
    }
}
```

Replace with:
```rust
pub fn editor_for(key: &str) -> Option<EditorWidget> {
    match key {
        "list" => Some(list_editor_widget),
        "code" => Some(code_editor_widget),
        _ => None,
    }
}
```

After the closing `}` of `list_editor_widget` (around line 48), add:
```rust

/// The `editor = "code"` widget. Extracts `body` from the block's
/// `BlockBody::Code`, reads `lang` from the manifest-driven `PluginMeta.attrs`,
/// and calls `code_editor::editable_code_view`.
fn code_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::Code(body) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
    let lang = ctx
        .block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("lang"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    code_editor::editable_code_view(
        body,
        lang,
        ctx.block.id,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
        Rc::clone(&ctx.on_undo),
        Rc::clone(&ctx.on_redo),
    )
}
```

### Step 1.5: Add the registry test

Extend the existing `editor_for_resolves_list_and_rejects_unknown` test in
`editor_registry.rs` (lines 52-56) to also assert `"code"` resolves. Replace
the entire test function:

```rust
    #[test]
    fn editor_for_resolves_list_and_rejects_unknown() {
        assert!(editor_for("list").is_some());
        assert!(editor_for("code").is_some());
        assert!(editor_for("paragraph").is_none());
        assert!(editor_for("bogus").is_none());
    }
```

### Step 1.6: Run tests — skeleton passes

```bash
cargo test -p lopress-editor 2>&1
```

Expected: all pass, including the extended registry test.

### Step 1.7: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 1.8: Commit

```bash
git add crates/lopress-editor/src/ui/blocks/code_editor.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "$(cat <<'EOF'
feat(editor): add editable code block widget skeleton and registry wiring

Creates code_editor.rs with a minimal editable_code_view that builds
BlockEditorState from a synthetic InlineRun, mounts via mount_block_editor
with no-op commit and structural-key callbacks, and renders a styled frame
with a corner lang label. Wires code_editor_widget into editor_registry.rs
with the "code" arm and extends the registry test.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 1.5: Self-review the widget file before committing Task 1

> **Note on plan structure:** Task 1 above delivers the entire widget in one
> code dump (skeleton + commit closure + structural key + styling + registry
> wiring), then commits. The earlier draft of this plan split that work into
> Tasks 2-4, but each was a "verify Task 1's code matches snippet X" task with
> no separate commit — pure noise. They've been collapsed into this self-
> review checklist. **Do this checklist immediately AFTER writing the file
> but BEFORE running `cargo test` in Step 1.6** — it catches the common
> mistakes faster than the compiler.

Walk the file you just wrote and tick each item:

- [ ] **Imports:** `use floem::views::editor::Editor;` is present (needed for
      the `RwSignal<Editor>` annotation in `make_code_commit` and
      `make_code_structural_key`).
- [ ] **InlineRun construction:** the body is wrapped via
      `InlineRun::plain(body)`, not a struct literal.
- [ ] **Tab arm ordering:** `Shift+Tab` arm (with `if shift` guard) comes
      **BEFORE** the unguarded Tab arm. Guards are evaluated top-down; the
      unguarded arm would otherwise shadow the guarded one.
- [ ] **No doubly-nested `KeyInput::Keyboard`:** every pattern is
      `KeyInput::Keyboard(Key::Named(NamedKey::X), _)` — single level.
- [ ] **All commit-then-navigate paths call `do_commit()` first, then
      `defer_focus(focus_target, target_id)`:** Ctrl+Home, Ctrl+End,
      PageUp, PageDown, ArrowUp at first vline, ArrowDown at last vline,
      Backspace on empty body.
- [ ] **Backspace at offset > 0 returns `None`:** the default handler
      should delete a single char, not the structural key callback.
- [ ] **Backspace at offset 0 of non-empty body returns `Some(Yes)`
      without doing anything:** keyboard isolation. NOT `MergeWithPrev`.
- [ ] **`slash_eligible: false` is passed to `mount_block_editor`:** `/`
      should not open the slash menu inside a code body.
- [ ] **The view returns `AnyView` via `.into_any()` at the end of
      `editable_code_view`.**
- [ ] **No `unused_mut` or `unused_imports`:** read the file once and remove
      any imports that didn't end up used.

If any of these fail, fix inline before committing. No separate commit for
this checklist.

---

## Task 2: Re-point `plugin.rs` fallback

**Why:** The built-in `BlockKind::Code` fallback in `render_body` (plugin.rs
lines 351-353) currently calls `code::render_code` (read-only). Re-point it to
`code_editor::editable_code_view` so plugin-less code blocks (created via
`ChangeType` from the toolbar/slash menu) also get the editable widget. This
is the same pattern the list arm uses.

### Step 2.1: Add `code_editor` to the plugin.rs imports

Find (around line 17):
```rust
use crate::ui::blocks::{code, heading, list, paragraph};
```

Replace with:
```rust
use crate::ui::blocks::{code, code_editor, heading, list, paragraph};
```

### Step 2.2: Re-point the `BlockKind::Code` fallback

Find (lines 351-353):
```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code::render_code(lang, text).into_any()
        }
```

Replace with:
```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(
                text,
                lang,
                block_id,
                on_action,
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
            )
            .into_any()
        }
```

Mirror the list arm's shape at lines 354-364 (the list arm already calls
`list::editable_list_view` with the same parameter pattern).

### Step 2.3: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean. The `code` module is still imported and compiled (it's
referenced in `mod.rs`), but no longer called by the `BlockKind::Code` arm.

### Step 2.4: Run workspace tests

```bash
cargo test --workspace 2>&1
```

Expected: all pass. The Stage 1 round-trip tests still pass (the model end
is unchanged), the registry test still passes.

### Step 2.5: Commit

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "$(cat <<'EOF'
refactor(editor): re-point BlockKind::Code fallback to the editable widget

The built-in code block fallback in render_body now calls
code_editor::editable_code_view instead of code::render_code. This
covers plugin-less code blocks (created via ChangeType from the
toolbar/slash menu) which never go through the registry path.

The read-only code::render_code stays in the tree (spec Section 5
defers its deletion to a follow-up cleanup).

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Manual GUI verification + report

**Why:** The widget is a Floem GUI component — keyboard behavior cannot be
fully automated. This task drives each check through the editor GUI and
reports pass/fail.

### Step 3.1: Launch the editor

```bash
cd C:\Users\corpo\Documents\projects\lopress
cargo run -p lopress-editor 2>&1
```

Or, if using the debug HTTP control server:
```bash
# Start the editor with debug ctrl server
cargo run -p lopress-editor
# Then use the driving-lopress-editor skill to send actions via 127.0.0.1:7878
```

### Step 3.2: Drive the manual verification checklist

For each check below, report **PASS** or **FAIL** with a brief note.

1. **Type into the body.** Click into a code block's body area. Type
   characters. Characters should appear in the editor. (PASS/FAIL)

2. **Enter inserts `\n`.** Place the cursor in the middle of a line.
   Press Enter. A new line should appear, the cursor should move to the
   next visual line, and the code block should NOT split into two blocks.
   (PASS/FAIL)

3. **Tab inserts two spaces.** Place the cursor at any position. Press
   Tab. Two spaces should be inserted at the cursor position.
   (PASS/FAIL)

4. **Shift+Enter inserts newline.** Press Shift+Enter. Same as Enter —
   a newline is inserted, no block split. (PASS/FAIL)

5. **Backspace at offset 0 of an empty body deletes the block.** Create
   a new code block, ensure the body is empty, place the cursor at offset
   0, press Backspace. The entire code block should be deleted.
   (PASS/FAIL)

6. **Backspace at offset 0 of a non-empty body is keyboard-isolated.**
   Create a code block with some text, place the cursor at offset 0,
   press Backspace. Nothing should happen — the code block stays, no
   content is lifted into the previous block. (PASS/FAIL)

7. **ArrowUp at first vline jumps to previous block.** Place the cursor
   at the first visual line of the code body, press ArrowUp. The caret
   should jump to the previous block (or the block before the code block
   in the document). (PASS/FAIL)

8. **ArrowDown at last vline jumps to next block.** Place the cursor
   at the last visual line of the code body, press ArrowDown. The caret
   should jump to the next block. (PASS/FAIL)

9. **Ctrl+Home jumps to first block.** Press Ctrl+Home. The caret
   should jump to the first block in the document. (PASS/FAIL)

10. **Ctrl+End jumps to last block.** Press Ctrl+End. The caret should
    jump to the last block in the document. (PASS/FAIL)

11. **PageUp jumps 10 blocks back.** Place the cursor in a code block
    that is more than 10 blocks from the start. Press PageUp. The caret
    should jump to approximately block 10 positions back (clamped to 0).
    (PASS/FAIL)

12. **PageDown jumps 10 blocks forward.** Place the cursor in a code
    block that is more than 10 blocks from the end. Press PageDown. The
    caret should jump to approximately block 10 positions forward
    (clamped to len-1). (PASS/FAIL)

13. **Lang label updates.** Edit the `lang` attribute in the attr form
    (Stage 1 wires `EditAttrs` for code blocks). The corner label should
    update immediately to show the new language. (PASS/FAIL)

14. **Save and reopen.** Edit the code block body and lang. Save the
    document. Close and reopen. The body text and lang should be preserved.
    (PASS/FAIL)

### Step 3.3: Final workspace verification

```bash
cargo test --workspace 2>&1
```

Expected: all pass.

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 3.4: Report

Report back with:
- A table of the 14 manual checks with PASS/FAIL per check
- Any issues found (with reproduction steps)
- Confirmation that `cargo test --workspace` and `cargo check --workspace` are clean

No commit needed (or a tiny `docs(plans):` commit checking off the plan if
you want a marker).

---

## Done when

- `cargo test --workspace` passes with zero failures
- `cargo check --workspace` is clean
- Two commits land in order:
  1. `feat(editor): add editable code block widget skeleton and registry wiring` (Task 1)
  2. `refactor(editor): re-point BlockKind::Code fallback to the editable widget` (Task 2)
- All 14 manual GUI verification checks are reported (PASS/FAIL)
- The `code` module is NOT deleted (spec Section 5 defers its removal)
- `ui/mod.rs` is NOT modified (Stage 3 territory)


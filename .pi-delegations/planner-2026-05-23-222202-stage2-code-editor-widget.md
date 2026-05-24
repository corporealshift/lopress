You are an implementation planner. Read the spec yourself and decide the
decomposition: map out which files to create or modify and what each is
responsible for, then break the work into tasks — each a self-contained,
independently testable change — in a sensible order. Expand every task into
bite-sized steps: write the failing test, run it to confirm it fails, write
the minimal implementation with real code, run the test to confirm it passes,
commit. Every code step must contain actual, complete code — never "TBD",
never "add error handling", never "similar to Task N". Use exact file paths
and exact commands with their expected output.

## Write this implementation plan

Read the spec at
`docs/superpowers/specs/2026-05-23-code-editor-block-and-ui-mod-split-design.md`
— plan **Section 3 (`editor = "code"` widget)** only. Section 4 (the
`ui/mod.rs` decomposition) is a separate stage and OUT of scope for this plan.

**Important context — Stage 1 is executing in parallel.** Sections 1+2 of
the spec (base plugin + load/save mirror) are being implemented right now in
a separate plan: `docs/superpowers/plans/2026-05-23-stage1-code-base-plugin-and-mirror.md`.
Stage 2 (this plan) assumes Stage 1 has landed:
- `base_plugins/code/manifest.toml` exists.
- `PluginRegistry::load_base_plugins` registers `lopress-code`.
- Markdown code blocks load with `block.plugin = Some(_)`, `plugin.attrs["lang"]`
  populated.
- `native_block_to_core` emits code blocks correctly.
- `apply_edit_attrs` mirrors `lang` into `BlockKind::Code.lang`.

The Stage 2 plan's tasks should run **after Stage 1's commits land on
`feat/code-editor-block`**. The implementer (qwen) should rebase / wait until
Stage 1 is committed before starting Stage 2 — note this prominently in the
plan's preamble.

Write the plan to
`docs/superpowers/plans/2026-05-23-stage2-code-editor-widget.md`,
starting with the required header block below. Produce the File Structure map
and the task decomposition yourself, then expand every task into bite-sized
steps. The two Stage 1 / Stage 0 plans
(`docs/superpowers/plans/2026-05-23-stage1-code-base-plugin-and-mirror.md`
and `docs/superpowers/plans/2026-05-23-stage0-rename-code-block-to-code.md`)
are the closest format references — same "for qwen" preamble, same TDD-first
structure, heredoc commit messages, `Co-Authored-By: Qwen` trailer.

## Required plan header

```markdown
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
```

## Scope

Section 3 of the spec, one focused stage. The widget is substantial (~250-
300 lines of new code) but cohesive: one new file, two integration sites
(`editor_registry.rs` and `plugin.rs`'s fallback). Section 4 (`ui/mod.rs`
decomposition) is deliberately deferred to Stage 3 and not in scope here.

### Known temporary duplication

The spec's Section 3 notes that `defer_focus` (the
`exec_after(Duration::ZERO)` helper currently inline in `list.rs`) should be
lifted to `ui/editing/focus.rs` in Section 4. Since Section 4 is a later
stage, **Stage 2 will inline `defer_focus` in `code_editor.rs` as a
private helper**, duplicating list.rs's version. Stage 3 will unify them.
Document this in the plan as a planned temporary duplication, not an
oversight. Do NOT extract `defer_focus` early — keeping the change footprint
small for this stage is more valuable than the brief duplication.

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

## Concrete file inventory (verified — use these in the plan)

### File to create

- **`crates/lopress-editor/src/ui/blocks/code_editor.rs`** — new module
  ~250-300 lines. Exports `editable_code_view`. Contents at a high level:
  - `editable_code_view(body: &str, lang: &str, block_id: BlockId,
    on_action: ActionSink, focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher, current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>, on_redo: Rc<dyn Fn()>) -> AnyView`
  - `fn make_code_commit(...) -> CommitClosure`
  - `fn make_code_structural_key(...) -> StructuralKey`
  - `fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id:
    BlockId)` — private duplicate of list.rs's, will be unified in Stage 3.

### Files to modify

- **`crates/lopress-editor/src/ui/blocks/mod.rs:8-16`** — add
  `pub mod code_editor;` declaration alongside the existing `pub mod code;`
  (don't remove `code` — the read-only renderer stays around for now per
  spec Section 5).

- **`crates/lopress-editor/src/ui/blocks/editor_registry.rs:13-40`** — add
  the code arm to the registry:

  Current:
  ```rust
  use crate::ui::blocks::list;
  ...
  pub fn editor_for(key: &str) -> Option<EditorWidget> {
      match key {
          "list" => Some(list_editor_widget),
          _ => None,
      }
  }
  ```

  Target: import `code_editor` too, add `"code" => Some(code_editor_widget)`
  arm, define `code_editor_widget(ctx: &EditorContext) -> AnyView`. The
  widget extracts `body` from `ctx.block.body` (expects `BlockBody::Code(s)`,
  returns `empty().into_any()` on body/kind mismatch — same defensive
  pattern as `list_editor_widget`), extracts `lang` from
  `ctx.block.plugin.as_ref()?.attrs.get("lang").and_then(Value::as_str)`
  (default `""`), then calls `code_editor::editable_code_view(...)`.

- **`crates/lopress-editor/src/ui/blocks/plugin.rs:351-353`** — re-point the
  built-in `BlockKind::Code` fallback in `render_body` from
  `code::render_code(lang, text)` (read-only) to
  `code_editor::editable_code_view(text, lang, block_id, on_action,
  focus_target, focus_pub, current_doc, on_undo, on_redo)`. This covers
  plugin-less code blocks (created via `ChangeType` from the toolbar/slash
  menu), which never go through the registry path. The list arm at
  lines 354-364 uses the same re-point pattern — mirror its shape.

- **`crates/lopress-editor/src/ui/blocks/editor_registry.rs`** (tests
  module at bottom) — extend the existing `editor_for_resolves_list_and_rejects_unknown`
  test to also assert `editor_for("code").is_some()`, or add a sibling
  test. Planner picks; sibling is cleaner.

### Files NOT to modify

- `crates/lopress-editor/src/ui/blocks/code.rs` — the read-only renderer.
  Spec Section 5 explicitly defers its deletion to a follow-up cleanup.
  Leave it in place; nothing should reference it after this stage (the
  built-in arm in plugin.rs is the last caller; we re-point that).
- `crates/lopress-editor/src/ui/mod.rs` — Stage 3 territory.
- Anything in `lopress-core`, `lopress-plugin`, `lopress-build` — already
  done in Stage 0 and Stage 1.

## The widget — implementation reference

Pi should consult `crates/lopress-editor/src/ui/blocks/list.rs` extensively
while writing this plan — the code widget is structurally a simpler list-
item editor. Specifically:

- `build_block_editor` is at `inline_editor.rs:72-112`. It takes
  `&[InlineRun]` and font size; returns a `BlockEditorState`. For code, pass
  a single-element slice with `InlineRun { text: body.to_string(), flags:
  0, link: None }` and font size `13` (the code body size). Inspect
  `crates/lopress-editor/src/model/types.rs` for the `InlineRun` constructor
  shape — there's likely an `InlineRun::plain(&str)` or similar; planner
  should grep for it. If not, build the struct literal.

- `mount_block_editor` is at `inline_editor.rs:184` onward. Signature:
  ```rust
  pub fn mount_block_editor(
      state: BlockEditorState,
      block_id: BlockId,
      publish_block_id: BlockId,    // same as block_id for code (no nesting)
      on_action: ActionSink,
      focus_target: RwSignal<Option<BlockId>>,
      focus_pub: FocusPublisher,
      current_doc: RwSignal<Option<EditorDoc>>,
      on_undo: Rc<dyn Fn()>,
      on_redo: Rc<dyn Fn()>,
      _commit: CommitClosure,
      structural_key: StructuralKey,
      slash_eligible: bool,         // false for code
  ) -> impl IntoView
  ```

- `CommitClosure = Rc<dyn Fn()>`. The code commit closure should:
  1. Read editor text: `editor_sig.with_untracked(|ed| String::from(&ed.doc().text()))`.
  2. Compare against the model's current body for `block_id`:
     ```rust
     let differs = current_doc.with_untracked(|maybe| {
         maybe.as_ref()
             .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
             .map(|b| !matches!(&b.body, BlockBody::Code(s) if s == &live_text))
             .unwrap_or(false)
     });
     ```
  3. If `differs`, emit `BlockAction::EditBlockBody { block_id, new_body:
     BlockBody::Code(live_text) }`.

  Mirrors list's `commit_live_if_changed` in shape but simpler (one String
  vs. a Vec<ListItem>).

- `StructuralKey = Rc<dyn Fn(&KeyPress, Modifiers) -> Option<CommandExecuted>>`.
  Returning `Some(Yes)` short-circuits the shared default handler; `None`
  falls through. The list editor's `make_list_structural_key` at
  `list.rs:287-491` is the closest reference — copy its overall structure
  (Ctrl/Cmd-first, PageUp/Down second, then a big match on the key).

### The code keymap — exact spec

This is verbatim from spec Section 3. The plan should reproduce this table
in Task 3 (the structural-key implementation):

| Key | Behaviour |
|---|---|
| Enter (no mods) | Consume. Call `editor_sig.get_untracked().receive_char("\n")`. Return `Some(Yes)`. Block is NOT split. |
| Shift+Enter | Same as Enter. |
| Tab | Consume. Call `editor_sig.get_untracked().receive_char("  ")` (two spaces). Return `Some(Yes)`. |
| Shift+Tab | Consume; no-op (return `Some(Yes)`). Defer outdent to a follow-up. |
| Backspace at offset 0 of an empty body | Commit (the commit closure is a no-op when nothing changed). Emit `BlockAction::Delete { block_id }`. Return `Some(Yes)`. |
| Backspace at offset 0 of a non-empty body | Return `Some(Yes)` without doing anything (keyboard isolation — don't lift code into the previous block, don't `MergeWithPrev`). |
| Backspace at offset > 0 | Return `None` (let default handler delete one char). |
| ArrowUp at first vline | Commit. `defer_focus` to the previous block's id. Return `Some(Yes)`. |
| ArrowUp not at first vline | Return `None` (within-block navigation). |
| ArrowDown at last vline | Commit. `defer_focus` to the next block's id. Return `Some(Yes)`. |
| ArrowDown not at last vline | Return `None`. |
| Ctrl/Cmd + Home | Commit. `defer_focus` to first block's id. Return `Some(Yes)`. |
| Ctrl/Cmd + End | Commit. `defer_focus` to last block's id. Return `Some(Yes)`. |
| PageUp | Commit. `defer_focus` to block 10 positions back (clamped to 0). Return `Some(Yes)`. |
| PageDown | Commit. `defer_focus` to block 10 positions forward (clamped to len-1). Return `Some(Yes)`. |
| Everything else | Return `None` (default handler — character insertion, in-block navigation, etc.). |

**Why every navigation path needs explicit handling in the code callback:**
the shared default handler in `inline_editor.rs::handle_key` (lines ~448-
588) calls `commit_from_editor` on these key paths, which emits
`EditBlockBody { new_body: BlockBody::Inline(runs) }`. For a code body
that's the wrong body shape (`Code(String)`, not `Inline(Vec<InlineRun>)`).
Intercepting and using our own commit closure (which emits
`BlockBody::Code(...)`) is the whole point of the structural-key override.

The list editor does this same thing — read `list.rs:287-491` for the
existing implementation pattern.

### Helpers

- Vline checks (first/last) — same pattern as list.rs:445-446 and 467-470:
  ```rust
  let on_first = editor_sig.with_untracked(|ed| {
      let offset = ed.cursor.with_untracked(|c| c.offset());
      ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
  });
  let on_last = editor_sig.with_untracked(|ed| {
      let offset = ed.cursor.with_untracked(|c| c.offset());
      let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
      vline.0 == ed.last_vline().0
  });
  ```

- `defer_focus` (inline private copy of list.rs:115-119):
  ```rust
  fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId) {
      floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
          focus_target.set(Some(target_id));
      });
  }
  ```

### View styling

A `v_stack` of `[header, body]`. Header is right-aligned lang label
(font_size 11, grey). Body is the mounted editor wrapped in a `stack` that
hides the gutter (`GutterClass -> hide`, see list.rs:271-279) and sets:
- `font_family(MONO_FAMILY.to_string())` — `MONO_FAMILY` is at
  `paragraph.rs:21`.
- `font_size(13.)`
- `padding(10.)`
- `width_full()`
- height = `lines * line_height` where `lines = text_sig.get().split('\n')
  .count().max(1)` — same shape as list.rs:274-278.

Outer frame:
- `background(Color::rgb8(245, 245, 245))`
- `border_radius(4.)`
- `border(1.).border_color(Color::rgb8(220, 220, 220))`
- `margin_vert(8.)`

These match the existing `code::render_code` styling at `code.rs:13-37`.
The body inside the frame is the new editor instead of a static `text(...)`.

## Suggested task decomposition (planner may revise)

This is the smallest sensible split. Pi may merge or further split per the
right-sizing rules in the writing-plans skill.

1. **Skeleton + registry wiring** — create `code_editor.rs` with a minimal
   `editable_code_view` that builds state, mounts with a no-op commit
   closure and a no-op structural-key, and renders a basic frame. Wire it
   into `editor_registry.rs` (add `code_editor_widget` and the `"code"`
   arm). Add `editor_for_resolves_code` test. Run `cargo test`. Commit.
2. **Code commit closure** — replace the no-op commit closure with the real
   one (read editor buffer → compare with model → emit
   `EditBlockBody { Code }` on diff). No new tests strictly required — the
   existing Stage 1 round-trip tests cover the model end. Commit.
3. **Structural-key callback — the full keymap** — implement the entire
   table above in `make_code_structural_key`. Includes the inline private
   `defer_focus` helper. Commit.
4. **View styling** — header with lang label, frame, monospace body,
   height-from-line-count. Replace the basic frame from Task 1 with the
   spec-matching styling. Commit.
5. **Re-point `plugin.rs` fallback** — change the `BlockKind::Code` arm in
   `render_body` to call `code_editor::editable_code_view` instead of
   `code::render_code`. Run `cargo check --workspace`. Commit.
6. **Manual GUI verification + report** — run the editor; drive each of
   the bullet-list checks from the "Manual verification" section. Report
   pass/fail per check. No commit (or a tiny `docs(plans):` commit checking
   off the plan if you want a marker).

The planner may merge Task 4 (styling) into Task 1 (skeleton) if the
skeleton's basic frame styling is already close to the final shape.

## Things the planner should not do

- Do not delete `crates/lopress-editor/src/ui/blocks/code.rs` — spec
  Section 5 defers that.
- Do not modify `ui/mod.rs` — Stage 3.
- Do not add syntax highlighting, auto-indent, bracket matching, or
  Shift+Tab outdent — explicit non-goals.
- Do not extract `defer_focus` to a new shared module — planned temporary
  duplication; Stage 3 unifies.
- Do not stamp PluginMeta inside `EditorBlock::code` — the plugin.rs
  fallback handles plugin-less code blocks just fine via the re-point.
- Do not change the toolbar's Code button or the slash-menu Code entry —
  out of scope (they produce plugin-less code blocks today; the fallback
  re-point handles those).

## Done when

The plan file exists at the path above, maps the file structure (with
verified paths and line numbers from the inventory above), decomposes the
work into ordered tasks each producing one commit (six total), expands every
task into bite-sized steps with complete code blocks (no "TBD"/"similar to
Task N"), and ends with a manual-verification task that lists the bullet
checks above as the acceptance criteria.

## On completion

Reply with a concise summary: the plan file path and the list of task titles.

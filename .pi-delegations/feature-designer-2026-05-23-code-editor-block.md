You are a feature designer. You are writing a design specification document from a
complete set of decisions that have already been made and approved. Every design
question is settled — do not invent requirements, change scope, or add features.
You have latitude on how to structure the document and how to word it: organize the
sections clearly, write in plain technical prose, and make the spec easy to read.
If something is genuinely missing or contradictory, note it explicitly at the end
under "Open questions for Claude" rather than guessing.

## Write this spec document

Write the design specification to
`docs/superpowers/specs/2026-05-23-code-editor-block-and-ui-mod-split-design.md`.

An existing spec under `docs/superpowers/specs/` is a useful format reference — read
one if helpful. `docs/superpowers/specs/2026-05-15-editable-list-base-plugin-design.md`
is the closest precedent because this spec mirrors that pattern for the code block.

## Approved design (full substance)

This spec covers two related but separable changes shipping together:

1. The **code editor block** — turning the read-only code block into an editable
   block using the existing base-plugin / editor-registry infrastructure that the
   list block already uses.
2. A **decomposition of `crates/lopress-editor/src/ui/mod.rs`** — a pure refactor
   to break the ~666-line file (with a ~400-line `editing_view` function) into
   small sibling modules organized by responsibility.

The refactor is in scope because the editor pane work touches `ui/mod.rs` enough
to make the size pain obvious. The two changes share a branch but should be
distinguishable as separate concerns in the spec.

### Section 1 — Code base plugin

Create `base_plugins/code/manifest.toml`, embedded at compile time the same way
`base_plugins/list/manifest.toml` already is (via `include_str!` in
`PluginRegistry::load_base_plugins`). Add a second `include_str!` entry; no new
mechanism needed.

Manifest:

```toml
name    = "lopress-code"
version = "0.1.0"

[[blocks]]
name    = "code"
editor  = "code"
native  = "code"
builtin = true

[blocks.attrs]
lang = { type = "string", ui = "text" }
```

`from_core` already populates `PluginMeta` for any block whose type name is
registered, so existing markdown code blocks automatically pick up
`plugin.attrs.lang = "<lang>"` once `lopress-code` is in the registry — no
explicit per-block migration code.

### Section 2 — `BlockKind::Code` ↔ attrs mirror

This mirrors how the list spec handles `ordered` (the "Level C seam"). `lang`
must flow between the model's `BlockKind::Code { lang: String }` field and the
plugin attrs map:

- **`from_core` (load):** when materializing a code block, after building
  `BlockKind::Code { lang }`, copy `lang` into `plugin.attrs["lang"]` so the attr
  form reads the right value at first paint.
- **`to_core` (save):** when serializing a code block, read
  `plugin.attrs["lang"]` as the source of truth, write it back into
  `BlockKind::Code.lang`, then go through the existing code arm in `to_core`.
  The code arm — like the list arm — must skip the plugin serialization path
  (markdown code fences are the canonical on-disk representation, not a plugin
  block).
- **`EditAttrs` handler:** when an `EditAttrs` action lands on a code block,
  after the attrs map is replaced, mirror `attrs["lang"]` back into the model's
  `BlockKind::Code.lang` so a subsequent save serializes correctly without
  waiting for a save → reload round-trip.

`BlockKind::Code` stays as the serialization-of-record. This is intentionally
symmetric with the list precedent — no new mechanism, no new abstractions.

### Section 3 — `editor = "code"` widget

A new file `crates/lopress-editor/src/ui/blocks/code_editor.rs` holds the
editable widget. The existing `code.rs` (read-only renderer) stays in the tree
for now and will be deleted in a follow-up; nothing should depend on it once
the registry routes code blocks to the new widget.

**Construction.** The widget builds one `BlockEditorState` via the existing
`build_block_editor`, fed a single synthetic `InlineRun { text: body, flags:
0, link: None }` where `body` is the `String` from `BlockBody::Code`. The
spans signal is created empty and stays empty — code has no inline styles.

**Mount.** The widget mounts via the existing `mount_block_editor` with:

- `slash_eligible: false` — the `/` key should not open the slash menu inside a
  code body.
- A **code commit closure** that:
  1. reads `editor_sig.with_untracked(|ed| String::from(&ed.doc().text()))` to
     get the current buffer as a `String`,
  2. compares it against the model's current `BlockBody::Code(s)` (read from
     `current_doc`),
  3. when they differ, emits `BlockAction::EditBlockBody { block_id, new_body:
     BlockBody::Code(text) }`.

  Equivalent to list's `commit_live_if_changed` but a `String` comparison
  instead of a `Vec<ListItem>` comparison.

- A **code structural-key callback** implementing this keymap:

  | Key | Behaviour |
  |---|---|
  | Enter (no mods) | Consume; call `editor_sig.receive_char("\n")` so a newline goes into the body. Block is NOT split. |
  | Shift+Enter | Same as Enter (newline). |
  | Tab | Consume; call `editor_sig.receive_char("  ")` (two spaces). |
  | Shift+Tab | Consume; no-op (defer outdent to a follow-up). |
  | Backspace at offset 0 of an empty body | Commit (no-op since empty), then emit `BlockAction::Delete { block_id }`. |
  | Backspace at offset 0 of a non-empty body | Consume; no-op (keyboard isolation — don't lift code into the previous block). |
  | ArrowUp at first vline | Commit, then `defer_focus` to the previous block's id. |
  | ArrowDown at last vline | Commit, then `defer_focus` to the next block's id. |
  | Ctrl/Cmd + Home | Commit, then `defer_focus` to first block. |
  | Ctrl/Cmd + End | Commit, then `defer_focus` to last block. |
  | PageUp / PageDown | Commit, then `defer_focus` to the block 10 positions away (clamped). |
  | Everything else | Return `None` and fall through to the shared default handler. |

  The "commit" step is the code commit closure above. The `defer_focus` helper
  is the same pattern the list editor uses (set focus on the next event-loop
  tick via `floem::action::exec_after(Duration::ZERO)`); it should be lifted to
  a small shared utility (see Section 4's `focus.rs`) so both list and code
  call it.

**View styling.** A `v_stack` of `[header, body]` matching the existing
read-only `code.rs` look:

- Header: corner-aligned `lang` label, small grey text (font_size 11).
- Body: `editor_view` wrapped in a `stack` that hides the gutter (`GutterClass
  -> hide`) and sets:
  - `font_family(MONO_FAMILY)`
  - `font_size(13.)`
  - `padding(10.)`
  - `width_full()`
  - height = `lines * line_height` where `lines = text_sig.get().split('\n')
    .count().max(1)` (same shape as the list-item height calc).
- Frame: `background(rgb8(245,245,245))`, `border_radius(4.)`,
  `border(1.).border_color(rgb8(220,220,220))`, `margin_vert(8.)`. These
  match `code.rs` today.

**Registry entry.** In
`crates/lopress-editor/src/ui/blocks/editor_registry.rs`:

```rust
match key {
    "list" => Some(list_editor_widget),
    "code" => Some(code_editor_widget),
    _ => None,
}
```

`code_editor_widget(ctx: &EditorContext) -> AnyView` extracts `body` from
`ctx.block.body` (expected `BlockBody::Code(s)`), reads `lang` from
`ctx.block.plugin.as_ref()?.attrs.get("lang")` (string, default `""`), and
calls the `editable_code_view(...)` exported from `code_editor.rs`.

**Plugin.rs fallback re-point.** In `crates/lopress-editor/src/ui/blocks/
plugin.rs`'s `render_body` fallback `match`, the `BlockKind::Code` arm gets
re-pointed at `code_editor::editable_code_view(...)` instead of
`code::render_code(...)`. This is the same pattern list uses — covers code
blocks created via `ChangeType` (which produces `plugin: None` so the
registry path doesn't fire).

### Section 4 — `ui/mod.rs` decomposition

Pure move-and-rename refactor of `crates/lopress-editor/src/ui/mod.rs`. No
behaviour change. Tests stay green without modification.

New module tree under `crates/lopress-editor/src/ui/`:

```
ui/
  mod.rs              -- root_view, StateTag, MAX_RECENTS only (~80 lines)
  editing/
    mod.rs            -- editing_view: assembles the pieces (~80 lines)
    focus.rs          -- focus_block_for, focus_after_apply, defer_focus
    pane_key.rs       -- KindTag, kind_tag, build_pane_key closure factory
    action_sink.rs    -- build_action_sink(...) returning the on_action Rc<dyn Fn>
    undo_redo.rs      -- build_undo(...), build_redo(...) each returning Rc<dyn Fn()>
    save_pipeline.rs  -- mark_dirty builder + start_save_debounce + status poll glue
    new_doc.rs        -- DocKind, make_new_doc_action
    ctrl_wire.rs      -- #[cfg(debug_assertions)] debug-HTTP-server signal/effect wiring
```

Each module exports plain functions that take the signals they need as
arguments and return a closure or start an effect. There is no shared state
struct — signals stay owned by `editing_view`'s body so Floem's `Rc<dyn Fn>`
ergonomics are not disturbed. Function signatures are designed to be obvious
from inspection (no module-private types leaked into the public surface).

**Per-module surface area:**

- `focus.rs`:
  - `pub fn focus_block_for(action: &BlockAction) -> Option<BlockId>` (moved verbatim).
  - `pub fn focus_after_apply(doc: Option<&EditorDoc>, action: &BlockAction) -> Option<BlockId>` (moved verbatim).
  - `pub fn defer_focus(focus_target: RwSignal<Option<BlockId>>, target_id: BlockId)` — the `exec_after(Duration::ZERO)` helper currently inline in `list.rs`. List and code both call this version after extraction.

- `pane_key.rs`:
  - `KindTag` enum and `kind_tag` fn (moved verbatim).
  - `pub fn build_pane_key(current_doc: RwSignal<Option<EditorDoc>>) -> impl Fn() -> Option<Vec<(BlockId, KindTag, bool)>> + Copy` — the `pane_key` closure factory.

- `action_sink.rs`:
  - `pub fn build_action_sink(current_doc, focus_target, slash_menu_open, undo_stack, mark_dirty) -> ActionSink` — encapsulates the entire `on_action` closure currently inline in `editing_view`. Inputs are all `RwSignal<_>` / `Rc<dyn Fn()>`.

- `undo_redo.rs`:
  - `pub fn build_undo(undo_stack, current_doc, focus_target, mark_dirty) -> Rc<dyn Fn()>`.
  - `pub fn build_redo(undo_stack, current_doc, focus_target, mark_dirty) -> Rc<dyn Fn()>`.

- `save_pipeline.rs`:
  - `pub struct SavePipeline { mark_dirty, dirty_sig, save_error_sig, build_status_sig, serve_status_sig }` — a plain bag of signals returned by the builder (no methods that hold state). This is the one struct in the split, used purely so `editing_view` can hold the values together for the footer call.
  - `pub fn start_save_pipeline(editing: Rc<RefCell<Option<EditingState>>>, current_doc: RwSignal<Option<EditorDoc>>) -> SavePipeline` — creates all the signals, spawns the `debounce_action`, and starts the build/serve status polls (using the existing `start_build_status_poll` and `start_serve_status_poll` in `ui/footer.rs`).

- `new_doc.rs`:
  - `DocKind` enum and `make_new_doc_action` (moved verbatim).

- `ctrl_wire.rs` (`#[cfg(debug_assertions)]` gated):
  - `pub fn wire_ctrl(ctrl, current_doc, current_path, on_action)` — the entire `if let Some((ctrl_handle, ctrl_action_rx)) = ctrl { ... }` block today inside `editing_view`.

**`editing_view` after the split.** Sketch:

```rust
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    #[cfg(debug_assertions)] ctrl: Option<(CtrlHandle, Receiver<CtrlActionEnvelope>)>,
) -> impl IntoView {
    // 1. Workspace + path signals.
    let workspace_signal = RwSignal::new(initial_workspace(&editing));
    let current_path: RwSignal<Option<PathBuf>> = RwSignal::new(None);

    // 2. Undo + focus + slash + dnd signals.
    let undo_stack = RwSignal::new(UndoStack::new());
    let focus_target: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let slash_menu_open: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let dnd = DndState::new();

    // 3. Save pipeline (signals + polling + debounce).
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);

    // 4. Action sink + undo/redo closures.
    let on_action = action_sink::build_action_sink(
        current_doc, focus_target, slash_menu_open, undo_stack, Rc::clone(&save.mark_dirty),
    );
    let on_undo = undo_redo::build_undo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));
    let on_redo = undo_redo::build_redo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));

    // 5. Sidebar + new-doc actions.
    let sidebar = sidebar_view(workspace_signal, current_path, on_open(...),
        new_doc::make_new_doc_action(..., DocKind::Post),
        new_doc::make_new_doc_action(..., DocKind::Page));

    // 6. Editor pane.
    let pane_key = pane_key::build_pane_key(current_doc);
    let editor = dyn_container(pane_key, ...).style(...);

    // 7. Inspector + footer.
    let inspector = inspector_view(current_doc, current_path, Rc::clone(&save.mark_dirty));
    let footer = footer_view(save.build_status_sig, save.dirty_sig, save.save_error_sig,
        current_doc, save.serve_status_sig);

    // 8. Debug ctrl wiring.
    #[cfg(debug_assertions)]
    if let Some(c) = ctrl { ctrl_wire::wire_ctrl(c, current_doc, current_path, on_action.clone()); }

    // 9. Assembly.
    let columns = h_stack((sidebar, editor, inspector)).style(...);
    stack((columns, footer)).style(...).on_event_stop(WindowClosed, |_| { ... })
}
```

Target line count for `editing_view`: ~80 lines.

### Section 5 — Out of scope (explicit YAGNI)

- Syntax highlighting in the code body.
- Auto-indent on Enter (copying prior line's indent).
- Bracket matching, auto-close pairs.
- Shift+Tab outdent.
- Curated language dropdown (deliberately deferred — free text is the agreed first cut).
- Migrating paragraph / heading to the editor registry (still hardcoded; list and now code use the registry).
- Deleting the old `ui/blocks/code.rs` (read-only). Leave for a follow-up cleanup once nothing references it.
- Any restructuring of `ui/mod.rs` beyond moving existing code into the new module tree.

### Testing

- Existing `from_to_core_tests.rs`, `plugin_block_tests.rs`, `actions_tests.rs` cover the model paths; extend them with code-specific assertions:
  - `from_core` of a markdown code fence with `lang: rust` produces a block with `plugin: Some(_)` and `plugin.attrs.lang == "rust"`.
  - `to_core` of such a block round-trips the lang (write lang to attrs, save, read back, assert lang preserved).
  - Applying `EditAttrs { lang: "python" }` to a code block mirrors `lang` into `BlockKind::Code.lang`.
- New unit-ish tests for the registry: `editor_for("code").is_some()`.
- The `ui/mod.rs` split is a no-behaviour-change refactor; `cargo check` + the existing test suite passing is the bar. No new tests required for the move itself.
- Manual verification via the editor GUI (in scope but not a test gate): open a document with a code fence, edit the body, edit the lang, undo/redo each, save and reopen, observe contents preserved.

## Resolved decisions and tradeoffs

1. **Editing target (Q1):** *Chosen:* body editing + editable language attribute. *Rejected:* (a) plain text body only, no lang editing — felt half-done given that the registry path makes lang editing nearly free; (b) body + lang + syntax highlighting — too big for one iteration.

2. **Keymap (Q2):** *Chosen:* code-native — Enter inserts `\n`, Tab inserts indent. *Rejected:* (a) prose-like (Enter splits the block) — wrong for code; (b) hybrid (Enter on empty trailing line exits) — too many edge cases for first cut.

3. **Body wiring (Q3):** *Chosen:* reuse `mount_block_editor` with a synthetic single-run and empty spans signal. *Rejected:* a parallel mini-mount specifically for `String`-bodied blocks — would duplicate focus/commit/structural-key plumbing for little gain.

4. **Language attr UI (Q4):** *Chosen:* free-form text input (default `attr_text` path). *Rejected:* (a) hardcoded select list — less flexibility, needs escape hatch; (b) defer to read-only — would feel incomplete given the rest of the changes.

5. **`lang` plumbing (Q5):** *Chosen:* mirror `BlockKind::Code.lang` and `plugin.attrs.lang` — symmetric with the list precedent. *Rejected:* moving `lang` to attrs only and dropping it from `BlockKind` — breaks symmetry, touches more files.

6. **Tab key (Q6):** *Chosen:* two spaces. *Rejected:* (a) four spaces — fine, but two is the more common Rust/web default; (b) hard tab — Floem's monospace tab rendering is untested here, risk of quirks.

7. **`ui/mod.rs` decomposition (Q7):** *Chosen:* extract into sibling modules organized by responsibility (focus, undo/redo, action sink, save pipeline, etc.) — each module a thin set of free functions taking signals as args. *Rejected:* (a) extract only the obvious bits and keep `editing_view` together — leaves the file ~300 lines, doesn't address the root pain; (b) introduce an `EditorPaneState` struct that owns the signals and exposes methods — fights Floem's owned-`Rc` closure idiom and adds an abstraction layer the rest of the codebase doesn't use.

## Document metadata

- **Date:** 2026-05-23
- **Status:** approved (implementation not started)
- **Branch:** `feat/code-editor-block`

## Done when

The spec file exists at the path above, covers every section listed (1–5 plus
testing), contains no "TBD"/"TODO"/placeholder text, and records the resolved
decisions. No code is written.

## On completion

Reply with a concise summary: the file you wrote, the sections it contains, and
anything you flagged under "Open questions for Claude".

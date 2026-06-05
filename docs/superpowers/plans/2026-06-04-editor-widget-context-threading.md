# Editor Widget Context Threading (BlockEnv) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bundle the six block-independent dependencies every editor widget threads by hand into one `BlockEnv` struct, so every widget and dispatcher takes `&BlockEnv` plus its own payload — deleting the block-widget-chain `clippy::too_many_arguments` suppressions.

**Architecture:** Define `BlockEnv { on_action, focus_target, focus_pub, current_doc, on_undo, on_redo }` in a new `ui/blocks/env.rs`, construct it once per column rebuild in `editor_pane::block_column` (`editor_pane.rs`), and thread it down `editor_pane → block_view → wrap_block/render_body/plugin_block_view → leaf widgets`. The registry's `EditorWidget` becomes `fn(&EditorBlock, &BlockEnv) -> AnyView`. Pure behavior-preserving refactor; the existing test suite is the safety net.

**Tech Stack:** Rust, floem reactive editor UI, the `lopress-editor` crate's block-widget module tree (`crates/lopress-editor/src/ui/blocks/`).

---

## File Structure

```
crates/lopress-editor/src/ui/
├── blocks/
│   ├── env.rs              ← NEW: BlockEnv struct definition + re-exports
│   ├── mod.rs              ← ADD pub mod env; UPDATE block_view + wrap_block
│   ├── plugin.rs           ← UPDATE plugin_block_view + render_body
│   ├── paragraph.rs        ← UPDATE render_paragraph_editable
│   ├── heading.rs          ← UPDATE render_heading_editable
│   ├── code_editor.rs      ← UPDATE editable_code_view
│   ├── list.rs             ← UPDATE editable_list_view + list_item_editor + make_list_structural_key
│   ├── inline_editor.rs    ← UPDATE editable_inline + mount_block_editor
│   ├── editor_registry.rs  ← UPDATE EditorWidget type + list_editor_widget + code_editor_widget
│   └── table.rs            ← UPDATE table_editor_widget
└── editor_pane.rs          ← UPDATE editor_pane + block_column (construct BlockEnv here)
```

## Suppression sites — IN SCOPE vs OUT OF SCOPE

`grep -rn too_many_arguments crates/lopress-editor/src` returns 15 hits. **13 are in scope**
(removed by this refactor); **2 are kept** (see below). The final count after this work is
**2**, not 1.

- **IN SCOPE (remove `too_many_arguments` as the arg count drops below the threshold):**
  - `mod.rs:55` — `block_view`
  - `plugin.rs:35` — `plugin_block_view`
  - `inline_editor.rs:138` — `editable_inline`
  - `inline_editor.rs:199` — `mount_block_editor`
  - `inline_editor.rs:410` — `handle_key`
  - `paragraph.rs:31` — `render_paragraph_editable`
  - `heading.rs:31` — `render_heading_editable`
  - `code_editor.rs:234` — `editable_code_view`
  - `list.rs:115` — `editable_list_view` (combined with `cast_precision_loss`)
  - `list.rs:197` — `list_item_editor` (multi-lint allow)
  - `list.rs:303` — `make_list_structural_key`
  - `table.rs:93` — `table_editor_widget` (combined with `cast_possible_truncation`)
  - `editor_pane.rs:185` — `block_column` (its 7 params don't trip the threshold once it
    builds `BlockEnv`; the attribute is vestigial — see Task 2 Step 4)

- **KEPT — do NOT remove these two:**
  - `ctrl_wire.rs:28` — threads debug control-server channels/receivers, not block-env
    deps. Out of scope; leave it.
  - `editor_pane.rs:29` — `editor_pane` keeps its 9 params (pane orchestrator:
    `slash_menu_open`/`on_insert_image`/`inserter_items` are not block-env deps). Its allow
    stays, justified by the doc comment above it. **Task 2 does not touch `editor_pane`.**

**CRITICAL — combined allows:** Several sites suppress `too_many_arguments` *together* with a cast lint that must STAY:
- `list.rs:115` is `#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]`
- `table.rs:93` is `#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]`
- `heading.rs:31`, `paragraph.rs:31`, `list.rs:197` are multi-lint `#[allow(...)]` blocks

For these, remove ONLY the `clippy::too_many_arguments` entry and keep the cast/other lints (with their existing justification). Do not delete the whole attribute.

---

## Task 1: Define `BlockEnv` struct in `env.rs`

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/env.rs`

`BlockEnv` bundles the six block-independent dependencies that every editor widget threads by hand. All fields are `Copy` signals or `Rc` — cloning the bundle is refcount-cheap. `ActionSink` and `FocusPublisher` are re-exported from `inline_editor.rs` (where they are defined); do not redefine them.

- [ ] **Step 1: Create `crates/lopress-editor/src/ui/blocks/env.rs`:**

```rust
//! The block-independent environment every editor widget renders into.
//! Created once per column rebuild; cloned freely (all fields are Copy signals or Rc).

use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::model::types::{BlockId, EditorDoc};
use floem::reactive::RwSignal;
use std::rc::Rc;

/// The block-independent environment every editor widget renders into.
/// Created once per column rebuild; cloned freely (all fields are Copy
/// signals or Rc).
#[derive(Clone)]
pub struct BlockEnv {
    pub on_action: ActionSink,                       // Rc<dyn Fn(BlockAction)>
    pub focus_target: RwSignal<Option<BlockId>>,     // Copy
    pub focus_pub: FocusPublisher,                   // Copy (signals inside)
    pub current_doc: RwSignal<Option<EditorDoc>>,    // Copy
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: PASS (new file with no dependencies on other changed files).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/env.rs
git commit -m "refactor(editor): define BlockEnv struct in env.rs"
```

---

## Task 2: Thread `BlockEnv` through `editor_pane` (construct + pass down)

**Files:**
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs`

Construct `BlockEnv` inside `block_column` (where `focus_pub` lives — it's per-rebuild and cannot be hoisted to `editing_view`). Pass `&BlockEnv` to `block_view`. Update both `editor_pane` and `block_column` signatures.

- [ ] **Step 1: Update imports in `editor_pane.rs`** — add the `BlockEnv` import:

```rust
use crate::ui::blocks::env::BlockEnv;
```

Add this to the existing `use` block near the top of the file (after `use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};`).

**`editor_pane`'s signature is UNCHANGED by this task.** It keeps all 9 params and its
existing `#[allow(clippy::too_many_arguments)]` (justified by the doc comment above it).
`editor_pane` is the pane orchestrator — `slash_menu_open`, `on_insert_image`,
`inserter_items` are not block-env deps — so it is intentionally out of this refactor's
scope. All the work happens in `block_column`, which already receives the five stable deps
(`on_action`, `focus_target`, `current_doc`, `on_undo`, `on_redo`) and creates `focus_pub`
locally — i.e. it already has everything `BlockEnv` needs.

- [ ] **Step 2: Construct `BlockEnv` in `block_column`** — add it right after `focus_pub`
is created. `on_action` is `.clone()`d because `block_column` still uses it directly below
(`add_block_button`, `gap_drop_zone`); `on_undo`/`on_redo` are **moved** into `env` (their
only other use — the `block_view` call — is replaced in Step 3, so no leftover binding);
`focus_target`/`current_doc` are `Copy`; `focus_pub` is moved in and read from `env`
afterward.

Replace:
```rust
    let focus_pub = FocusPublisher {
        block: RwSignal::new(None),
        editor_and_spans: RwSignal::new(None),
    };
    let mut rows: Vec<AnyView> = Vec::with_capacity(doc.blocks.len() * 2 + 1);
```

With:
```rust
    let focus_pub = FocusPublisher {
        block: RwSignal::new(None),
        editor_and_spans: RwSignal::new(None),
    };
    let env = BlockEnv {
        on_action: on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
    };
    let mut rows: Vec<AnyView> = Vec::with_capacity(doc.blocks.len() * 2 + 1);
```

- [ ] **Step 3: Update the `block_view` call** inside `block_column` — pass `dnd` + `&env`
instead of the six individual deps:

Replace:
```rust
rows.push(block_view(
    b,
    on_action.clone(),
    focus_target,
    focus_pub,
    dnd,
    current_doc,
    Rc::clone(&on_undo),
    Rc::clone(&on_redo),
));
```

With:
```rust
rows.push(block_view(
    b,
    dnd,
    &env,
));
```

- [ ] **Step 4: Remove `block_column`'s vestigial suppression** — `block_column` keeps all
7 params (it needs them to build `env`), and 7 is at clippy's default
`too-many-arguments-threshold` (the lint fires only at 8+), so the attribute is
unnecessary:

Replace:
```rust
#[allow(clippy::too_many_arguments)]
fn block_column(
```
With:
```rust
fn block_column(
```
(If `cargo clippy -p lopress-editor` unexpectedly flags `block_column` at 7 args — it
should not — restore the attribute WITH a one-line justification; the suppression gate on
this branch requires one.)

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `block_view` still has its old signature (converted next in Task 3). This
is the expected red signal; the crate compiles again at the end of Task 3.

- [ ] **Step 6: Commit** (an intermediate red commit is fine — Task 3 makes it green)

```bash
git add crates/lopress-editor/src/ui/editor_pane.rs
git commit -m "refactor(editor): construct BlockEnv in block_column, pass to block_view"
```

---

## Task 3: Convert `block_view` and `wrap_block` dispatchers

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`

Update `block_view` to take `&BlockEnv` instead of six individual deps. Update `wrap_block` to take `&BlockEnv` for the focus_pub/on_action it needs. Remove the `#[allow(clippy::too_many_arguments)]`.

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `mod.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `block_view` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
#[allow(clippy::too_many_arguments)]
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
```

With:
```rust
pub fn block_view(
    block: &EditorBlock,
    dnd: DndState,
    env: &BlockEnv,
) -> AnyView {
```

Remove the `#[allow(clippy::too_many_arguments)]` — arg count drops from 8 to 3.

- [ ] **Step 3: Update the function body** — replace every use of the old params with `env.` accesses:

Replace:
```rust
    let plugin_view = plugin::plugin_block_view(
        block,
        on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        Rc::clone(&on_undo),
        Rc::clone(&on_redo),
    );
```

With:
```rust
    let plugin_view = plugin::plugin_block_view(
        block,
        env,
    );
```

Replace:
```rust
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => code_editor::editable_code_view(
            text,
            lang,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        ),
```

With:
```rust
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block.id,
            env,
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block.id,
            env,
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => code_editor::editable_code_view(
            text,
            lang,
            block.id,
            env,
        ),
```

Replace:
```rust
    wrap_block(body, block_id, kind, dnd, focus_pub, on_action)
```

With:
```rust
    wrap_block(body, block_id, kind, dnd, &env)
```

- [ ] **Step 4: Update `wrap_block` signature** — replace `focus_pub` + `on_action` with `&BlockEnv`:

Replace:
```rust
fn wrap_block(
    body: AnyView,
    block_id: BlockId,
    kind: BlockKind,
    dnd: DndState,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
) -> AnyView {
```

With:
```rust
fn wrap_block(
    body: AnyView,
    block_id: BlockId,
    kind: BlockKind,
    dnd: DndState,
    env: &BlockEnv,
) -> AnyView {
```

- [ ] **Step 5: Update the `wrap_block` body** — replace uses of `focus_pub` and `on_action` with `env.` accesses:

In the `toolbar_slot` closure, replace `on_action.clone()` with `env.on_action.clone()`:

```rust
    let toolbar_slot = {
        let on_action = env.on_action.clone();
        dyn_container(
```

In the border-style closure, replace `focus_pub.block.get()` with `env.focus_pub.block.get()`:

```rust
    let row_with_border = row.style(move |s| {
        let focused = env.focus_pub.block.get() == Some(block_id);
```

In the `block_toolbar_for` call, replace `focus_pub` with `env.focus_pub`:

```rust
                    block_toolbar_for(block_id, kind.clone(), env.focus_pub, on_action.clone())
```

In the `dyn_container` style closure, replace `focus_pub.block.get()` with `env.focus_pub.block.get()`:

```rust
        .style(move |s| {
            if env.focus_pub.block.get() == Some(block_id) {
```

In the outer `v_stack` style closure, replace `focus_pub.block.get()` with `env.focus_pub.block.get()`:

```rust
        .style(move |s| {
            let focused = env.focus_pub.block.get() == Some(block_id);
```

That's all the changes needed in `wrap_block` — every former use of `focus_pub` or `on_action` now reads through `env.`.

- [ ] **Step 6: Remove unused imports** — `focus_target` and `current_doc` are no longer used at the top level of `mod.rs`. Check if `RwSignal<Option<BlockId>>` and `RwSignal<Option<EditorDoc>>` are still needed (they may be used by `wrap_block`'s `focus_pub` type). Keep `RwSignal` import since `FocusPublisher` uses it.

- [ ] **Step 7: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `plugin::plugin_block_view` signature hasn't been updated yet.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "refactor(editor): convert block_view and wrap_block to take &BlockEnv"
```

---

## Task 4: Convert `plugin_block_view` and `render_body`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs`

Update `plugin_block_view` to take `&BlockEnv`. Update `render_body` similarly. Remove the `#[allow(clippy::too_many_arguments)]`.

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `plugin.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `plugin_block_view` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
#[allow(clippy::too_many_arguments)]
pub fn plugin_block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
```

With:
```rust
pub fn plugin_block_view(
    block: &EditorBlock,
    env: &BlockEnv,
) -> AnyView {
```

Remove `#[allow(clippy::too_many_arguments)]` — arg count drops from 7 to 2.

- [ ] **Step 3: Update the `plugin_block_view` body** — replace individual deps with `env.`:

Replace:
```rust
    let body = render_body(
        block,
        on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
    );
```

With:
```rust
    let body = render_body(
        block,
        env,
    );
```

- [ ] **Step 4: Update `render_body` signature** — same pattern:

Replace:
```rust
fn render_body(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
```

With:
```rust
fn render_body(
    block: &EditorBlock,
    env: &BlockEnv,
) -> AnyView {
```

- [ ] **Step 5: Update the `render_body` body** — replace individual deps with `env.` in all call sites:

Replace:
```rust
            let ctx = EditorContext {
                block,
                on_action: on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                on_undo: Rc::clone(&on_undo),
                on_redo: Rc::clone(&on_redo),
            };
            return widget(&ctx);
```

With:
```rust
            return widget(block, env);
```

Replace the registry dispatch with the new two-arg signature:
```rust
        if let Some(widget) = editor_for(key) {
            return widget(block, env);
        }
```

Replace the fallback match arms with `env.` accesses:
```rust
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block_id,
            env,
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block_id,
            env,
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => code_editor::editable_code_view(
            text,
            lang,
            block_id,
            env,
        )
        .into_any(),
        (BlockKind::List { ordered }, BlockBody::List(items)) => list::editable_list_view(
            items,
            block_id,
            *ordered,
            env,
        ),
```

- [ ] **Step 6: Remove unused imports** — `RwSignal<Option<BlockId>>` and `RwSignal<Option<EditorDoc>>` are no longer used. Remove them from the `use` block. Keep `RwSignal` since it may still be needed for `FocusPublisher`.

- [ ] **Step 7: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `paragraph::render_paragraph_editable` signature hasn't been updated yet.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "refactor(editor): convert plugin_block_view and render_body to take &BlockEnv"
```

---

## Task 5: Convert `render_paragraph_editable`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/paragraph.rs`

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `paragraph.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `render_paragraph_editable` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
// `BODY_FONT_SIZE` is a small positive integer-valued constant, so the
// f32->usize conversion is exact. The argument count mirrors the plumbing.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn render_paragraph_editable(
    runs: &[InlineRun],
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> impl IntoView {
```

With:
```rust
// `BODY_FONT_SIZE` is a small positive integer-valued constant, so the
// f32->usize conversion is exact.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn render_paragraph_editable(
    runs: &[InlineRun],
    block_id: BlockId,
    env: &BlockEnv,
) -> impl IntoView {
```

Remove `clippy::too_many_arguments` from the `#[allow(...)]` block. Keep `cast_possible_truncation` and `cast_sign_loss` with their existing justification.

- [ ] **Step 3: Update the `editable_inline` call** — replace individual deps with `env.`:

Replace:
```rust
    container(editable_inline(
        state,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        true,
        on_undo,
        on_redo,
    ))
```

With:
```rust
    container(editable_inline(
        state,
        block_id,
        env,
        true,
    ))
```

- [ ] **Step 4: Remove unused imports** — `RwSignal<Option<BlockId>>` and `RwSignal<Option<EditorDoc>>` are no longer used.

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `editable_inline` signature hasn't been updated yet.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/paragraph.rs
git commit -m "refactor(editor): convert render_paragraph_editable to take &BlockEnv"
```

---

## Task 6: Convert `render_heading_editable`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/heading.rs`

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `heading.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `render_heading_editable` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
// Font sizes are small positive integer-valued constants, so the f32->usize
// conversion is exact. The argument count mirrors the block-render plumbing.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn render_heading_editable(
    level: u8,
    runs: &[InlineRun],
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> impl IntoView {
```

With:
```rust
// Font sizes are small positive integer-valued constants, so the f32->usize
// conversion is exact.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn render_heading_editable(
    level: u8,
    runs: &[InlineRun],
    block_id: BlockId,
    env: &BlockEnv,
) -> impl IntoView {
```

Remove `clippy::too_many_arguments` from the `#[allow(...)]` block. Keep `cast_possible_truncation` and `cast_sign_loss`.

- [ ] **Step 3: Update the `editable_inline` call** — replace individual deps with `env.`:

Replace:
```rust
    container(editable_inline(
        state,
        block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        false,
        on_undo,
        on_redo,
    ))
```

With:
```rust
    container(editable_inline(
        state,
        block_id,
        env,
        false,
    ))
```

- [ ] **Step 4: Remove unused imports** — `RwSignal<Option<BlockId>>` and `RwSignal<Option<EditorDoc>>` are no longer used.

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `editable_inline` signature hasn't been updated yet.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/heading.rs
git commit -m "refactor(editor): convert render_heading_editable to take &BlockEnv"
```

---

## Task 7: Convert `editable_code_view`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/code_editor.rs`

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `code_editor.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `editable_code_view` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
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
```

With:
```rust
pub fn editable_code_view(
    body: &str,
    lang: &str,
    block_id: BlockId,
    env: &BlockEnv,
) -> AnyView {
```

Remove the `#[allow(clippy::too_many_arguments)]`.

- [ ] **Step 3: Update the `make_code_commit` call** — it needs `current_doc`, which comes from `env`:

The existing `make_code_commit` function signature takes `current_doc: RwSignal<Option<EditorDoc>>`. Pass `env.current_doc` instead.

Replace:
```rust
    let commit = make_code_commit(block_id, editor_sig, commit_on_action, current_doc);
```

With:
```rust
    let commit = make_code_commit(block_id, editor_sig, commit_on_action, env.current_doc);
```

- [ ] **Step 4: Update the `make_code_structural_key` call** — it needs `focus_target` and `current_doc`:

Replace:
```rust
    let structural_key = make_code_structural_key(
        block_id,
        editor_sig,
        on_action.clone(),
        focus_target,
        current_doc,
        commit.clone(),
    );
```

With:
```rust
    let structural_key = make_code_structural_key(
        block_id,
        editor_sig,
        on_action.clone(),
        env.focus_target,
        env.current_doc,
        commit.clone(),
    );
```

- [ ] **Step 5: Update the `mount_block_editor` call** — replace individual deps with `env.`:

Replace:
```rust
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
```

With:
```rust
    let editor_view = mount_block_editor(
        state,
        block_id,
        block_id,
        env,
        commit,
        structural_key,
        /* slash_eligible */ false,
    );
```

- [ ] **Step 6: Remove unused imports** — `RwSignal<Option<BlockId>>` and `RwSignal<Option<EditorDoc>>` are no longer used.

- [ ] **Step 7: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `mount_block_editor` signature hasn't been updated yet.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/code_editor.rs
git commit -m "refactor(editor): convert editable_code_view to take &BlockEnv"
```

---

## Task 8: Convert `editable_list_view`, `list_item_editor`, `make_list_structural_key`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs`

Three functions need conversion. `editable_list_view` takes `&BlockEnv`, `list_item_editor` takes `BlockEnv` by value (it clones for the closure), `make_list_structural_key` takes `&BlockEnv` (it only needs `current_doc` and `focus_target`).

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `list.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `editable_list_view` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
pub fn editable_list_view(
    items: &[ListItem],
    block_id: BlockId,
    ordered: bool,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
```

With:
```rust
#[allow(clippy::cast_precision_loss)]
pub fn editable_list_view(
    items: &[ListItem],
    block_id: BlockId,
    ordered: bool,
    env: &BlockEnv,
) -> AnyView {
```

Remove `clippy::too_many_arguments` from the `#[allow(...)]` block. Keep `cast_precision_loss`.

- [ ] **Step 3: Update the `list_item_editor` call** — pass `env` (cloned for the closure):

Replace:
```rust
            let (editor, editor_sig) = list_item_editor(
                &item.runs,
                block_id,
                item.id,
                idx,
                count,
                Rc::clone(&item_ids),
                Rc::clone(&handles),
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
            );
```

With:
```rust
            let (editor, editor_sig) = list_item_editor(
                &item.runs,
                block_id,
                item.id,
                idx,
                count,
                Rc::clone(&item_ids),
                Rc::clone(&handles),
                env,
            );
```

- [ ] **Step 4: Update `list_item_editor` signature** — take `BlockEnv` by value (it clones for the commit closure):

Replace:
```rust
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn list_item_editor(
    runs: &[InlineRun],
    list_block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> (AnyView, RwSignal<Editor>) {
```

With:
```rust
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn list_item_editor(
    runs: &[InlineRun],
    list_block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    env: &BlockEnv,
) -> (AnyView, RwSignal<Editor>) {
```

Remove `clippy::too_many_arguments`. Keep the cast lints.

- [ ] **Step 5: Update the `list_item_editor` body** — replace individual deps with `env.`:

Replace:
```rust
    let commit: CommitClosure = Rc::new(move || {
        emit_list_commit(
            &commit_handles,
            list_block_id,
            &commit_on_action,
            current_doc,
        );
    });
```

With:
```rust
    let commit_on_action = env.on_action.clone();
    let commit: CommitClosure = Rc::new(move || {
        emit_list_commit(
            &commit_handles,
            list_block_id,
            &commit_on_action,
            env.current_doc,
        );
    });
```

Replace:
```rust
    let view = mount_block_editor(
        state,
        item_id,
        list_block_id,
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
```

With:
```rust
    let view = mount_block_editor(
        state,
        item_id,
        list_block_id,
        env,
        commit,
        structural_key,
        /* slash_eligible */ false,
    );
```

Replace:
```rust
    let structural_key = make_list_structural_key(
        list_block_id,
        item_index,
        item_count,
        Rc::clone(&item_ids),
        Rc::clone(&handles),
        editor_sig,
        on_action.clone(),
        focus_target,
        current_doc,
    );
```

With:
```rust
    let structural_key = make_list_structural_key(
        list_block_id,
        item_index,
        item_count,
        Rc::clone(&item_ids),
        Rc::clone(&handles),
        editor_sig,
        env,
    );
```

- [ ] **Step 6: Update `make_list_structural_key` signature** — replace individual deps with `&BlockEnv":

Replace:
```rust
#[allow(clippy::too_many_arguments)]
fn make_list_structural_key(
    list_block_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> StructuralKey {
```

With:
```rust
fn make_list_structural_key(
    list_block_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    editor_sig: RwSignal<Editor>,
    env: &BlockEnv,
) -> StructuralKey {
```

Remove `#[allow(clippy::too_many_arguments)]`. This function's arg count drops from 9 to 6 (below the threshold). Inside the body, `focus_target` becomes `env.focus_target`, `current_doc` becomes `env.current_doc`, and `on_action` becomes `env.on_action.clone()`.

- [ ] **Step 7: Remove unused imports** — `Rc<dyn Fn()>` for `on_undo`/`on_redo` is no longer needed at the top level.

- [ ] **Step 8: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — `mount_block_editor` signature hasn't been updated yet.

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/list.rs
git commit -m "refactor(editor): convert list widgets to take &BlockEnv"
```

---

## Task 9: Convert `editable_inline` and `mount_block_editor`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

Two public functions and one private function need conversion. `editable_inline` takes `&BlockEnv`, `mount_block_editor` takes `&BlockEnv`, and `handle_key` takes `&BlockEnv` (it needs `current_doc`, `focus_target`, `on_undo`, `on_redo`).

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `inline_editor.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `editable_inline` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
#[allow(clippy::too_many_arguments)]
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    slash_eligible: bool,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> impl IntoView {
```

With:
```rust
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    env: &BlockEnv,
    slash_eligible: bool,
) -> impl IntoView {
```

Remove the `#[allow(clippy::too_many_arguments)]`.

- [ ] **Step 3: Update the `editable_inline` body** — replace individual deps with `env.`:

Replace:
```rust
    let commit: CommitClosure = Rc::new(move || {
        // Suppress the commit when the block's kind is no longer inline-bodied.
        let should_commit = current_doc.with_untracked(|maybe| {
            maybe.as_ref().and_then(|doc| {
                doc.blocks
                    .iter()
                    .find(|b| b.id == block_id)
                    .map(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::Heading(_)))
            })
        });
        if should_commit.unwrap_or(false) {
            commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
        }
    });
    let structural_key: StructuralKey = Rc::new(|_, _| None);
    mount_block_editor(
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
        slash_eligible,
    )
```

With:
```rust
    let commit: CommitClosure = Rc::new(move || {
        let should_commit = env.current_doc.with_untracked(|maybe| {
            maybe.as_ref().and_then(|doc| {
                doc.blocks
                    .iter()
                    .find(|b| b.id == block_id)
                    .map(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::Heading(_)))
            })
        });
        if should_commit.unwrap_or(false) {
            commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
        }
    });
    let structural_key: StructuralKey = Rc::new(|_, _| None);
    mount_block_editor(
        state,
        block_id,
        block_id,
        env,
        commit,
        structural_key,
        slash_eligible,
    )
```

- [ ] **Step 4: Update `mount_block_editor` signature** — replace the six env deps with `&BlockEnv`:

Replace:
```rust
#[allow(clippy::too_many_arguments)]
pub fn mount_block_editor(
    state: BlockEditorState,
    block_id: BlockId,
    publish_block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
    _commit: CommitClosure,
    structural_key: StructuralKey,
    slash_eligible: bool,
) -> impl IntoView {
```

With:
```rust
pub fn mount_block_editor(
    state: BlockEditorState,
    block_id: BlockId,
    publish_block_id: BlockId,
    env: &BlockEnv,
    _commit: CommitClosure,
    structural_key: StructuralKey,
    slash_eligible: bool,
) -> impl IntoView {
```

Remove the `#[allow(clippy::too_many_arguments)]`.

- [ ] **Step 5: Update the `mount_block_editor` body** — replace individual deps with `env.`:

Replace:
```rust
    let on_action_for_key = on_action;
    let commit_for_key = _commit;
    let commit_on_focus_lost = Rc::clone(&commit_for_key);
```

This stays the same (no env deps here).

Replace the `handle_key` call:
```rust
        let result = handle_key(
            kp,
            ms,
            editor_sig,
            spans_sig,
            style_rev,
            block_id,
            &on_action_for_key,
            focus_target,
            current_doc,
            &on_undo,
            &on_redo,
            &commit_for_key,
            slash_eligible,
            link_url_sig,
        );
```

With:
```rust
        let result = handle_key(
            kp,
            ms,
            editor_sig,
            spans_sig,
            style_rev,
            block_id,
            env,
            &commit_for_key,
            slash_eligible,
            link_url_sig,
        );
```

- [ ] **Step 6: Update `handle_key` signature** — replace individual deps with `&BlockEnv`:

Replace:
```rust
// Five reactive signals + two inputs are needed to drive key processing.
#[allow(clippy::too_many_arguments)]
fn handle_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: &Rc<dyn Fn()>,
    on_redo: &Rc<dyn Fn()>,
    commit: &CommitClosure,
    slash_eligible: bool,
    link_url_sig: RwSignal<Option<String>>,
) -> CommandExecuted {
```

With:
```rust
fn handle_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    block_id: BlockId,
    env: &BlockEnv,
    commit: &CommitClosure,
    slash_eligible: bool,
    link_url_sig: RwSignal<Option<String>>,
) -> CommandExecuted {
```

Remove the `#[allow(clippy::too_many_arguments)]` and its comment. The arg count drops from 13 to 10 (below the threshold).

- [ ] **Step 7: Update the `handle_key` body** — replace `current_doc`, `on_undo`, `on_redo` with `env.` accesses:

Replace every `current_doc` usage with `env.current_doc`:
```rust
            let first_id = current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
```
Becomes:
```rust
            let first_id = env.current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
```

Replace `on_undo()` with `env.on_undo()`:
```rust
                        on_undo();
```
Becomes:
```rust
                        env.on_undo();
```

Replace `on_redo()` with `env.on_redo()`:
```rust
                        on_redo();
```
Becomes:
```rust
                        env.on_redo();
```

- [ ] **Step 8: Remove unused imports** — `Rc<dyn Fn()>` is no longer used as a param type.

- [ ] **Step 9: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — the registry widgets (`list_editor_widget`, `code_editor_widget`) still pass the old `EditorContext`-based signature.

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "refactor(editor): convert editable_inline and mount_block_editor to take &BlockEnv"
```

---

## Task 10: Convert registry widgets to `fn(&EditorBlock, &BlockEnv)`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

Update `EditorWidget` type alias to `fn(&EditorBlock, &BlockEnv) -> AnyView`. Update `list_editor_widget` and `code_editor_widget` to take `(&EditorBlock, &BlockEnv)` and extract what they need.

- [ ] **Step 1: Add the `BlockEnv` import** at the top of `editor_registry.rs`:

```rust
use crate::ui::blocks::env::BlockEnv;
```

- [ ] **Step 2: Update `EditorWidget` type alias:**

Replace:
```rust
pub type EditorWidget = fn(&EditorContext) -> AnyView;
```

With:
```rust
pub type EditorWidget = fn(&EditorBlock, &BlockEnv) -> AnyView;
```

- [ ] **Step 3: Update `list_editor_widget` signature and body:**

Replace:
```rust
fn list_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::List(items) = &ctx.block.body else {
        ...
    };
    let ordered = ctx.block.plugin.as_ref()...;
    list::editable_list_view(
        items,
        ctx.block.id,
        ordered,
        ctx.on_action.clone(),
        ctx.focus_target,
        ctx.focus_pub,
        ctx.current_doc,
        Rc::clone(&ctx.on_undo),
        Rc::clone(&ctx.on_redo),
    )
}
```

With:
```rust
fn list_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::List(items) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry list: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub)
            .into_any();
    };
    let ordered = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("ordered"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    list::editable_list_view(
        items,
        block.id,
        ordered,
        env,
    )
}
```

- [ ] **Step 4: Update `code_editor_widget` signature and body:**

Replace:
```rust
fn code_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::Code(body) = &ctx.block.body else {
        ...
    };
    let lang = ctx.block.plugin.as_ref()...;
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

With:
```rust
fn code_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let BlockBody::Code(body) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry code: {:?} has body {:?}",
            block.id, block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub)
            .into_any();
    };
    let lang = block
        .plugin
        .as_ref()
        .and_then(|m| m.attrs.get("lang"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    code_editor::editable_code_view(
        body,
        lang,
        block.id,
        env,
    )
}
```

- [ ] **Step 5: Remove unused imports** — `ActionSink` and `FocusPublisher` are no longer imported from `inline_editor.rs` (they're used by `BlockEnv` which re-exports them).

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p lopress-editor`
Expected: FAIL — the `table`/`image`/`read_more`/`separator` registry widgets still use `&EditorContext` (converted next in Task 11). The crate does not compile again until Task 11 is complete.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "refactor(editor): convert registry widgets to fn(&EditorBlock, &BlockEnv)"
```

---

## Task 11: Convert the remaining registry widgets (`table`, `image`, `read_more`, `separator`)

After Task 10 changed `EditorWidget` to `fn(&EditorBlock, &BlockEnv)`, the crate does NOT
compile until ALL registry widgets are converted off `&EditorContext`. Task 10 converted
`list` and `code`; this task converts the other four (`table`, `image`, `read_more`,
`separator`). `cargo build -p lopress-editor` passes only at the END of this task.

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/table.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/image.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/read_more.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/separator.rs`

- [ ] **Step 1: Update the `table.rs` import:**

Replace:
```rust
use crate::ui::blocks::editor_registry::EditorContext;
```

With:
```rust
use crate::model::types::EditorBlock;
use crate::ui::blocks::env::BlockEnv;
```
(Leave `table.rs`'s other imports as they are.)

- [ ] **Step 2: Update `table_editor_widget` signature:**

Replace:
```rust
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
pub fn table_editor_widget(ctx: &EditorContext) -> AnyView {
```

With:
```rust
#[allow(clippy::cast_possible_truncation)]
pub fn table_editor_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
```

Remove `clippy::too_many_arguments`. Keep `cast_possible_truncation`.

- [ ] **Step 3: Update the `table_editor_widget` body** — replace `ctx.` accesses with `block.` or `env.`:

Replace:
```rust
    let BlockBody::Table(data) = &ctx.block.body else { ... };
    let block_id = ctx.block.id;
    let on_action = ctx.on_action.clone();
    let focus_target = ctx.focus_target;
    let focus_pub = ctx.focus_pub;
    let current_doc = ctx.current_doc;
    let on_undo = Rc::clone(&ctx.on_undo);
    let on_redo = Rc::clone(&ctx.on_redo);
```

With:
```rust
    let BlockBody::Table(data) = &block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] table widget: {:?} has non-table body",
            block.id
        );
        return crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub)
            .into_any();
    };
    let block_id = block.id;
    let on_action = env.on_action.clone();
    let focus_pub = env.focus_pub;
    let current_doc = env.current_doc;
    let on_undo = env.on_undo.clone();
    let on_redo = env.on_redo.clone();
```

Replace every `mount_block_editor(` call in the body — the `focus_target` param should become `env.focus_target`, `current_doc` becomes `env.current_doc`, `on_undo` becomes `env.on_undo.clone()`, `on_redo` becomes `env.on_redo.clone()`.

- [ ] **Step 4: Convert `image.rs`:**

Replace:
```rust
use crate::ui::blocks::editor_registry::EditorContext;
```
With:
```rust
use crate::model::types::EditorBlock;
use crate::ui::blocks::env::BlockEnv;
```

Replace:
```rust
pub fn image_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let meta = match ctx.block.plugin.as_ref() {
```
With:
```rust
pub fn image_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let meta = match block.plugin.as_ref() {
```

Then in the body, change BOTH `attr_field(...)` calls' last argument from
`ctx.on_action.clone(),` to `env.on_action.clone(),`. The helper fns
(`build_placeholder_preview`, `attr_field`, `labeled`) are unchanged.

- [ ] **Step 5: Convert `read_more.rs`:**

Replace:
```rust
use crate::ui::blocks::editor_registry::EditorContext;
```
With:
```rust
use crate::model::types::EditorBlock;
use crate::ui::blocks::env::BlockEnv;
```

Replace:
```rust
pub fn read_more_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
```
With:
```rust
pub fn read_more_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let focus_pub = env.focus_pub;
```

- [ ] **Step 6: Convert `separator.rs`:**

Replace:
```rust
use crate::ui::blocks::editor_registry::EditorContext;
```
With:
```rust
use crate::model::types::EditorBlock;
use crate::ui::blocks::env::BlockEnv;
```

Replace:
```rust
pub fn separator_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
```
With:
```rust
pub fn separator_widget(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let focus_pub = env.focus_pub;
```

- [ ] **Step 7: Verify all registry widgets compile**

Run: `cargo build -p lopress-editor`
Expected: PASS — `table`, `image`, `read_more`, and `separator` now all take
`(&EditorBlock, &BlockEnv)`, matching the `EditorWidget` type from Task 10. Sanity check:
`grep -rn "ctx: &EditorContext" crates/lopress-editor/src` must return nothing.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/table.rs \
        crates/lopress-editor/src/ui/blocks/image.rs \
        crates/lopress-editor/src/ui/blocks/read_more.rs \
        crates/lopress-editor/src/ui/blocks/separator.rs
git commit -m "refactor(editor): convert table/image/read_more/separator widgets to (&EditorBlock, &BlockEnv)"
```

---

## Task 12: Remove redundant `EditorContext` re-bundling

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

`EditorContext` is now only used internally. The `render_body` function in `plugin.rs` no longer constructs it (it was already changed in Task 4 to pass `block` + `env` directly to the widget). Check if `EditorContext` is still referenced anywhere.

- [ ] **Step 1: Verify `EditorContext` is no longer used** — grep for it:

Run: `grep -rn "EditorContext" crates/lopress-editor/src/ui/`
Expected: Only the struct definition and type alias in `editor_registry.rs` remain. All usages should have been replaced.

- [ ] **Step 2: Delete the `EditorContext` struct:**

`EditorContext` is now dead — every widget takes `(&EditorBlock, &BlockEnv)`, and the
`EditorWidget` alias was already changed to that signature in Task 10. Delete the struct
from `editor_registry.rs`. **KEEP** the `EditorWidget` alias — it now reads
`pub type EditorWidget = fn(&EditorBlock, &BlockEnv) -> AnyView;` and is still used by
`editor_for`. Delete only:

```rust
pub struct EditorContext<'a> {
    pub block: &'a EditorBlock,
    pub on_action: ActionSink,
    pub focus_target: RwSignal<Option<BlockId>>,
    pub focus_pub: FocusPublisher,
    pub current_doc: RwSignal<Option<EditorDoc>>,
    pub on_undo: Rc<dyn Fn()>,
    pub on_redo: Rc<dyn Fn()>,
}
```

- [ ] **Step 3: Remove now-unused imports in `editor_registry.rs`:**

With the struct gone, some imports become unused. Rather than guess, let the build report
them: after Step 2, `cargo build -p lopress-editor` will warn (and, under `-D warnings`,
fail) on each unused import. Remove exactly those — typically `ActionSink`,
`FocusPublisher`, `RwSignal`, and `Rc`. Keep `EditorBlock`, `BlockEnv`, `AnyView`, and any
`BlockBody`/`BlockId` still referenced by the widget bodies.

- [ ] **Step 4: Verify it compiles:**

Run: `cargo build -p lopress-editor`
Expected: PASS.

- [ ] **Step 5: Verify the in-scope suppression count is zero:**

Run: `grep -rn "too_many_arguments" crates/lopress-editor/src/`
Expected: exactly TWO remain — `ctrl_wire.rs:28` and `editor_pane.rs:29` (both KEPT; see
the IN SCOPE vs KEPT section near the top). The 13 in-scope widget/`block_column`
`too_many_arguments` suppressions are gone.

- [ ] **Step 6: Commit:**

```bash
git add crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "refactor(editor): drop dead EditorContext struct"
```

---

## Task 13: Final workspace gate + suppression verification

**Files:** none (verification only; any fixes get folded into the relevant task's files).

- [ ] **Step 1: Force clippy re-lint** — touch one source file per changed crate:

```bash
touch crates/lopress-editor/src/lib.rs
```

- [ ] **Step 2: Run the full workspace gate**

```bash
bash scripts/check.sh
```

Expected: PASS — `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `taplo fmt --check`, `scripts/check-suppressions.sh` all green.

- [ ] **Step 3: Verify suppression count**

Run: `grep -rn "too_many_arguments" crates/lopress-editor/src/`
Expected: exactly two — `ctrl_wire.rs:28` and `editor_pane.rs:29` (both KEPT).

- [ ] **Step 4: Verify no new `#[allow]` suppressions were added**

Run: `grep -rn "#\[allow" crates/lopress-editor/src/ui/blocks/env.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/plugin.rs crates/lopress-editor/src/ui/blocks/paragraph.rs crates/lopress-editor/src/ui/blocks/heading.rs crates/lopress-editor/src/ui/blocks/code_editor.rs crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/inline_editor.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs crates/lopress-editor/src/ui/blocks/table.rs crates/lopress-editor/src/ui/editor_pane.rs`
Expected: Only cast lints (kept with justification) and no new `too_many_arguments` entries.

- [ ] **Step 5: Manual GUI smoke test** (spec §7)

If the implementer has access to a GUI:
1. `cargo run` with a visible window
2. Open the control server at `127.0.0.1:7878`
3. Open a document, focus a paragraph/heading/code/list block
4. Confirm editing + toolbar still work
5. If the implementer cannot run a GUI, return this step as "needs human" with a concrete checklist

- [ ] **Step 6: Final commit (only if fixes were needed)**

```bash
git add <only the files you changed>
git commit -m "fix(editor): address gate failures from BlockEnv refactor"
```

---

## Done when

All 13 tasks are committed (one commit each, named-file staging), `bash scripts/check.sh` passes clean, and the manual GUI smoke test confirms the editor renders and edits identically. The `too_many_arguments` suppression count drops from 15 to 2 — the 13 in-scope widget/`block_column` suppressions are removed, and `ctrl_wire.rs:28` + `editor_pane.rs:29` are intentionally KEPT.

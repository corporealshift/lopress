# Editor Visual / UX Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four lopress editor visual/UX defects — invisible caret, non-scrolling pane, per-block scrollbars/no-wrap, and missing focus indication.

**Architecture:** Replace Floem's high-level `editor_container_view` (which wraps every block in its own scroll) with the lower-level `editor_view`, re-wiring pointer/key/focus events by hand. Size each block to its wrapped visual-line count. Make the caret-visibility read reactive and give the caret an explicit color. Add a reactive focus border in `block_view`.

**Tech Stack:** Rust, Floem 0.2 GUI framework, crate `lopress-editor`.

---

### Task 1: Pure block-height helper + unit test

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

- [ ] **Step 1: Write the failing test**

Append the following test module at the very end of `inline_editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::block_height_px;

    #[test]
    fn block_height_scales_with_visual_lines() {
        assert!((block_height_px(1, 20.0) - 20.0).abs() < f32::EPSILON);
        assert!((block_height_px(3, 20.0) - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn block_height_clamps_empty_to_one_line() {
        assert!((block_height_px(0, 20.0) - 20.0).abs() < f32::EPSILON);
    }
}
```

- [ ] **Step 2: Verify the test fails to compile**

```
cargo test -p lopress-editor block_height
```

Expected: compilation error — `unresolved import 'super::block_height_px'` — because the function does not exist yet.

- [ ] **Step 3: Add `block_height_px` helper function**

Add the following free function near the top of `inline_editor.rs`, after the existing imports and before the `build_block_editor` function (which starts around line 75):

```rust
/// Pixel height of a block given its wrapped visual-line count. Clamps to at
/// least one line so an empty block still has height.
fn block_height_px(visual_lines: u16, line_height: f32) -> f32 {
    f32::from(visual_lines.max(1)) * line_height
}
```

- [ ] **Step 4: Verify tests pass**

```
cargo test -p lopress-editor block_height
```

Expected: `test result: ok. 2 passed; 0 failed` (two tests pass).

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "test(editor): add block_height_px helper with unit tests

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2a: Replace `editor_container_view` with `editor_view`; wire pointer + focus events

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, function `editable_inline`

- [ ] **Step 1: Current state note**

`editable_inline` currently calls `editor_container_view(editor_sig, is_active, key_handler)`, which wraps every block in its own `scroll` container. This causes per-block scrollbars (problem 3) and swallows mouse-wheel events (problem 2). No unit test — this is GUI wiring.

- [ ] **Step 2: Update imports**

Change the existing import:

```rust
// BEFORE:
use floem::views::editor::view::editor_container_view;

// AFTER:
use floem::views::editor::view::editor_view;
```

Add two new imports near the top of the file (after the existing `use floem::...` lines):

```rust
use floem::event::{Event, EventListener};
```

Keep the `use floem::views::editor::gutter::GutterClass;` import for now — it is still referenced by the height closure until Task 3.

- [ ] **Step 3: Replace the `editor_container_view` call and wire events**

In `editable_inline`, locate the `let default_kp_handler = default_key_handler(editor_sig);` line and the `let view = editor_container_view(...);` statement that immediately follows it. **Replace only those two items** — from `let default_kp_handler` up to and including the closing `);` of the `editor_container_view(...)` call — with the code below.

**Do NOT touch** the two `create_effect` blocks that follow the `editor_container_view` call (the focus-publish effect and the focus_target effect), and **do NOT touch** the `let line_height = ...` / `stack((view,)).style(...)` block at the end of the function — those stay exactly as they are (the `stack` block is replaced later, in Task 3).

Replacement code:

```rust
    // Build the default command handler once (arrows, backspace, etc).
    let default_kp_handler = default_key_handler(editor_sig);
    let _combined_key = move |kp: &KeyPress, ms: floem::keyboard::Modifiers| {
        let result = handle_key(
            kp, ms, editor_sig, spans_sig, style_rev, block_id,
            &on_action_for_key, focus_target, current_doc, &on_undo, &on_redo,
            slash_eligible, link_url_sig,
        );
        if result == CommandExecuted::Yes {
            result
        } else {
            default_kp_handler(kp, ms)
        }
    };

    // Lower-level editor view: no gutter, no per-block scroll. The is_active
    // closure reads `active` with a *tracked* `.get()` so the caret paint is
    // invalidated when focus changes (Floem wraps this closure in a memo).
    let view = editor_view(editor_sig, move |_| editor_sig.with(|ed| ed.active.get()));
    let view_id = view.id();
    editor_sig.with_untracked(|ed| ed.editor_view_id.set(Some(view_id)));

    let view = view
        .style(|s| s.size_full().cursor(floem::style::CursorStyle::Text))
        .on_event_cont(EventListener::FocusGained, move |_| {
            editor_sig.with_untracked(|ed| ed.editor_view_focused.notify());
        })
        .on_event_cont(EventListener::FocusLost, move |_| {
            editor_sig.with_untracked(|ed| ed.editor_view_focus_lost.notify());
        })
        .on_event_cont(EventListener::PointerDown, move |event| {
            if let Event::PointerDown(pe) = event {
                view_id.request_active();
                view_id.request_focus();
                editor_sig.get_untracked().pointer_down(pe);
            }
        })
        .on_event_cont(EventListener::PointerMove, move |event| {
            if let Event::PointerMove(pe) = event {
                editor_sig.get_untracked().pointer_move(pe);
            }
        })
        .on_event_cont(EventListener::PointerUp, move |event| {
            if let Event::PointerUp(pe) = event {
                editor_sig.get_untracked().pointer_up(pe);
            }
        })
        .on_move(move |point| {
            editor_sig.with_untracked(|ed| ed.window_origin.set(point));
        });
```

After this edit the function reads, in order: the new code above, then the two unchanged `create_effect` blocks, then the unchanged `let line_height = ...; stack((view,)).style(...)` block. The new `let view` shadows cleanly — the trailing `stack((view,))` still refers to it.

Notes:
- `_combined_key` is prefixed with `_` to avoid an unused-variable warning in this intermediate commit — Task 2b renames it back and uses it.
- The KeyDown handler is intentionally absent here — it is wired in Task 2b.
- The height closure (`stack((view,))...`) is left unchanged for now — it is replaced in Task 3.
- The `is_active` closure uses `.with(|ed| ed.active.get())` (tracked) instead of `.with_untracked(...)` — this is the reactive caret fix from Section 2 of the spec.

- [ ] **Step 4: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 5: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Build and run the app.
- Click into a paragraph block — the block should activate (focus).
- **Typing will NOT work yet** (KeyDown is wired in Task 2b) — this is the expected intermediate state.
- Note: the caret may now be visible (reactive `is_active` fix from Task 4 is partially applied), and per-block scrollbars should be gone.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "refactor(editor): mount inline blocks via editor_view, drop per-block scroll

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2b: Wire the KeyDown event (key handler + character input)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, function `editable_inline`

- [ ] **Step 1: Current state note**

The `_combined_key` closure is built but unused (prefixed with `_` in Task 2a). The `editor_view` has no KeyDown handler, so typing does not insert characters.

- [ ] **Step 2: Add KeyDown handler to the view builder chain**

Rename `_combined_key` back to `combined_key`, and insert the `.on_event_stop(EventListener::KeyDown, ...)` call just before the `.on_move(...)` call in the `view` builder chain. The full updated chain (FocusGained/Lost, PointerDown/Move/Up, KeyDown, then `.on_move`) should be:

```rust
    let view = editor_view(editor_sig, move |_| editor_sig.with(|ed| ed.active.get()));
    let view_id = view.id();
    editor_sig.with_untracked(|ed| ed.editor_view_id.set(Some(view_id)));

    let view = view
        .style(|s| s.size_full().cursor(floem::style::CursorStyle::Text))
        .on_event_cont(EventListener::FocusGained, move |_| {
            editor_sig.with_untracked(|ed| ed.editor_view_focused.notify());
        })
        .on_event_cont(EventListener::FocusLost, move |_| {
            editor_sig.with_untracked(|ed| ed.editor_view_focus_lost.notify());
        })
        .on_event_cont(EventListener::PointerDown, move |event| {
            if let Event::PointerDown(pe) = event {
                view_id.request_active();
                view_id.request_focus();
                editor_sig.get_untracked().pointer_down(pe);
            }
        })
        .on_event_cont(EventListener::PointerMove, move |event| {
            if let Event::PointerMove(pe) = event {
                editor_sig.get_untracked().pointer_move(pe);
            }
        })
        .on_event_cont(EventListener::PointerUp, move |event| {
            if let Event::PointerUp(pe) = event {
                editor_sig.get_untracked().pointer_up(pe);
            }
        })
        .on_event_stop(EventListener::KeyDown, move |event| {
            let Event::KeyDown(key_event) = event else {
                return;
            };
            let key_text = key_event.key.text.clone();
            let Ok(keypress) = KeyPress::try_from(key_event) else {
                return;
            };
            combined_key(&keypress, key_event.modifiers);

            // Character insertion: Floem's editor_view does not insert text
            // itself — editor_content used to. Replicate that, with SHIFT/ALTGR
            // (and ALT on macOS) cleared so shifted characters still type.
            let mut mods = key_event.modifiers;
            mods.set(floem::keyboard::Modifiers::SHIFT, false);
            mods.set(floem::keyboard::Modifiers::ALTGR, false);
            #[cfg(target_os = "macos")]
            mods.set(floem::keyboard::Modifiers::ALT, false);
            if mods.is_empty() {
                use floem::keyboard::{Key, NamedKey};
                match keypress.key {
                    KeyInput::Keyboard(Key::Character(c), _) => {
                        editor_sig.get_untracked().receive_char(&c);
                    }
                    KeyInput::Keyboard(Key::Named(NamedKey::Space), _) => {
                        editor_sig.get_untracked().receive_char(" ");
                    }
                    KeyInput::Keyboard(Key::Unidentified(_), _) => {
                        if let Some(text) = key_text {
                            editor_sig.get_untracked().receive_char(&text);
                        }
                    }
                    _ => {}
                }
            }
        })
        .on_move(move |point| {
            editor_sig.with_untracked(|ed| ed.window_origin.set(point));
        });
```

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 4: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Build and run the app.
- Click into a paragraph block and type characters — they should insert at the cursor.
- Space inserts a space character.
- Enter splits the block.
- Arrow keys navigate within the block.
- Backspace at offset 0 merges with the previous block.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "feat(editor): wire keydown + character input for editor_view blocks

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 3: Reactive block height from visual line count

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, function `editable_inline`

- [ ] **Step 1: Current state note**

The block height closure currently uses `text_sig.get().split('\n').count()` — counting only hard newlines. Wrapped text overflows because the block height does not account for visual (wrapped) lines.

- [ ] **Step 2: Replace the height closure**

Locate the final `stack((view,)).style(...)` block in `editable_inline` and replace it with:

```rust
    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    stack((view,)).style(move |s| {
        // Track screen_lines so the height recomputes when wrapping reflows
        // (text edit or column-width change). `last_vline()` is the index of
        // the last wrapped visual line; +1 converts it to a count.
        let visual_lines = editor_sig.with(|ed| {
            ed.screen_lines.get();
            u16::try_from(ed.last_vline().0 + 1).unwrap_or(u16::MAX)
        });
        s.width_full()
            .height(block_height_px(visual_lines, line_height))
    })
```

- [ ] **Step 3: Remove the unused `GutterClass` import**

Remove the line near the top of the file:

```rust
use floem::views::editor::gutter::GutterClass;
```

This import is no longer referenced anywhere in the file.

- [ ] **Step 4: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 5: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Build and run the app.
- Type a paragraph longer than the column width (720 px max content width) — it should wrap onto multiple visual lines.
- The block should grow vertically to fit its wrapped content — no clipping, no inner scrollbar.
- Mouse-wheel scrolling over the block should scroll the editor pane (not a per-block scroll, since that no longer exists).

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "fix(editor): size inline blocks to wrapped visual line count

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 4: Reactive caret + explicit caret color

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, function `editable_inline`

- [ ] **Step 1: Current state note**

The `is_active` read was already made reactive (tracked) in Task 2a. The caret color is still the default, which may not contrast against the white editing background.

- [ ] **Step 2: Add caret color constant**

Add the following module-level constant near the top of `inline_editor.rs`, after the existing imports and before the `block_height_px` function:

```rust
/// Caret color for inline block editors — dark enough to contrast on white.
const CARET_COLOR: floem::peniko::Color = floem::peniko::Color::rgb8(40, 40, 40);
```

- [ ] **Step 3: Set the caret color on the view style**

Locate the `.style(|s| s.size_full().cursor(floem::style::CursorStyle::Text))` call on the `view` builder chain and replace it with:

```rust
        .style(|s| {
            s.size_full()
                .cursor(floem::style::CursorStyle::Text)
                .set(floem::views::editor::CursorColor, CARET_COLOR)
        })
```

If `floem::views::editor::CursorColor` does not resolve, locate the correct import path (it is re-exported from the editor module) and confirm the caret is visible with the debug skill.

- [ ] **Step 4: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 5: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Build and run the app.
- Click into a paragraph block and take a screenshot — a dark (rgb 40,40,40) blinking caret should be clearly visible at the insertion point.
- Confirm the caret blink state (`cursor_info.hidden`) is not stuck hidden.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "fix(editor): make caret reactive to focus and give it a visible color

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 5: Focused-block border in `block_view`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`, function `block_view`

- [ ] **Step 1: Current state note**

The focused block is tracked by `focus_pub.block` (a `FocusPublisher` signal) but no visual border is rendered. Both return paths in `block_view` end in `.style(|s| s.width_full())` with no border.

- [ ] **Step 2: Add focus border color constant**

Add the following module-level constant near the top of `mod.rs`, after the existing imports:

```rust
/// Border color for the block that currently holds focus.
const FOCUS_BORDER: floem::peniko::Color = floem::peniko::Color::rgb8(150, 180, 230);
```

- [ ] **Step 3: Update the plugin return path**

In the plugin early-return, locate:

```rust
    return v_stack((toolbar_slot, plugin_view))
        .style(|s| s.width_full())
        .into_any();
```

Replace with:

```rust
    return v_stack((toolbar_slot, plugin_view))
        .style(move |s| {
            let focused = focus_pub.block.get() == Some(block_id);
            let s = s.width_full().border(1.0).border_radius(4.0);
            if focused {
                s.border_color(FOCUS_BORDER)
            } else {
                s.border_color(floem::peniko::Color::TRANSPARENT)
            }
        })
        .into_any();
```

- [ ] **Step 4: Update the normal (non-plugin) return path**

Locate the final return in `block_view`:

```rust
    v_stack((toolbar_slot, row))
        .style(|s| s.width_full())
        .into_any()
```

Replace with:

```rust
    v_stack((toolbar_slot, row))
        .style(move |s| {
            let focused = focus_pub.block.get() == Some(block_id);
            let s = s.width_full().border(1.0).border_radius(4.0);
            if focused {
                s.border_color(FOCUS_BORDER)
            } else {
                s.border_color(floem::peniko::Color::TRANSPARENT)
            }
        })
        .into_any()
```

Note: `focus_pub` is `Copy` and `block_id` is `Copy`, so the closure captures them by value with `move`. Both blocks always carry a 1px border (transparent when unfocused) so focus changes never shift layout.

- [ ] **Step 5: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 6: Manual verification**

Using the `driving-lopress-editor` debug skill:
- Build and run the app.
- Click between blocks — a light-blue (rgb 150,180,230) 1px border should appear on the focused block.
- Unfocused blocks show no visible border (same-width transparent border, no layout shift).
- The focus border moves with focus as the user clicks between blocks.

- [ ] **Step 7: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "feat(editor): outline the focused block with a border

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 6: Full manual verification pass

**Files:** No file changes. No commit.

- [ ] **Step 1: Verify caret visibility**

Using the `driving-lopress-editor` skill: build and run the app, click into a paragraph block, take a screenshot. Confirm a dark blinking caret is visible at the insertion point.

- [ ] **Step 2: Verify wrapping and no inner scrollbar**

Type a paragraph longer than the column width. Confirm it wraps within the column with no inner scrollbar and the block grows to fit.

- [ ] **Step 3: Verify focus border**

Click between blocks. Confirm a light-blue 1px border appears on the focused block and moves with focus. Unfocused blocks show no visible border and nothing shifts position.

- [ ] **Step 4: Verify pane scroll**

Open or create a long document (20+ blocks). Use mouse wheel to scroll the editor pane. Confirm the document scrolls smoothly.

- [ ] **Step 5: Run the block-height unit tests**

```
cargo test -p lopress-editor block_height
```

Expected: 2 tests pass.

- [ ] **Step 6: Run clippy**

```
cargo clippy -p lopress-editor
```

Expected: no warnings (or only pre-existing warnings not introduced by this change).

- [ ] **Step 7: Check formatting**

```
cargo fmt --check
```

Expected: no output (files are already formatted).

---

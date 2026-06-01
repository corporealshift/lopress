# Fail-safe block rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every `empty()`-on-mismatch render path with a focusable, recoverable fallback block view; tidy the two stale-commit sources and add a provenance-gated `debug_assert`; and make the `from_core` parse boundary total and non-panicking.

**Architecture:** Three complementary layers around one shared `fallback_block_view`: (1) the fallback view wired into all four `empty()` sites + the `editor_for == None` dead-end, (2) commit-source tidying (toolbar pre-commits the block's actual body shape; the inline-editor FocusLost commit is suppressed when the kind changed under it) plus a `debug_assert` at the `apply` coercion chokepoint, and (3) hardening `from_core` so unclassifiable on-disk blocks load through the same fallback instead of panicking or vanishing.

**Tech Stack:** Rust, floem 0.2 (reactive GUI), the in-app `tiny_http` control server on 127.0.0.1:7878 for live e2e verification.

---

## File Structure

| File | Tasks | Role |
|---|---|---|
| `crates/lopress-editor/src/actions.rs` | 1, 6 | Promote `body_to_flat_text`; add `debug_assert` with built-in-provenance gate |
| `crates/lopress-editor/src/ui/blocks/fallback.rs` | 2 | New module: `fallback_block_view` (content + warning chrome + focus handler) |
| `crates/lopress-editor/src/ui/blocks/mod.rs` | 3 | Wire the `_ => empty()` kind/body mismatch arm to the fallback |
| `crates/lopress-editor/src/ui/blocks/plugin.rs` | 3 | Wire the `_ => empty()` built-in dispatch fallthrough to the fallback |
| `crates/lopress-editor/src/ui/blocks/editor_registry.rs` | 3 | Wire the two `let ... else { empty() }` body-shape guards to the fallback |
| `crates/lopress-editor/src/ui/toolbar.rs` | 4 | Pre-commit the block's actual body shape instead of unconditionally `Inline` |
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | 5 | FocusLost suppression via `current_doc` when kind changed |
| `crates/lopress-editor/src/model/from_core.rs` | 7 | Harden: no panics on disk data, route unclassifiable blocks through fallback |
| `crates/lopress-editor/tests/actions_tests.rs` | 1, 6 | Tests for `body_to_flat_text`, coercion, and `debug_assert` characterization |
| `crates/lopress-editor/tests/from_to_core_tests.rs` | 7 | Tests for unknown plugin / malformed attrs loading |

---

### Task 1: Promote `body_to_flat_text` to a shared `pub(crate)` helper

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs`

**Goal:** Make `body_to_flat_text` visible to `fallback.rs` (and any other crate module that needs to flatten a body to text for display).

- [ ] **Step 1: Write the failing test**

Append the following test module at the very end of `crates/lopress-editor/src/actions.rs` (after the existing `#[cfg(test)]` blocks):

```rust
#[cfg(test)]
mod body_to_flat_text_tests {
    use super::*;

    #[test]
    fn inline_runs_concatenate() {
        let body = BlockBody::Inline(vec![
            InlineRun::plain("hello "),
            InlineRun { text: "world".into(), bold: true, ..Default::default() },
        ]);
        assert_eq!(body_to_flat_text(&body), "hello world");
    }

    #[test]
    fn code_returns_text_as_is() {
        let body = BlockBody::Code("fn main() {}".to_string());
        assert_eq!(body_to_flat_text(&body), "fn main() {}");
    }

    #[test]
    fn list_joins_items_with_newlines() {
        let body = BlockBody::List(vec![
            ListItem { id: BlockId::new(), runs: vec![InlineRun::plain("a")] },
            ListItem { id: BlockId::new(), runs: vec![InlineRun::plain("b")] },
        ]);
        assert_eq!(body_to_flat_text(&body), "a\nb");
    }

    #[test]
    fn opaque_returns_empty_string() {
        let body = BlockBody::Opaque(serde_json::json!({"foo": 42}));
        assert_eq!(body_to_flat_text(&body), "");
    }
}
```

- [ ] **Step 2: Verify the test fails to compile**

```
cargo test -p lopress-editor body_to_flat_text
```

Expected: compilation error — `unresolved import 'super::body_to_flat_text'` (the function is private).

- [ ] **Step 3: Promote `body_to_flat_text` to `pub(crate)`**

In `crates/lopress-editor/src/actions.rs`, locate the function at line ~632:

```rust
fn body_to_flat_text(body: &BlockBody) -> String {
```

Change the visibility to `pub(crate)`:

```rust
/// Flatten any body to its plain text. Mirrors the flattening that
/// `apply_change_type` performs: `Inline`/`List` runs are concatenated, list
/// items are joined with `\n`, `Code` is already flat, and `Opaque` has no
/// text. Shared between `apply_change_type` / `coerce_body_to_kind` and the
/// render-layer fallback view so every code path presents the same text.
pub(crate) fn body_to_flat_text(body: &BlockBody) -> String {
```

- [ ] **Step 4: Verify tests pass**

```
cargo test -p lopress-editor body_to_flat_text
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: Verify the whole workspace still compiles**

```
bash scripts/check.sh
```

Expected: all three phases pass (fmt, clippy, test). No new warnings.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/actions.rs
git commit -m "$(cat <<'EOF'
refactor(editor): promote body_to_flat_text to pub(crate) for fallback view

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Create `fallback_block_view` — content + warning chrome + focus handler

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/fallback.rs`

**Goal:** A new module exposing `fallback_block_view(block, focus_pub) -> AnyView` that renders (a) the block's flat text (or pretty-printed JSON for Opaque bodies), (b) an inline warning banner, and (c) an `on_event(PointerDown)` handler that sets `focus_pub.block` and clears `focus_pub.editor_and_spans` to mount the toolbar for recovery.

The fallback is **read-only** — it emits no `on_action` because the body shape is ambiguous and committing it would risk a fresh mismatch. Recovery happens through the toolbar slot, which the wrapper (`block_view` in `mod.rs`, `plugin_block_view` in `plugin.rs`) mounts keyed on `focus_pub.block`.

- [ ] **Step 1: Write the module**

Create `crates/lopress-editor/src/ui/blocks/fallback.rs` with the following complete content:

```rust
//! Recoverable fallback view for blocks that can't be rendered by their normal editor.
//!
//! Renders visible content (flat text or pretty-printed JSON for Opaque bodies),
//! a persistent inline warning banner, and a PointerDown handler that sets focus
//! so the toolbar mounts — giving the user Change Type / Delete to recover.

use crate::actions::body_to_flat_text;
use crate::model::types::{BlockBody, EditorBlock};
use crate::ui::blocks::inline_editor::FocusPublisher;
use crate::ui::blocks::paragraph::MONO_FAMILY;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{label, stack, text, Decorators};
use floem::{AnyView, IntoView};

/// Warning text shown inline on every fallback block. Self-clears because the
/// fallback view is no longer constructed once the block renders normally.
const WARNING_TEXT: &str = "This block couldn't be displayed with its editor — showing its raw content. Change its type or delete it to recover.";

/// Build a recoverable fallback view for a block that can't be rendered normally.
///
/// Renders the block's flat text (or pretty-printed JSON for Opaque bodies),
/// an inline warning banner, and a PointerDown handler that sets `focus_pub.block`
/// (mounting the toolbar) and clears `focus_pub.editor_and_spans` (preventing
/// stale editor handles from being read by the toolbar's pre-commit).
///
/// The fallback is read-only — no in-place editing, because the body shape is
/// ambiguous and committing it would risk a fresh mismatch. Recovery is via the
/// toolbar only (Change Type re-mounts a working editor; Delete removes the block).
pub fn fallback_block_view(block: &EditorBlock, focus_pub: FocusPublisher) -> AnyView {
    let block_id = block.id;

    // Visible content: flat text for known body shapes, pretty-printed JSON for Opaque.
    let content = match &block.body {
        BlockBody::Opaque(value) => {
            // Opaque bodies have no flat text; show the pretty-printed JSON
            // the same way the opaque renderer does (opaque.rs), so the user
            // can still see the raw content even when it can't be classified.
            let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            text(json)
                .style(|s| {
                    s.font_family(MONO_FAMILY.to_string())
                        .font_size(12.)
                        .padding(8.)
                        .background(Color::rgb8(245, 245, 245))
                        .width_full()
                })
                .into_any()
        }
        _ => {
            let flat = body_to_flat_text(&block.body);
            text(flat)
                .style(|s| {
                    s.font_size(14.)
                        .padding(8.)
                        .width_full()
                })
                .into_any()
        }
    };

    // Warning banner: persistent, inline, non-blocking.
    let warning = label(|| WARNING_TEXT.to_string())
        .style(|s| {
            s.font_size(11.)
                .color(Color::rgb8(180, 120, 40))
                .padding_horiz(8.)
                .padding_vert(4.)
                .background(Color::rgb8(255, 248, 230))
                .border_radius(4.)
                .margin(6.)
        });

    // The body: content + warning stacked, with a PointerDown that sets focus.
    let body = stack((content, warning))
        .style(|s| {
            s.width_full()
                .border(1.)
                .border_color(Color::rgb8(220, 200, 160))
                .border_radius(4.)
                .background(Color::rgb8(255, 252, 240))
        })
        .on_event(EventListener::PointerDown, move |_| {
            // Mount the toolbar: set the focused block id.
            focus_pub.block.set(Some(block_id));
            // Clear stale editor handles so the toolbar's pre-commit doesn't
            // read a previous block's inline editor and fire it against this one.
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        });

    body.into_any()
}
```

Key design decisions:
- **No `on_action` parameter.** The fallback is read-only. Recovery happens through the toolbar slot mounted by the wrapper, keyed on `focus_pub.block`.
- **Opaque bodies show pretty-printed JSON** (via `serde_json::to_string_pretty` on the inner `Value`, mirroring `opaque.rs`). `body_to_flat_text` returns `""` for Opaque, so the JSON branch is what keeps the content visible — essential because unknown-from-disk blocks (Task 7) are the primary consumers of the fallback.
- **`FocusPublisher` passed by value** (it derives `Clone, Copy`).
- **`debug_assertions` eprintln!** for debug logging — the crate uses `eprintln!`, not `log::debug!`.
- **Imports are minimal** — only what the final body uses: `EventListener`, `EventPropagation`, `Color`, `SignalUpdate`, `text`, `label`, `stack`, `Decorators`, `AnyView`, `IntoView`, `body_to_flat_text`, `BlockBody`, `EditorBlock`, `FocusPublisher`.
- **`font_size(12.)` uses `f32`** — matches the real floem API as seen in `opaque.rs` and `paragraph.rs`.

- [ ] **Step 2: Register the module**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add the module declaration after `pub mod opaque;`:

```rust
pub mod fallback;
```

- [ ] **Step 3: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 4: Verify the workspace gate passes**

```
bash scripts/check.sh
```

Expected: all three phases pass. No new warnings.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/fallback.rs
git commit -m "$(cat <<'EOF'
feat(editor): add fallback_block_view for unclassifiable blocks

Renders visible content (flat text), an inline warning banner, and a
PointerDown handler that sets focus so the toolbar mounts — giving the
user Change Type / Delete to recover. Read-only; no on_action parameter.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Wire the four `empty()`-on-mismatch sites to the fallback

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

**Goal:** Replace every `empty()`-on-mismatch render path with `fallback_block_view`. Add `#[cfg(debug_assertions)] eprintln!` logging at each site in debug builds.

- [ ] **Step 1: Wire the `_ => empty()` kind/body mismatch arm in `mod.rs`**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, locate the body match arm at the bottom of the `let body = match ...` block (line ~148):

Current code:
```rust
        // Body/kind mismatch — render nothing.
        _ => empty().into_any(),
```

Replace with the **complete arm** (including `_ => {` and closing `}`):

```rust
        // Body/kind mismatch — render fallback so content is visible and recoverable.
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] block {:?}: kind/body mismatch ({:?} + {:?})",
                block_id, block.kind, block.body
            );
            fallback::fallback_block_view(block, focus_pub).into_any()
        }
```

Note: `block`, `block_id`, and `focus_pub` are all in scope in `block_view`. No `on_action` parameter on `fallback_block_view`.

- [ ] **Step 2: Wire the `_ => empty()` fallthrough in `plugin.rs`**

In `crates/lopress-editor/src/ui/blocks/plugin.rs`, locate the `render_body` function's match arm at the bottom (line ~380):

Current code:
```rust
        _ => floem::views::empty().into_any(),
```

Replace with:

```rust
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] plugin block {:?}: kind/body mismatch ({:?} + {:?})",
                block.id, block.kind, block.body
            );
            crate::ui::blocks::fallback::fallback_block_view(block, focus_pub).into_any()
        }
```

Note: `block` and `focus_pub` are in scope in `render_body`. Uses the full path `crate::ui::blocks::fallback::fallback_block_view` since `fallback` is not imported into `plugin.rs`.

- [ ] **Step 3: Wire the `list_editor_widget` body-shape guard**

In `crates/lopress-editor/src/ui/blocks/editor_registry.rs`, locate the `list_editor_widget` function's early return (line ~45):

Current code:
```rust
    let BlockBody::List(items) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
```

Replace with:

```rust
    let BlockBody::List(items) = &ctx.block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry list: {:?} has body {:?}",
            ctx.block.id, ctx.block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(
            ctx.block, ctx.focus_pub,
        )
        .into_any();
    };
```

Note: keep the binding named `items` — `let ... else` binds it for the happy path below (the `else` block diverges via `return`), and `items` is used by `editable_list_view` further down. Renaming it would break that call.

- [ ] **Step 4: Wire the `code_editor_widget` body-shape guard**

In `crates/lopress-editor/src/ui/blocks/editor_registry.rs`, locate the `code_editor_widget` function's early return (line ~72):

Current code:
```rust
    let BlockBody::Code(body) = &ctx.block.body else {
        return floem::views::empty().into_any();
    };
```

Replace with:

```rust
    let BlockBody::Code(body) = &ctx.block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] editor_registry code: {:?} has body {:?}",
            ctx.block.id, ctx.block.body
        );
        return crate::ui::blocks::fallback::fallback_block_view(
            ctx.block, ctx.focus_pub,
        )
        .into_any();
    };
```

Note: keep the binding named `body` — `let ... else` binds it for the happy path below (the `else` block diverges via `return`), and `body` is used by `editable_code_view` further down. Renaming it would break that call.

- [ ] **Step 5: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 6: Verify the workspace gate passes**

```
bash scripts/check.sh
```

Expected: all three phases pass. No new warnings.

- [ ] **Step 7: Commit**

```
git add crates/lopress-editor/src/ui/blocks/mod.rs \
        crates/lopress-editor/src/ui/blocks/plugin.rs \
        crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "$(cat <<'EOF'
feat(editor): wire all empty()-on-mismatch sites to fallback_block_view

Every dead-end render path now routes through the fallback: kind/body
mismatch in block_view and render_body, and body-shape guards in
list_editor_widget and code_editor_widget. Debug logging at each site.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Toolbar pre-commit the actual body shape

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`

**Goal:** The Change-Type button currently unconditionally pre-commits an `Inline` body before emitting `ChangeType`, regardless of the focused block's kind. Change it to pre-commit the block's *actual* body shape. For non-inline kinds (Code/List), there is no inline `editor_and_spans` to read, so the pre-commit is a no-op.

- [ ] **Step 1: Add `is_inline_kind` helper**

Add the helper function near the bottom of `crates/lopress-editor/src/ui/toolbar.rs` (after `same_kind`):

```rust
/// True when `kind` is inline-bodied (Paragraph or Heading). These are the
/// only kinds whose editor publishes `editor_and_spans`; the toolbar's
/// pre-commit reads from that signal. For non-inline kinds (Code, List) the
/// signal is None (or stale from a different block), so pre-committing an
/// Inline body would be wrong — it's the exact regression this fixes.
fn is_inline_kind(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_))
}
```

- [ ] **Step 2: Update the type-selector button's PointerDown handler**

In `crates/lopress-editor/src/ui/toolbar.rs`, locate the type-selector button's `PointerDown` handler (around line 78). The current body:

```rust
            .on_event_stop(EventListener::PointerDown, move |_| {
                // Commit current editor text before changing kind.
                if let Some((editor_sig, spans_sig, _, _)) =
                    focus_pub.editor_and_spans.get_untracked()
                {
                    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                    let spans = spans_sig.get_untracked();
                    let rope = lapce_xi_rope::Rope::from(text.as_str());
                    let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                    on_action_for_btn(BlockAction::EditBlockBody {
                        block_id,
                        new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                    });
                }
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_kind: kind_for_action.clone(),
                });
            })
```

Replace with:

```rust
            .on_event_stop(EventListener::PointerDown, move |_| {
                // Pre-commit the current block's body shape before changing kind.
                // Only inline-bodied blocks (Paragraph/Heading) have an active
                // editor_and_spans to read; for non-inline kinds (Code/List) the
                // editor_and_spans is None (or belongs to a different block), so
                // we skip the pre-commit — the body shape already matches what
                // ChangeType expects.
                if is_inline_kind(&current_kind) {
                    if let Some((editor_sig, spans_sig, _, _)) =
                        focus_pub.editor_and_spans.get_untracked()
                    {
                        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                        let spans = spans_sig.get_untracked();
                        let rope = lapce_xi_rope::Rope::from(text.as_str());
                        let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                        on_action_for_btn(BlockAction::EditBlockBody {
                            block_id,
                            new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                        });
                    }
                }
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_kind: kind_for_action.clone(),
                });
            })
```

- [ ] **Step 3: Write the unit test**

Append the following test module at the end of `crates/lopress-editor/src/ui/toolbar.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inline_kind_paragraph() {
        assert!(is_inline_kind(&BlockKind::Paragraph));
    }

    #[test]
    fn is_inline_kind_heading() {
        assert!(is_inline_kind(&BlockKind::Heading(1)));
        assert!(is_inline_kind(&BlockKind::Heading(6)));
    }

    #[test]
    fn is_inline_kind_not_code() {
        assert!(!is_inline_kind(&BlockKind::Code { lang: Rc::from("") }));
    }

    #[test]
    fn is_inline_kind_not_list() {
        assert!(!is_inline_kind(&BlockKind::List { ordered: false }));
        assert!(!is_inline_kind(&BlockKind::List { ordered: true }));
    }

    #[test]
    fn is_inline_kind_not_opaque() {
        assert!(!is_inline_kind(&BlockKind::Opaque { type_name: Rc::from("video") }));
    }
}
```

- [ ] **Step 4: Verify tests pass**

```
cargo test -p lopress-editor is_inline_kind
```

Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 5: Verify the workspace gate passes**

```
bash scripts/check.sh
```

Expected: all three phases pass. No new warnings.

- [ ] **Step 6: Commit**

```
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "$(cat <<'EOF'
fix(editor): pre-commit actual body shape in toolbar, not unconditional Inline

The Change-Type button previously pre-committed an Inline body regardless
of the focused block's kind, which for Code/List blocks would produce a
stale EditBlockBody{Inline} that overwrites the correct body shape. Now
it only pre-commits when the current kind is inline-bodied (Paragraph/Heading).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: FocusLost suppression via `current_doc`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

**Goal:** The `commit_from_editor` closure (called on FocusLost) always emits `EditBlockBody{Inline}`. Give it access to `current_doc` so it can skip committing when the block's kind is no longer inline-bodied (Paragraph/Heading) — i.e. a `ChangeType` swapped the kind under it.

- [ ] **Step 1: Modify the commit closure to check the current kind**

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, locate the `editable_inline` function and the commit closure creation (around line 186). The current code:

```rust
    let commit: CommitClosure = Rc::new(move || {
        commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
    });
```

Replace with:

```rust
    let commit: CommitClosure = Rc::new(move || {
        // Suppress the commit when the block's kind is no longer inline-bodied.
        // A ChangeType swaps the kind from Paragraph/Heading to Code/List
        // while this editor is still mounted; the FocusLost that follows
        // would emit a stale EditBlockBody{Inline} that overwrites the
        // correct body shape for Code/List blocks.
        let should_commit = current_doc.with_untracked(|maybe| {
            maybe.and_then(|doc| {
                doc.blocks.iter().find(|b| b.id == block_id).map(|b| {
                    matches!(b.kind, BlockKind::Paragraph | BlockKind::Heading(_))
                })
            })
        });
        if should_commit.unwrap_or(false) {
            commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
        }
    });
```

Note: `should_commit` is `Option<bool>` because we may not find the block (e.g. it was deleted). We default to committing when we can't determine — this preserves the existing behavior for edge cases.

- [ ] **Step 2: Verify it compiles**

```
cargo check -p lopress-editor
```

Expected: no errors.

- [ ] **Step 3: Verify the workspace gate passes**

```
bash scripts/check.sh
```

Expected: all three phases pass. No new warnings.

- [ ] **Step 4: Commit**

```
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "$(cat <<'EOF'
fix(editor): suppress FocusLost commit when kind changed under the editor

A ChangeType triggers current_doc.update() which rebuilds the editor pane,
unmounting the old inline editor and firing FocusLost. The FocusLost closure
now checks whether the block's kind is still inline-bodied (Paragraph/Heading)
before committing — suppressing the stray EditBlockBody{Inline} that would
overwrite the correct body shape for Code/List blocks.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `debug_assert` with built-in-provenance gate at the `apply` coercion chokepoint

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs`
- Modify: `crates/lopress-editor/tests/actions_tests.rs`
- Modify: `crates/lopress-editor/tests/undo_tests.rs`
- Modify: `crates/lopress-editor/tests/list_action_tests.rs`
- Modify: `crates/lopress-editor/src/ctrl/mod.rs` (pattern `..` updates)

**Goal:** Add a `debug_assert!` in `apply_edit_block_body` that fires when an `EditBlockBody` arrives shape-mismatched with the block's kind. Gate it on built-in-provenance: the commit is only asserted when it came from a built-in widget path, not from external input (control server, tests).

**The `built_in` field:**

Add `built_in: bool` to `BlockAction::EditBlockBody`:

```rust
    EditBlockBody {
        block_id: BlockId,
        new_body: Box<BlockBody>,
        /// True when this commit originates from a built-in editor widget
        /// (paragraph, heading, code, list). Used by the debug_assert in
        /// apply_edit_block_body to distinguish internal regressions from
        /// plugin-originated input — plugin commits always degrade gracefully.
        built_in: bool,
    },
```

**Provenance rule (decisive):**

- `built_in: true` — fresh in-process built-in widget commits:
  1. `src/ui/toolbar.rs` — THREE sites (~lines 78, ~169, ~193): the type-selector pre-commit, the URL commit, and the URL remove commit. All three construct `EditBlockBody { Inline(...) }`.
  2. `src/ui/blocks/inline_editor.rs` — `commit_from_editor` function: emits `EditBlockBody { Inline(...) }`.
  3. `src/ui/blocks/code_editor.rs` — `make_code_commit` function: emits `EditBlockBody { Code(...) }`.
  4. `src/ui/blocks/list.rs` — `commit_live_if_changed`, `emit_list_commit`, structural-key Enter/Backspace arms: emit `EditBlockBody { List(...) }`.
  5. `src/ui/editing/focus.rs` — `focus_block_for` matches `EditBlockBody { .. }` but does NOT construct one; no change needed.

- `built_in: false` — external / non-widget sources:
  1. `src/ctrl/mod.rs` `into_block_action` — the `EditInline` and `EditCode` translations (lines ~147-154). Control input is external HTTP.
  2. `src/actions.rs` `apply_edit_block_body` — the record/inverse construction (~lines 703/707). These are history records, not fresh commits; `false` keeps undo/redo replay from tripping the assert.
  3. All test files — `tests/actions_tests.rs`, `tests/undo_tests.rs`, `tests/list_action_tests.rs`. Tests drive the model directly / simulate external input, and several deliberately feed mismatches to test coercion.

**Pattern match updates:**

Every `match` arm that destructures `EditBlockBody` must add `..` to remain compilable. The real sites:
- `src/ctrl/mod.rs` tests (~lines 670, 692): `BlockAction::EditBlockBody { block_id, ref new_body, .. }` — already has `..`.
- `tests/undo_tests.rs`: `BlockAction::EditBlockBody { ref new_body, .. }` — already has `..`.
- `tests/actions_tests.rs`: `BlockAction::EditBlockBody { block_id, new_body }` — needs `..` for destructuring.

**The assert logic:**

In `apply_edit_block_body`, compute the mismatch on the INCOMING `new_body` vs `block.kind` BEFORE coercion:

```rust
fn apply_edit_block_body(
    doc: &mut EditorDoc,
    id: BlockId,
    new_body: BlockBody,
    built_in: bool,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    // Debug assertion: catch shape-mismatched commits from built-in sources.
    // (`get_mut` because `block.body` is mutated below; the assert and
    // `coerce_body_to_kind` only take immutable reborrows of `block`.)
    // Plugin-originated input (built_in: false) always degrades gracefully —
    // it can carry any body shape from a third-party editor. Built-in widgets
    // (built_in: true) should only emit bodies matching the block's kind; a
    // mismatch here is an internal regression.
    debug_assert!(
        !(built_in && !body_matches_kind(&block.kind, &new_body)),
        "built-in EditBlockBody mismatch: block {:?} kind {:?}, body {:?}",
        id,
        block.kind,
        new_body
    );
    // Coerce the incoming body to the block's kind so a stale or out-of-order
    // commit can never leave the block in an unrenderable shape. See
    // `coerce_body_to_kind`.
    let new_body = canonicalize_body(&coerce_body_to_kind(&block.kind, new_body));
    if canonicalize_body(&block.body) == new_body {
        return None;
    }
    let old_body = std::mem::replace(&mut block.body, new_body.clone());
    Some((
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(new_body),
            built_in: false, // Record/inverse: external provenance.
        },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(old_body),
            built_in: false, // Record/inverse: external provenance.
        },
    ))
}
```

Add the helper function:

```rust
/// True when `body` is the expected shape for `kind`. Used by the
/// debug_assert in apply_edit_block_body to distinguish valid from
/// mismatched commits.
fn body_matches_kind(kind: &BlockKind, body: &BlockBody) -> bool {
    matches!(
        (kind, body),
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_))
            | (BlockKind::Code { .. }, BlockBody::Code(_))
            | (BlockKind::List { .. }, BlockBody::List(_))
            | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
    )
}
```

**Update the `apply` function match arm:**

In `apply`, the `EditBlockBody` arm must destructure `built_in` and pass it through:

```rust
        BlockAction::EditBlockBody { block_id, new_body, built_in } => {
            apply_edit_block_body(doc, block_id, *new_body, built_in)
        }
```

**Resolve the contradiction in prose:**

Because control-server input (`ctrl/mod.rs`) and tests all use `built_in: false`, neither the live control-server scenarios (Task 8) nor the coercion unit tests trip the assert. The assert fires ONLY when an in-process built-in widget emits a wrong-shaped body — exactly the regression class. **Do NOT write a unit test that triggers the assert** (it is a debug panic; per spec §6.3 it is a CI tripwire, not an e2e/unit assertion).

**Test files — add `built_in: false` to every `EditBlockBody` literal:**

- `tests/actions_tests.rs`: Every `BlockAction::EditBlockBody { block_id, new_body, .. }` literal. Search for `BlockAction::EditBlockBody {` — there are ~15 construction sites in this file.
- `tests/undo_tests.rs`: Every `BlockAction::EditBlockBody { block_id, new_body, .. }` literal — 5 construction sites.
- `tests/list_action_tests.rs`: Every `BlockAction::EditBlockBody { block_id, new_body, .. }` literal — 2 construction sites.
- `src/ctrl/mod.rs` tests: Already use `BlockAction::EditBlockBody { block_id, ref new_body, .. }` for destructuring (no construction), so no change needed.

**Coercion tests:**

Append the following tests to `crates/lopress-editor/tests/actions_tests.rs`:

```rust
#[test]
fn coerce_body_to_kind_inline_to_code_preserves_text() {
    // Regression: a stale Inline commit on a Code block should coerce to
    // Code body, preserving the text, not leave {kind: Code, body: Inline}.
    let (id, block) = paragraph_with_id("hello world");
    let mut doc = doc_with(vec![block]);

    // First, change the kind to Code.
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::Code { lang: Rc::from("") },
        },
    );
    assert!(matches!(&doc.blocks[0].body, BlockBody::Code(t) if t == "hello world"));

    // Now apply a stale Inline body (the regression scenario).
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("stale")])),
            built_in: false,
        },
    );
    // Coercion should have converted the Inline to Code, preserving "stale".
    assert!(matches!(&doc.blocks[0].body, BlockBody::Code(t) if t == "stale"));
}

#[test]
fn coerce_body_to_kind_inline_to_list_preserves_text() {
    let (id, block) = paragraph_with_id("line1\nline2");
    let mut doc = doc_with(vec![block]);

    // Change to List.
    apply(
        &mut doc,
        BlockAction::ChangeType {
            block_id: id,
            new_kind: BlockKind::List { ordered: false },
        },
    );

    // Stale Inline commit.
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(vec![InlineRun::plain("line1\nline2")])),
            built_in: false,
        },
    );
    // Should coerce to List with one item per line.
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!("expected List body");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].runs, vec![InlineRun::plain("line1")]);
    assert_eq!(items[1].runs, vec![InlineRun::plain("line2")]);
}

#[test]
fn coerce_body_to_kind_matching_body_unchanged() {
    // When the body shape already matches the kind, coercion is a no-op.
    let (id, block) = paragraph_with_id("hello");
    let mut doc = doc_with(vec![block]);

    let initial_body = doc.blocks[0].body.clone();
    apply(
        &mut doc,
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(initial_body.clone()),
            built_in: false,
        },
    );
    // The body should be unchanged (canonicalization may normalize runs).
    // What matters is no panic and no silent data loss.
    assert_eq!(doc.blocks.len(), 1);
}
```

**Commit message:**

```
git add crates/lopress-editor/src/actions.rs \
        crates/lopress-editor/tests/actions_tests.rs \
        crates/lopress-editor/tests/undo_tests.rs \
        crates/lopress-editor/tests/list_action_tests.rs \
        crates/lopress-editor/src/ctrl/mod.rs
git commit -m "$(cat <<'EOF'
feat(editor): add debug_assert for built-in EditBlockBody shape mismatches

Gate on built_in: true (built-in widget provenance). Plugin-originated
input (ctrl server, tests) uses built_in: false and always degrades
gracefully. The assert catches internal regressions where a built-in
widget emits a body shape that doesn't match the block's kind.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Harden `from_core` for unclassifiable blocks

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`
- Modify: `crates/lopress-editor/tests/from_to_core_tests.rs`

**Goal:** Ensure no `unwrap`, `expect`, or `panic` on disk-sourced data in `from_core`. Route every unclassifiable block through the fallback view.

- [ ] **Step 1: Audit `from_core.rs` for `unwrap`/`expect`/`panic`**

The file `crates/lopress-editor/src/model/from_core.rs` uses only `unwrap_or` / `unwrap_or_else` / `and_then` — all defensive. No `unwrap()` on disk data. The `Block` JSON is serialized via `serde_json::to_value(b).unwrap_or(serde_json::Value::Null)` in the `Opaque` path. This is safe.

- [ ] **Step 2: Wire the `Opaque` render arm to the fallback**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, locate the `Opaque` arm in the body match (line ~143):

Current code:
```rust
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            opaque::render_opaque(type_name, value).into_any()
        }
```

Replace with:

```rust
        (BlockKind::Opaque { .. }, BlockBody::Opaque(_)) => {
            // Opaque blocks load from disk with unknown/removed plugin types.
            // Route through the fallback so they're visible and recoverable,
            // not a silent drop or a read-only card with no toolbar.
            fallback::fallback_block_view(block, focus_pub).into_any()
        }
```

This makes Opaque blocks consistent with mismatched blocks — all unclassifiable blocks render through the same recoverable view.

- [ ] **Step 3: Write tests for unknown plugin / malformed attrs loading**

Append the following tests to `crates/lopress-editor/tests/from_to_core_tests.rs`. These use the **real** `lopress_core` types (`Block`, `Document`, `FrontMatter`) and the **real** entry points (`doc_from_core`, `doc_to_core`):

```rust
#[test]
fn unknown_block_type_loads_as_opaque_no_panic() {
    // A block type that is neither built-in nor in the registry must load
    // as Opaque without panicking. The body contains verbatim JSON so the
    // fallback view can render it.
    let unknown_block = Block {
        r#type: "unknown:foobar".into(),
        attrs: json!({ "foo": "bar" }),
        children: vec![],
        text: Some("raw text content".to_string()),
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![unknown_block],
    };

    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "unknown:foobar"
    ));
    assert!(matches!(&editor.blocks[0].body, BlockBody::Opaque(v) if v.get("text").and_then(Value::as_str) == Some("raw text content")));
}

#[test]
fn malformed_attrs_loads_as_opaque_no_panic() {
    // A block with attrs that can't be parsed as an object should still
    // load without panicking — serde_json::to_value handles any Block.
    let malformed_block = Block {
        r#type: "weird:block".into(),
        attrs: json!("not-an-object"),  // malformed: attrs should be an object
        children: vec![],
        text: None,
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![malformed_block],
    };

    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "weird:block"
    ));
}

#[test]
fn unregistered_plugin_type_loads_as_opaque() {
    // A block type matching a plugin namespace but not registered in the
    // current registry must load as Opaque, not panic.
    let custom_block = Block {
        r#type: "lopress:video".into(),
        attrs: json!({ "src": "video.mp4" }),
        children: vec![],
        text: None,
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![custom_block],
    };

    // Use an empty registry — no plugins registered.
    let registry = PluginRegistry::default();
    let editor = doc_from_core(&core, &registry);
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "lopress:video"
    ));

    // Round-trip: the Opaque body preserves the original JSON.
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}
```

Note: These tests use the real `lopress_core` types (`Block`, `Document`, `FrontMatter`) and the real `doc_from_core`/`doc_to_core` functions. The existing test file already imports these and uses them in the same patterns.

- [ ] **Step 4: Verify the workspace gate passes**

```
bash scripts/check.sh
```

Expected: all three phases pass. No new warnings.

- [ ] **Step 5: Commit**

```
git add crates/lopress-editor/src/model/from_core.rs \
        crates/lopress-editor/src/ui/blocks/mod.rs \
        crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "$(cat <<'EOF'
feat(editor): harden from_core — no panics on disk data, Opaque routes through fallback

All unclassifiable blocks (unknown types, malformed attrs, unregistered
plugins) now load as Opaque blocks that render through the fallback view
— visible content, warning banner, focusable for toolbar recovery. No
panics or dropped blocks on disk-sourced data.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Live control-server e2e verification

**Files:**
- None (uses the `driving-lopress-editor` skill)

**Goal:** Verify all e2e scenarios through the debug control server. These are *not* cargo tests — their "Done when" is the observed `/state`/`/screenshot` result.

**Prerequisites:** `cargo run` (debug, never `--release`) to start the editor.

**Protocol facts (verified from `src/ctrl/mod.rs`):**

- `POST /action` accepts `CtrlAction` JSON (`#[serde(tag = "type")]`):
  - `{"type":"Split","block_id":<u64>,"byte_offset":<usize>}`
  - `{"type":"MergeWithPrev","block_id":<u64>}`
  - `{"type":"Delete","block_id":<u64>}`
  - `{"type":"Move","block_id":<u64>,"to_index":<usize>}`
  - `{"type":"ChangeType","block_id":<u64>,"new_kind":<CtrlBlockKind>}` where `CtrlBlockKind` is `#[serde(tag="type")]`: `{"type":"Paragraph"}` | `{"type":"Heading","level":<u8>}` | `{"type":"Code","lang":"<str>"}` | `{"type":"List","ordered":<bool>}`
  - `{"type":"EditInline","block_id":<u64>,"new_runs":[<InlineRun>...]}` where `InlineRun` fields are: `{text, bold, italic, code, link}`
  - `{"type":"EditCode","block_id":<u64>,"new_text":"<str>"}`
  - `{"type":"EditAttrs","block_id":<u64>,"new_attrs":{<obj>}}`

- `GET /state` returns flat schema:
  ```json
  {"doc_open":bool,"path":str|null,"blocks":[ {block} ]}
  ```
  Each block is one of:
  - inline: `{"id":<u64>,"kind":"Paragraph"|"Heading<N>"|"Code"|"List"|"Opaque(<name>)","text":"<flattened>"}`
  - code: `{"id":<u64>,"kind":"Code","lang":"<str>","text":"<code>"}`
  - list: `{"id":<u64>,"kind":"List","text":"<items joined by \n>"}`
  - opaque: `{"id":<u64>,"kind":"Opaque(<name>)","text":""}`

- `block_id` is the raw `u64` (`b.id.raw()`), not a struct.

- `POST /open` accepts `{"path":"<file_path>"}` — opens an existing file. There is NO way to inject an arbitrary block via the control API.

- `GET /screenshot` returns a PNG — visual check only.

**Important limitation:** The control API CANNOT inject a mismatched block or an Opaque block directly. There is no `InsertAfter` action and no way to set `block.kind`/`block.body` arbitrarily. The only actions are `EditInline`, `EditCode`, `EditAttrs`, `ChangeType`, `Split`, `MergeWithPrev`, `Delete`, `Move`.

---

#### 8a. Coercion / ordering end-state (proves the regression is gone)

This scenario drives the full regression sequence through the control server. Because all control input is `built_in: false`, no assert fires. The model guard (coercion) fixes the mismatch at apply, so the block never reaches the renderer mismatched via this path — this scenario proves the regression is gone.

- [ ] **Step 1: Create a paragraph and drive it through the full sequence**

```powershell
# 1. Start with a paragraph via EditInline (creates block 1 with Inline body)
$body = '{"type":"EditInline","block_id":1,"new_runs":[{"text":"my paragraph text","bold":false,"italic":false,"code":false,"link":null}]}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method POST -ContentType 'application/json' -Body $body
```

- [ ] **Step 2: Change type to Code (triggers ChangeType → model converts body to Code)**

```powershell
$body = '{"type":"ChangeType","block_id":1,"new_kind":{"type":"Code","lang":""}}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method POST -ContentType 'application/json' -Body $body
```

- [ ] **Step 3: Inject a trailing EditInline (the would-be stale commit)**

```powershell
$body = '{"type":"EditInline","block_id":1,"new_runs":[{"text":"my paragraph text","bold":false,"italic":false,"code":false,"link":null}]}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method POST -ContentType 'application/json' -Body $body
```

- [ ] **Step 4: Assert /state shows the block as Code with text preserved**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:7878/state | ConvertTo-Json
```

Expected `/state`:
```json
{
  "doc_open": true,
  "path": "...",
  "blocks": [
    {"id": 1, "kind": "Code", "lang": "", "text": "my paragraph text"}
  ]
}
```

The text is preserved because coercion converts the stale Inline to Code. The `built_in: false` from control input means no assert fires.

---

#### 8b. Fallback view renders for Opaque blocks (via fixture file + /open)

The control API cannot inject Opaque blocks. Drive this via `POST /open` on a fixture document authored with an unknown block type, which `from_core` yields as an `Opaque` block. Task 7 routes Opaque through the fallback.

- [ ] **Step 1: Create a fixture document with an unknown plugin block**

Create `crates/lopress-editor/tests/fixtures/unknown_plugin.lopress`:

```
---
title: Unknown Plugin Test
---
# Heading

<!-- lopress:video {"src":"unknown.mp4"} -->
video content here
<!-- /lopress:video -->

After the video.
```

- [ ] **Step 2: Open the fixture via the control server**

```powershell
$body = '{"path":"crates/lopress-editor/tests/fixtures/unknown_plugin.lopress"}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method POST -ContentType 'application/json' -Body $body
```

Wait — `POST /action` does not open documents. The `/open` endpoint is `POST /open`:

```powershell
$body = '{"path":"crates/lopress-editor/tests/fixtures/unknown_plugin.lopress"}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/open -Method POST -ContentType 'application/json' -Body $body
```

- [ ] **Step 3: Assert /state shows the Opaque block present (not dropped)**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:7878/state | ConvertTo-Json
```

Expected: the `lopress:video` block is present with `kind: "Opaque(lopress:video)"` and `text: ""`. The block is NOT dropped — it loaded through the `from_core` path as `Opaque`.

- [ ] **Step 4: Visual check — screenshot shows fallback view**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:7878/screenshot -OutFile screenshot.png
```

Expected: the screenshot shows the fallback card with warning banner and the raw content visible. Clicking the block should mount the toolbar (Change Type / Delete).

- [ ] **Step 5: Recovery — ChangeType to Paragraph**

```powershell
$body = '{"type":"ChangeType","block_id":2,"new_kind":{"type":"Paragraph"}}'
Invoke-RestMethod -Uri http://127.0.0.1:7878/action -Method POST -ContentType 'application/json' -Body $body
```

- [ ] **Step 6: Assert /state shows kind flipped to Paragraph**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:7878/state | ConvertTo-Json
```

Expected: block 2 now shows `kind: "Paragraph"` — the Opaque block recovered through ChangeType.

---

#### 8c. Dead-end parity — plugin editor-key-with-no-widget

This block cannot be synthesized through the control API or a plain fixture (the control API has no way to inject a `plugin` field or an arbitrary `Editor` key). It is **not a separate live scenario**. The wiring is covered by Task 3's `editor_registry` changes (the `editor_for("bogus")` returns `None`, so the code falls through to the `_ => { eprintln!; fallback_block_view(...) }` arm). This is verified by the screenshot check above and the `editor_registry` unit test which already passes (`editor_for("bogus").is_none()`).

---

## Done when

All eight tasks are complete: the workspace gate (`bash scripts/check.sh`) passes for every task, the fallback view exists and is wired to all dead-end render paths, the toolbar pre-commits the actual shape, FocusLost is suppressed when kind changed, the `debug_assert` fires on built-in mismatches, `from_core` is hardened, and all e2e scenarios pass through the control server.

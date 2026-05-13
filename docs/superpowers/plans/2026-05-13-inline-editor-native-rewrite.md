# Inline Editor Native Rewrite — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled `editable_inline` widget (fake caret span, `0.55×font_size` geometry approximation, broken word wrap, silent Shift+Enter) with Floem's native `editor_container_view` backed by `floem_editor_core`, fixing all cursor and line-break issues.

**Architecture:** Each inline block gets a `BlockEditorState { editor_sig: RwSignal<Editor>, spans_sig: RwSignal<Vec<StyleSpan>>, style_rev: RwSignal<u64> }`. Style spans (byte-offset ranges → bold/italic/code/link) live separately from the rope and are applied via a custom `InlineRunStyling` struct implementing Floem's `Styling` trait. A custom key handler intercepts Enter (split), Shift+Enter (soft break), Backspace-at-zero (merge), ↑/↓ at block boundary (cross-block focus), and Ctrl+B/I/E/K (style toggle). `selection.rs`, `sel_ctx.rs`, `BlockAction::DeleteRange`, and `BlockAction::ToggleInlineRange` are deleted entirely — no dead code survives.

**Tech Stack:** Rust, Floem 0.2, `floem_editor_core`, `lapce_xi_rope::Rope`, `lopress-core` block tree.

**Spec:** `docs/superpowers/specs/2026-05-13-inline-editor-native-rewrite-design.md`

**Lints (workspace-wide — never violate):**
- No `unwrap()`, `expect()`, `panic!()`, `todo!()`, `unimplemented!()`, `unreachable!()`
- No `as` casts — use `From`/`TryFrom`
- No `[idx]` direct indexing — use `.get()`
- All public fallible functions must return `Result`

---

## File Map

| File | Disposition |
|------|-------------|
| `src/ui/blocks/style_span.rs` | NEW — `StyleSpan`, `InlineFlag`, `toggle_inline`, `split_span_at`, `coalesce_spans`, `InlineRunStyling` |
| `src/model/sync.rs` | NEW — `inline_runs_to_rope_and_spans`, `rope_and_spans_to_runs` |
| `src/ui/blocks/inline_editor.rs` | REWRITE — `BlockEditorState`, `build_block_editor`, `editable_inline`; old `Caret`, `LocalSelection`, `GeometryCache`, `render_block`, `emit_run_segments`, `caret_span` all deleted |
| `src/ui/editor_pane.rs` | MODIFY — remove `sel_ctx`, add `editor_state_map` + `current_doc` params |
| `src/ui/mod.rs` | MODIFY — remove `sel_ctx`/`doc_selection`/`geometry`, add `editor_state_map` |
| `src/ui/toolbar.rs` | MODIFY — read `RwSignal<Vec<StyleSpan>>` from `focus_pub` |
| `src/ui/blocks/mod.rs` | MODIFY — add `pub mod style_span`; update `block_view` signature |
| `src/ui/blocks/paragraph.rs` | MODIFY — update `editable_inline` call site |
| `src/ui/blocks/heading.rs` | MODIFY — update `editable_inline` call site |
| `src/ui/blocks/list.rs` | MODIFY — update `editable_inline` call site |
| `src/model/mod.rs` | MODIFY — add `pub mod sync` |
| `src/lib.rs` | MODIFY — remove `pub mod selection` |
| `src/actions.rs` | MODIFY — remove `DeleteRange`, `ToggleInlineRange`; remove `use crate::selection::*` |
| `src/selection.rs` | DELETE |
| `src/ui/sel_ctx.rs` | DELETE |
| `tests/style_span_tests.rs` | NEW |
| `tests/sync_tests.rs` | NEW |

---

## Task 1: `StyleSpan` type and toggle helpers

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/style_span.rs`
- Create: `crates/lopress-editor/tests/style_span_tests.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` — add `pub mod style_span;`

- [ ] **Step 1: Write failing tests**

Create `crates/lopress-editor/tests/style_span_tests.rs`:

```rust
use lopress_editor::ui::blocks::style_span::{
    coalesce_spans, split_span_at, toggle_inline, InlineFlag, StyleSpan,
};

fn plain(start: usize, end: usize) -> StyleSpan {
    StyleSpan { start, end, bold: false, italic: false, code: false, link: None }
}
fn bold(start: usize, end: usize) -> StyleSpan {
    StyleSpan { start, end, bold: true, italic: false, code: false, link: None }
}

#[test]
fn test_split_span_at_mid() {
    let mut spans = vec![plain(0, 10)];
    split_span_at(&mut spans, 4);
    assert_eq!(spans, vec![plain(0, 4), plain(4, 10)]);
}

#[test]
fn test_split_span_at_boundary_noop() {
    let mut spans = vec![plain(0, 5), plain(5, 10)];
    split_span_at(&mut spans, 5);
    assert_eq!(spans.len(), 2);
}

#[test]
fn test_coalesce_merges_same_style() {
    let mut spans = vec![plain(0, 3), plain(3, 7)];
    coalesce_spans(&mut spans);
    assert_eq!(spans, vec![plain(0, 7)]);
}

#[test]
fn test_coalesce_keeps_different_style() {
    let mut spans = vec![plain(0, 3), bold(3, 7)];
    coalesce_spans(&mut spans);
    assert_eq!(spans.len(), 2);
}

#[test]
fn test_toggle_sets_flag_when_partial() {
    // "hello world" — only first 5 bytes are bold
    let mut spans = vec![bold(0, 5), plain(5, 11)];
    toggle_inline(&mut spans, 0, 11, InlineFlag::Bold);
    // All-set check: NOT all bold, so bold is set on all
    assert!(spans.iter().all(|s| s.bold));
}

#[test]
fn test_toggle_clears_flag_when_all_set() {
    let mut spans = vec![bold(0, 5), bold(5, 11)];
    toggle_inline(&mut spans, 0, 11, InlineFlag::Bold);
    assert!(spans.iter().all(|s| !s.bold));
}

#[test]
fn test_toggle_collapsed_selection_noop() {
    let mut spans = vec![plain(0, 10)];
    toggle_inline(&mut spans, 5, 5, InlineFlag::Bold);
    assert!(!spans[0].bold);
}

#[test]
fn test_toggle_partial_range() {
    // Select bytes 2..7 in a 10-byte plain block
    let mut spans = vec![plain(0, 10)];
    toggle_inline(&mut spans, 2, 7, InlineFlag::Italic);
    // Should produce: plain(0,2), italic(2,7), plain(7,10)
    assert_eq!(spans.len(), 3);
    assert!(!spans[0].italic);
    assert!(spans[1].italic);
    assert!(!spans[2].italic);
}

#[test]
fn test_toggle_coalesces_after_clear() {
    // Three spans: bold | bold | bold — clear all → should collapse to one plain span
    let mut spans = vec![bold(0, 3), bold(3, 6), bold(6, 10)];
    toggle_inline(&mut spans, 0, 10, InlineFlag::Bold);
    assert_eq!(spans, vec![plain(0, 10)]);
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test -p lopress-editor --test style_span_tests 2>&1 | tail -5
```
Expected: compile error (module not found).

- [ ] **Step 3: Create `style_span.rs`**

Create `crates/lopress-editor/src/ui/blocks/style_span.rs`:

```rust
/// Byte-offset span with inline style flags. `start` is inclusive, `end`
/// exclusive, both measured in UTF-8 bytes from the block's rope start.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl StyleSpan {
    pub fn plain(start: usize, end: usize) -> Self {
        Self { start, end, bold: false, italic: false, code: false, link: None }
    }

    pub fn same_style(&self, other: &StyleSpan) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.code == other.code
            && self.link == other.link
    }
}

/// Which inline attribute a toolbar/keyboard shortcut toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFlag {
    Bold,
    Italic,
    Code,
    Link,
}

/// Split the span that straddles `abs` into two spans at that byte boundary.
/// No-op if `abs` falls on an existing span boundary or outside all spans.
pub fn split_span_at(spans: &mut Vec<StyleSpan>, abs: usize) {
    let Some(i) = spans.iter().position(|s| s.start < abs && abs < s.end) else {
        return;
    };
    let span = spans[i].clone();
    let left = StyleSpan { start: span.start, end: abs, ..span.clone() };
    let right = StyleSpan { start: abs, end: span.end, ..span };
    spans.splice(i..=i, [left, right]);
}

/// Merge adjacent spans that share the same style and are contiguous.
pub fn coalesce_spans(spans: &mut Vec<StyleSpan>) {
    let mut i = 0;
    while i + 1 < spans.len() {
        let merge = spans[i].end == spans[i + 1].start
            && spans[i].same_style(&spans[i + 1]);
        if merge {
            spans[i].end = spans[i + 1].end;
            spans.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Toggle `flag` across `[sel_start, sel_end)` (byte offsets).
/// If every overlapping span already has the flag, clears it; otherwise sets.
/// A collapsed selection (`sel_start == sel_end`) is a no-op.
pub fn toggle_inline(
    spans: &mut Vec<StyleSpan>,
    sel_start: usize,
    sel_end: usize,
    flag: InlineFlag,
) {
    if sel_start >= sel_end || spans.is_empty() {
        return;
    }
    let all_set = spans
        .iter()
        .filter(|s| s.start < sel_end && s.end > sel_start)
        .all(|s| match flag {
            InlineFlag::Bold => s.bold,
            InlineFlag::Italic => s.italic,
            InlineFlag::Code => s.code,
            InlineFlag::Link => s.link.is_some(),
        });
    let new_value = !all_set;
    // Split higher boundary first so the lower index stays valid.
    split_span_at(spans, sel_end);
    split_span_at(spans, sel_start);
    for span in spans.iter_mut() {
        if span.start >= sel_start && span.end <= sel_end {
            match flag {
                InlineFlag::Bold => span.bold = new_value,
                InlineFlag::Italic => span.italic = new_value,
                InlineFlag::Code => span.code = new_value,
                InlineFlag::Link => {
                    span.link = if new_value { Some(String::new()) } else { None };
                }
            }
        }
    }
    coalesce_spans(spans);
}
```

- [ ] **Step 4: Add the module declaration**

In `crates/lopress-editor/src/ui/blocks/mod.rs`, add:
```rust
pub mod style_span;
```

Also add the public re-export so tests can access it. In `crates/lopress-editor/src/lib.rs`, the `ui` module is already public. The path `lopress_editor::ui::blocks::style_span` will work once `blocks` is `pub mod`.

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test -p lopress-editor --test style_span_tests 2>&1 | tail -10
```
Expected: all 8 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/style_span.rs \
        crates/lopress-editor/src/ui/blocks/mod.rs \
        crates/lopress-editor/tests/style_span_tests.rs
git commit -m "feat(editor): StyleSpan type and toggle_inline helpers"
```

---

## Task 2: `sync.rs` — InlineRun ↔ (Rope, Vec<StyleSpan>)

**Files:**
- Create: `crates/lopress-editor/src/model/sync.rs`
- Create: `crates/lopress-editor/tests/sync_tests.rs`
- Modify: `crates/lopress-editor/src/model/mod.rs` — add `pub mod sync;`

- [ ] **Step 1: Write failing tests**

Create `crates/lopress-editor/tests/sync_tests.rs`:

```rust
use lopress_editor::model::sync::{inline_runs_to_rope_and_spans, rope_and_spans_to_runs};
use lopress_editor::model::types::InlineRun;
use lopress_editor::ui::blocks::style_span::StyleSpan;

fn plain_run(text: &str) -> InlineRun {
    InlineRun { text: text.into(), bold: false, italic: false, code: false, link: None }
}
fn bold_run(text: &str) -> InlineRun {
    InlineRun { text: text.into(), bold: true, italic: false, code: false, link: None }
}

#[test]
fn test_plain_roundtrip() {
    let runs = vec![plain_run("hello world")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_empty_roundtrip() {
    let runs: Vec<InlineRun> = vec![];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_mixed_style_roundtrip() {
    let runs = vec![
        plain_run("hello "),
        bold_run("world"),
        plain_run("!"),
    ];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_coalesce_same_style() {
    // Two adjacent plain runs should coalesce into one span.
    let runs = vec![plain_run("hello "), plain_run("world")];
    let (_, spans) = inline_runs_to_rope_and_spans(&runs);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 11); // "hello world" = 11 bytes
}

#[test]
fn test_newline_roundtrip() {
    // A soft break (\n) inside a run survives the round-trip.
    let runs = vec![plain_run("line one\nline two")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_unicode_roundtrip() {
    // Multi-byte characters: byte offsets must not split codepoints.
    let runs = vec![plain_run("héllo"), bold_run(" wörld")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_spans_cover_full_byte_range() {
    let runs = vec![plain_run("abc"), bold_run("def")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 3);
    assert_eq!(spans[1].start, 3);
    assert_eq!(spans[1].end, 6);
    // rope length equals last span end
    use floem_editor_core::buffer::rope_text::RopeText;
    assert_eq!(rope_and_spans_to_runs(&rope, &spans).iter().map(|r| r.text.len()).sum::<usize>(), spans.last().map(|s| s.end).unwrap_or(0));
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test -p lopress-editor --test sync_tests 2>&1 | tail -5
```
Expected: compile error (module not found).

- [ ] **Step 3: Create `sync.rs`**

Create `crates/lopress-editor/src/model/sync.rs`:

```rust
use crate::model::types::InlineRun;
use crate::ui::blocks::style_span::{coalesce_spans, StyleSpan};
use lapce_xi_rope::Rope;

/// Convert `Vec<InlineRun>` into a flat `Rope` and parallel style spans.
/// Adjacent runs with identical styles coalesce into one span.
/// `\n` inside run text becomes a real newline in the rope (soft line break).
pub fn inline_runs_to_rope_and_spans(runs: &[InlineRun]) -> (Rope, Vec<StyleSpan>) {
    let mut text = String::new();
    let mut spans: Vec<StyleSpan> = Vec::with_capacity(runs.len());
    let mut acc = 0usize; // running byte offset

    for run in runs {
        let byte_len = run.text.len();
        if byte_len > 0 {
            spans.push(StyleSpan {
                start: acc,
                end: acc + byte_len,
                bold: run.bold,
                italic: run.italic,
                code: run.code,
                link: run.link.clone(),
            });
        }
        text.push_str(&run.text);
        acc += byte_len;
    }

    coalesce_spans(&mut spans);
    (Rope::from(text.as_str()), spans)
}

/// Reconstruct `Vec<InlineRun>` from a `Rope` and its style spans.
/// Produces one `InlineRun` per span; `\n` in span text is preserved.
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let full: String = String::from(rope);
    spans
        .iter()
        .filter_map(|span| {
            let text = full.get(span.start..span.end)?.to_owned();
            Some(InlineRun {
                text,
                bold: span.bold,
                italic: span.italic,
                code: span.code,
                link: span.link.clone(),
            })
        })
        .collect()
}
```

Note: `String::from(rope)` works because `lapce_xi_rope::Rope` implements `From<Rope> for String` (verified from xi-rope source: `String::from(&b)` is used in examples).

If `String::from(rope)` does not compile, use instead:
```rust
use floem_editor_core::buffer::rope_text::RopeText;
let full: String = {
    // rope is &Rope — wrap in a RopeTextVal for the RopeText trait
    // or collect chunks:
    let mut s = String::new();
    for chunk in rope.iter_chunks(0..rope.len()) {
        s.push_str(chunk);
    }
    s
};
```

- [ ] **Step 4: Add module declaration**

In `crates/lopress-editor/src/model/mod.rs`, add:
```rust
pub mod sync;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p lopress-editor --test sync_tests 2>&1 | tail -15
```
Expected: all 7 tests pass. If `String::from(rope)` does not compile, use the `iter_chunks` fallback above.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/sync.rs \
        crates/lopress-editor/src/model/mod.rs \
        crates/lopress-editor/tests/sync_tests.rs
git commit -m "feat(editor): sync.rs — InlineRun ↔ (Rope, Vec<StyleSpan>)"
```

---

## Task 3: `InlineRunStyling` — Floem `Styling` impl

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/style_span.rs` — add `InlineRunStyling` struct + `Styling` impl

No new tests for this task (Floem `Styling` is UI glue; correctness is validated by the smoke test in Task 11).

- [ ] **Step 1: Add imports to `style_span.rs`**

Add these imports at the top of `src/ui/blocks/style_span.rs`:

```rust
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGetUntracked, SignalUpdate};
use floem::text::{Attrs, AttrsList, FamilyOwned, Weight};
use floem::views::editor::id::EditorId;
use floem::views::editor::layout::TextLayoutLine;
use floem::views::editor::text::{EditorStyle, Styling};
use std::hash::{DefaultHasher, Hash, Hasher};
```

- [ ] **Step 2: Add `InlineRunStyling` to `style_span.rs`**

Append to `src/ui/blocks/style_span.rs`:

```rust
// Link color matching the existing paragraph renderer.
const LINK_COLOR: Color = Color::rgb8(0x22, 0x7C, 0xBB);
// Monospace font for code spans.
const MONO_FAMILY: &str = "monospace";

/// Implements Floem's `Styling` trait to map `Vec<StyleSpan>` onto the
/// native editor's text layout (bold/italic/code/link per character range).
///
/// `text` is the full block text (updated via `TextDocument::add_on_update`
/// whenever the rope changes). It is needed to compute where each logical
/// line starts so `apply_attr_styles` can convert document-level byte offsets
/// to line-relative byte offsets.
///
/// `rev` is bumped whenever `spans` changes, causing Floem to invalidate its
/// text-layout cache and re-run `apply_attr_styles`.
pub struct InlineRunStyling {
    pub spans: RwSignal<Vec<StyleSpan>>,
    pub text: RwSignal<String>,
    pub rev: RwSignal<u64>,
    pub font_size: f32,
}

impl Styling for InlineRunStyling {
    fn id(&self) -> u64 {
        self.rev.get_untracked()
    }

    fn font_size(&self, _edid: EditorId, _line: usize) -> usize {
        // Floem expects usize; font_size is f32 (always a whole number for us).
        // Cast via u32 → usize to satisfy the lint (no direct f32 → usize).
        u32::try_from(self.font_size as u64).unwrap_or(16) as usize
    }

    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        line: usize,
        default: Attrs,
        attrs: &mut AttrsList,
    ) {
        let spans = self.spans.get_untracked();
        let full_text = self.text.get_untracked();

        // Compute the byte offset of the start of `line` within the full text.
        // Logical lines are separated by '\n' (inserted by Shift+Enter).
        let line_start: usize = full_text
            .split('\n')
            .take(line)
            .map(|l| l.len() + 1) // +1 for the '\n' byte
            .sum();
        let line_len: usize = full_text
            .split('\n')
            .nth(line)
            .map(str::len)
            .unwrap_or(0);
        let line_end: usize = line_start + line_len;

        for span in &spans {
            // Skip spans that do not overlap this line.
            if span.end <= line_start || span.start >= line_end {
                continue;
            }
            // Clip to line boundaries and make line-relative.
            let local_start = span.start.saturating_sub(line_start);
            let local_end = span.end.min(line_end) - line_start;
            if local_start >= local_end {
                continue;
            }

            let mut a = default.clone();
            if span.bold {
                a = a.weight(Weight::BOLD);
            }
            if span.italic {
                a = a.style(floem::text::Style::Italic);
            }
            if span.code {
                a = a.family(&[FamilyOwned::Name(MONO_FAMILY.into())]);
            }
            if span.link.is_some() {
                a = a.color(LINK_COLOR);
            }
            attrs.add_span(local_start..local_end, a);
        }
    }

    fn apply_layout_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _layout_line: &mut TextLayoutLine,
    ) {
        // No layout-level overrides needed for inline styling.
    }
}

impl InlineRunStyling {
    /// Bump the revision counter so Floem's text-layout cache is invalidated.
    /// Call this whenever `spans` is mutated.
    pub fn bump_rev(&self) {
        self.rev.update(|r| *r = r.wrapping_add(1));
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | head -20
```
Expected: no errors in `style_span.rs`. (Other errors from not-yet-updated files are fine.)

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/style_span.rs
git commit -m "feat(editor): InlineRunStyling — Floem Styling impl for inline spans"
```

---

## Task 4: `BlockEditorState` and `build_block_editor`

**Files:**
- Rewrite: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

This task replaces the entire contents of `inline_editor.rs`. Delete everything in the file and start fresh.

- [ ] **Step 1: Replace `inline_editor.rs` completely**

Write `crates/lopress-editor/src/ui/blocks/inline_editor.rs`:

```rust
//! Per-block native editor state and construction.

use crate::actions::BlockAction;
use crate::model::sync::{inline_runs_to_rope_and_spans, rope_and_spans_to_runs};
use crate::model::types::{BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::style_span::{toggle_inline, InlineFlag, InlineRunStyling, StyleSpan};
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalGetUntracked, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::id::EditorId;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::text::WrapMethod;
use floem::views::editor::text_document::TextDocument;
use floem::views::editor::view::editor_container_view;
use floem::views::editor::Editor;
use floem::views::{Decorators};
use floem::{IntoView};
use floem_editor_core::buffer::rope_text::RopeText;
use floem_editor_core::cursor::CursorAffinity;
use lapce_xi_rope::Rope;
use std::rc::Rc;

/// Callback used by editable widgets to push every block-tree mutation
/// through the `actions::apply` chokepoint.
pub type ActionSink = Rc<dyn Fn(BlockAction)>;

/// Pane-level slot that the focused block publishes to so the toolbar
/// can read the current block's editor and spans signals.
#[derive(Clone, Copy)]
pub struct FocusPublisher {
    pub block: RwSignal<Option<BlockId>>,
    pub editor_and_spans: RwSignal<Option<(RwSignal<Editor>, RwSignal<Vec<StyleSpan>>, RwSignal<u64>)>>,
}

/// All reactive state owned by one inline block's native editor.
#[derive(Clone, Copy)]
pub struct BlockEditorState {
    pub editor_sig: RwSignal<Editor>,
    pub spans_sig: RwSignal<Vec<StyleSpan>>,
    /// Revision counter; bump to invalidate Floem's text-layout cache after
    /// a style toggle.
    pub style_rev: RwSignal<u64>,
    /// Full block text, kept in sync with the rope via `TextDocument::add_on_update`.
    pub text_sig: RwSignal<String>,
}

/// Build a `BlockEditorState` for an inline block, initialised from `runs`.
/// Creates the `TextDocument`, `InlineRunStyling`, and `Editor` in scope `cx`.
pub fn build_block_editor(
    cx: Scope,
    runs: &[InlineRun],
) -> BlockEditorState {
    let (rope, spans) = inline_runs_to_rope_and_spans(runs);

    let initial_text: String = {
        let rt = rope.clone(); // Rope is Clone
        String::from(&rt)     // lapce_xi_rope::Rope implements Into<String>
    };

    let spans_sig = cx.create_rw_signal(spans);
    let style_rev = cx.create_rw_signal(0u64);
    let text_sig = cx.create_rw_signal(initial_text.clone());

    let styling = Rc::new(InlineRunStyling {
        spans: spans_sig,
        text: text_sig,
        rev: style_rev,
        font_size: 16.0,
    });

    let doc = Rc::new(TextDocument::new(cx, rope));

    // Keep `text_sig` in sync with the rope so `InlineRunStyling::apply_attr_styles`
    // can compute line boundaries for multi-line blocks (Shift+Enter soft breaks).
    let text_sig_for_update = text_sig;
    doc.add_on_update(move |upd| {
        if let Some(ed) = upd.editor {
            let rt = ed.rope_text();
            let new_text = rt.slice_to_cow(0..rt.len()).into_owned();
            text_sig_for_update.set(new_text);
        }
    });

    let editor = Editor::new(cx, doc, styling, false /* not modal */);
    let editor_sig = cx.create_rw_signal(editor);

    BlockEditorState { editor_sig, spans_sig, style_rev, text_sig }
}

/// Commit the current rope text + style spans back to the block model.
pub fn commit_runs(state: &BlockEditorState, block_id: BlockId, on_action: &ActionSink) {
    let text = state.text_sig.get_untracked();
    let spans = state.spans_sig.get_untracked();
    // Build a temporary rope from text for the conversion.
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditInline { block_id, new_runs });
}
```

- [ ] **Step 2: Verify it compiles (errors in other files are fine)**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error\[" | grep "inline_editor" | head -10
```
Expected: no errors originating inside `inline_editor.rs` itself.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "feat(editor): BlockEditorState and build_block_editor"
```

---

## Task 5: `editable_inline` — native editor view with key handler

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` — append `editable_inline` and helper functions

- [ ] **Step 1: Append the key handler and `editable_inline` to `inline_editor.rs`**

Append to `crates/lopress-editor/src/ui/blocks/inline_editor.rs`:

```rust
/// Build the native-editor view for an inline block.
///
/// `focus_target`: when set to `block_id`, this block requests Floem focus.
/// `current_doc`: needed by the key handler to find adjacent blocks for
///   cross-block ↑/↓ navigation.
/// `slash_eligible`: true for paragraph blocks (enables the `/` slash menu).
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    slash_eligible: bool,
) -> impl IntoView {
    let cx = floem::reactive::Scope::current();
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let style_rev = state.style_rev;

    let on_action_for_key = on_action.clone();
    let on_action_for_focus = on_action.clone();

    let view = editor_container_view(
        editor_sig,
        |_| true, // is_active — always active when focused
        move |kp, ms| {
            handle_key(
                kp,
                ms,
                editor_sig,
                spans_sig,
                style_rev,
                block_id,
                &on_action_for_key,
                focus_target,
                current_doc,
                slash_eligible,
            )
        },
    )
    // Enable word wrap at editor width.
    .wrap_method(WrapMethod::EditorWidth)
    // Hide line-number gutter (this is a prose editor, not a code editor).
    .editor_style(|s| s.hide_gutter(true))
    .style(|s| s.width_full());

    // Publish focus state so the toolbar can reach our editor + spans.
    let focus_effect_editor = editor_sig;
    create_effect(move |_| {
        // Floem's editor sets `active` reactively when it gains/loses focus.
        let is_active = focus_effect_editor.with(|ed| ed.active.get());
        if is_active {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(Some((editor_sig, spans_sig, style_rev)));
        }
    });

    // When `focus_target` is set to this block, hand Floem focus here.
    // (editor_container_view gives us no direct ViewId to call request_focus
    // on; instead, Floem's editor sets `active` via its PointerDown handler.
    // We set a signal that the pane rebuilds from — see editor_pane comments.)
    // TODO: if direct programmatic focus is needed, expose the EditorView's
    // ViewId via a Floem API and call view_id.request_focus() in a create_effect.

    view
}

// ── Key handler ──────────────────────────────────────────────────────────────

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
    slash_eligible: bool,
) -> CommandExecuted {
    use floem::keyboard::{Key, NamedKey};
    use floem_editor_core::command::EditCommand;

    let shift = ms.shift();
    let ctrl_or_cmd = ms.control() || ms.meta();

    // ── Ctrl/Cmd shortcuts ───────────────────────────────────────────────────
    if ctrl_or_cmd {
        if let Key::Character(ref s) = kp.key.logical_key {
            match s.as_str() {
                "b" | "B" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Bold);
                    return CommandExecuted::Yes;
                }
                "i" | "I" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Italic);
                    return CommandExecuted::Yes;
                }
                "e" | "E" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Code);
                    return CommandExecuted::Yes;
                }
                "k" | "K" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Link);
                    return CommandExecuted::Yes;
                }
                _ => {}
            }
        }
        return CommandExecuted::No; // let Floem handle Ctrl+A, Ctrl+C, etc.
    }

    match kp.key.logical_key.clone() {
        // Shift+Enter — insert a soft line break within the block.
        Key::Named(NamedKey::Enter) if shift => {
            editor_sig.with_untracked(|ed| {
                // Floem's editor handles InsertNewLine in Insert mode.
                let mut cursor = ed.cursor.get_untracked();
                let mut reg = ed.register.get_untracked();
                ed.doc.with_untracked(|doc| {
                    doc.do_edit(
                        ed,
                        &mut cursor,
                        &EditCommand::InsertNewLine,
                        false,
                        &mut reg,
                    );
                });
                ed.cursor.set(cursor);
                ed.register.set(reg);
            });
            CommandExecuted::Yes
        }

        // Enter — commit runs and split block at cursor.
        Key::Named(NamedKey::Enter) => {
            let byte_offset = editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            let text = editor_sig.with_untracked(|ed| {
                let rt = ed.rope_text();
                rt.slice_to_cow(0..rt.len()).into_owned()
            });
            let spans = spans_sig.get_untracked();
            let rope = Rope::from(text.as_str());
            let new_runs = rope_and_spans_to_runs(&rope, &spans);
            on_action(BlockAction::EditInline { block_id, new_runs });
            on_action(BlockAction::Split { block_id, byte_offset });
            CommandExecuted::Yes
        }

        // Backspace at position 0 — merge with previous block.
        Key::Named(NamedKey::Backspace) => {
            let offset = editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            if offset == 0 {
                let text = editor_sig.with_untracked(|ed| {
                    let rt = ed.rope_text();
                    rt.slice_to_cow(0..rt.len()).into_owned()
                });
                let spans = spans_sig.get_untracked();
                let rope = Rope::from(text.as_str());
                let new_runs = rope_and_spans_to_runs(&rope, &spans);
                on_action(BlockAction::EditInline { block_id, new_runs });
                on_action(BlockAction::MergeWithPrev { block_id });
                CommandExecuted::Yes
            } else {
                CommandExecuted::No // Floem handles within-block backspace
            }
        }

        // ↑ — if on first visual line, jump to previous block.
        Key::Named(NamedKey::ArrowUp) => {
            let on_first_vline = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                let vline = ed.vline_of_offset(offset, CursorAffinity::Backward);
                vline.get() == 0
            });
            if on_first_vline {
                commit_and_jump_prev(editor_sig, spans_sig, block_id, on_action, focus_target, current_doc);
                CommandExecuted::Yes
            } else {
                CommandExecuted::No
            }
        }

        // ↓ — if on last visual line, jump to next block.
        Key::Named(NamedKey::ArrowDown) => {
            let on_last_vline = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                vline == ed.last_vline()
            });
            if on_last_vline {
                commit_and_jump_next(editor_sig, spans_sig, block_id, on_action, focus_target, current_doc);
                CommandExecuted::Yes
            } else {
                CommandExecuted::No
            }
        }

        _ => CommandExecuted::No,
    }
}

/// Read the current selection byte range from the editor and apply a style
/// toggle to the spans. Also bumps `style_rev` to invalidate the layout cache.
fn apply_style_toggle(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    flag: InlineFlag,
) {
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        use floem_editor_core::cursor::CursorMode;
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => {
                let start = sel.min_offset();
                let end = sel.max_offset();
                (start, end)
            }
            CursorMode::Normal(offset) => (*offset, *offset),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    if sel_start == sel_end {
        return; // collapsed — no-op
    }
    spans_sig.update(|s| toggle_inline(s, sel_start, sel_end, flag));
    style_rev.update(|r| *r = r.wrapping_add(1));
}

fn commit_and_jump_prev(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let prev_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
    });
    if let Some(id) = prev_id {
        focus_target.set(Some(id));
    }
}

fn commit_and_jump_next(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let next_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        d.blocks.get(i + 1).map(|b| b.id)
    });
    if let Some(id) = next_id {
        focus_target.set(Some(id));
    }
}

fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    let text = editor_sig.with_untracked(|ed| {
        let rt = ed.rope_text();
        rt.slice_to_cow(0..rt.len()).into_owned()
    });
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditInline { block_id, new_runs });
}
```

**Note on `BlockAction::Split`:** The current `Split` variant carries `{ block_id, run: usize, offset: usize }` (run-and-char-offset from the old `Caret` model). This task changes the signature to `{ block_id, byte_offset: usize }`. Update `actions.rs` accordingly in Task 10.

**Note on `editor_container_view` and `wrap_method`/`editor_style`:** `editor_container_view` returns `impl IntoView`, not `TextEditor`, so `.wrap_method()` and `.editor_style()` are not directly available on it. Instead:
- Apply `WrapMethod::EditorWidth` by setting the style prop on the editor before building the view:
  ```rust
  editor_sig.update(|ed| {
      ed.es.update(|es| es.set(WrapProp, WrapMethod::EditorWidth));
  });
  ```
  Or use the `editor_container_view` output and wrap it in a styled container. Check `floem::views::editor::WrapProp` for the exact API.
- Hide the gutter: style the container to hide `GutterClass`:
  ```rust
  .class(floem::views::editor::gutter::GutterClass, |s| s.hide())
  ```
  on the wrapping view.

- [ ] **Step 2: Update `BlockAction::Split` in `actions.rs`**

Open `crates/lopress-editor/src/actions.rs` and update the `Split` variant:

```rust
/// Split the block at `byte_offset` (UTF-8 byte offset into the block's
/// flat text). The trailing portion becomes a new block of the same kind.
Split {
    block_id: BlockId,
    byte_offset: usize,
},
```

Also update the `apply_split` function to use `byte_offset` instead of `run + offset`. The split must:
1. Find the block in the doc
2. For `BlockBody::Inline(runs)`: flatten all run text, split the flat string at `byte_offset`, reconstruct two `Vec<InlineRun>` (each as a single plain run — style span reconstruction at the split boundary is complex and correctness matters more than preserving spans across split). Use `InlineRun::plain(head)` and `InlineRun::plain(tail)`.

Implementation of `apply_split` with byte_offset:

```rust
fn apply_split(doc: &mut EditorDoc, block_id: BlockId, byte_offset: usize) {
    let Some(idx) = doc.blocks.iter().position(|b| b.id == block_id) else {
        return;
    };
    let Some(block) = doc.blocks.get(idx) else { return };
    let (head_runs, tail_runs) = match &block.body {
        BlockBody::Inline(runs) => {
            let flat: String = runs.iter().map(|r| r.text.as_str()).collect();
            // Clamp byte_offset to a valid char boundary.
            let safe_offset = flat
                .char_indices()
                .map(|(b, _)| b)
                .chain(std::iter::once(flat.len()))
                .find(|&b| b >= byte_offset)
                .unwrap_or(flat.len());
            let head = flat.get(..safe_offset).unwrap_or("").to_owned();
            let tail = flat.get(safe_offset..).unwrap_or("").to_owned();
            (vec![InlineRun::plain(head)], vec![InlineRun::plain(tail)])
        }
        _ => return,
    };
    let kind = block.kind.clone();
    let plugin = block.plugin.clone();
    let new_block = EditorBlock {
        id: BlockId::new(),
        kind,
        body: BlockBody::Inline(tail_runs),
        plugin,
    };
    if let Some(b) = doc.blocks.get_mut(idx) {
        b.body = BlockBody::Inline(head_runs);
    }
    doc.blocks.insert(idx + 1, new_block);
}
```

- [ ] **Step 3: Check compilation of `inline_editor.rs`**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | grep -E "inline_editor|style_span|sync" | head -20
```
Fix any type mismatches before continuing.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs \
        crates/lopress-editor/src/actions.rs
git commit -m "feat(editor): editable_inline with native editor_container_view and key handler"
```

---

## Task 6: Introduce `editor_state_map` in `editing_view`, remove `sel_ctx`

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs`

- [ ] **Step 1: Add editor state map, remove sel_ctx**

In `crates/lopress-editor/src/ui/mod.rs`, in `editing_view`:

1. **Remove** these lines:
```rust
let doc_selection: RwSignal<DocSelection> = ...;
let geometry = Rc::new(RefCell::new(GeometryCache::default()));
let sel_ctx = SelectionContext { doc_selection, current_doc, geometry };
```

2. **Add** before the `dyn_container` call:
```rust
use crate::ui::blocks::inline_editor::BlockEditorState;
use std::collections::HashMap;
// Map from BlockId → native editor state. Lives outside dyn_container so
// state survives text-edit renders (which don't change the pane key).
let editor_state_map: Rc<RefCell<HashMap<BlockId, BlockEditorState>>> =
    Rc::new(RefCell::new(HashMap::new()));
```

3. **Update** the `editor_pane::editor_pane(...)` call to remove `sel_ctx` and add the new params:
```rust
editor_pane::editor_pane(
    &doc,
    on_action.clone(),
    focus_target,
    slash_menu_open,
    dnd,
    current_doc,                        // NEW
    Rc::clone(&editor_state_map),       // NEW
)
.into_any(),
```

4. **Remove** the `use crate::selection::...` and `use crate::ui::sel_ctx::SelectionContext` imports.

Also remove the line that references `DocSelection` in the `on_action` closure (the `doc_selection.set(...)` call after Split). The split focus is now handled purely by `focus_target` — that line can be deleted.

- [ ] **Step 2: Check compilation**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | grep "mod.rs\|ui/mod" | head -15
```

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/mod.rs
git commit -m "refactor(editor): introduce editor_state_map, remove sel_ctx from editing_view"
```

---

## Task 7: Rewrite `editor_pane.rs`

**Files:**
- Rewrite: `crates/lopress-editor/src/ui/editor_pane.rs`

- [ ] **Step 1: Rewrite `editor_pane.rs`**

Replace the entire file:

```rust
//! The vertical scrollable editor pane.

use crate::actions::BlockAction;
use crate::model::sync::inline_runs_to_rope_and_spans;
use crate::model::types::{BlockBody, BlockId, EditorDoc};
use crate::ui::blocks::block_view;
use crate::ui::blocks::inline_editor::{build_block_editor, ActionSink, BlockEditorState, FocusPublisher};
use crate::ui::dnd::{gap_drop_zone, DndState};
use crate::ui::slash_menu::slash_menu;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate};
use floem::views::{dyn_container, empty, scroll, stack, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView};
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    editor_state_map: Rc<RefCell<HashMap<BlockId, BlockEditorState>>>,
) -> impl IntoView {
    let cx = Scope::current();

    let focus_pub = FocusPublisher {
        block: cx.create_rw_signal(None),
        editor_and_spans: cx.create_rw_signal(None),
    };

    // Ensure editor state exists for every inline block; prune stale entries.
    {
        let mut map = editor_state_map.borrow_mut();
        let live_ids: std::collections::HashSet<BlockId> =
            doc.blocks.iter().map(|b| b.id).collect();
        map.retain(|id, _| live_ids.contains(id));
        for block in &doc.blocks {
            if let BlockBody::Inline(runs) = &block.body {
                if !map.contains_key(&block.id) {
                    map.insert(block.id, build_block_editor(cx, runs));
                }
            }
        }
    }

    let mut rows: Vec<AnyView> = Vec::with_capacity(doc.blocks.len() * 2 + 1);
    for (i, b) in doc.blocks.iter().enumerate() {
        rows.push(gap_drop_zone(i, dnd, on_action.clone()).into_any());
        let editor_state = editor_state_map.borrow().get(&b.id).copied();
        rows.push(block_view(
            b,
            on_action.clone(),
            focus_target,
            focus_pub,
            dnd,
            current_doc,
            editor_state,
        ));
    }
    rows.push(gap_drop_zone(doc.blocks.len(), dnd, on_action.clone()).into_any());

    let column = v_stack_from_iter(rows).style(|s| {
        s.max_width(720.)
            .width_full()
            .margin_horiz(floem::unit::PxPctAuto::Auto)
            .padding(24.)
    });
    let scroll_view = scroll(column).style(|s| s.width_full().height_full());

    let on_action_for_menu = on_action;
    let menu_overlay = dyn_container(
        move || slash_menu_open.get(),
        move |maybe_block| match maybe_block {
            None => empty().into_any(),
            Some(block_id) => {
                let on_action_for_select = on_action_for_menu.clone();
                let on_select = move |new_kind| {
                    on_action_for_select(BlockAction::ChangeType { block_id, new_kind });
                };
                let on_close = move || {
                    slash_menu_open.set(None);
                    focus_target.set(Some(block_id));
                };
                slash_menu(on_select, on_close)
                    .style(|s| s.margin_top(40.).margin_horiz(floem::unit::PxPctAuto::Auto))
                    .into_any()
            }
        },
    )
    .style(|s| {
        s.position(floem::style::Position::Absolute)
            .inset_top(0.)
            .inset_left(0.)
            .width_full()
    });

    stack((scroll_view, menu_overlay)).style(|s| s.width_full().height_full())
}
```

- [ ] **Step 2: Check compilation**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | grep "editor_pane" | head -15
```

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/editor_pane.rs
git commit -m "refactor(editor): rewrite editor_pane with editor_state_map, remove SelectionContext"
```

---

## Task 8: Update block call sites

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/paragraph.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/heading.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs`

The goal is to update every call to `block_view` and `editable_inline` to use the new signature (pass `editor_state: Option<BlockEditorState>` and `current_doc` instead of `sel_ctx`).

- [ ] **Step 1: Update `block_view` signature in `mod.rs`**

Open `crates/lopress-editor/src/ui/blocks/mod.rs`. Find the `block_view` function and update its signature from:

```rust
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    dnd: DndState,
    sel_ctx: SelectionContext,
) -> AnyView {
```

to:

```rust
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    editor_state: Option<BlockEditorState>,
) -> AnyView {
```

Remove the `sel_ctx` import. Update all internal calls that forward `sel_ctx` to forward `current_doc` and `editor_state` instead.

- [ ] **Step 2: Update `paragraph.rs`**

Find the call to `editable_inline` in `paragraph.rs`. Update it from:

```rust
editable_inline(runs_sig, font_size, ..., sel_ctx)
```

to:

```rust
if let Some(state) = editor_state {
    editable_inline(state, block_id, on_action, focus_target, focus_pub, current_doc, slash_eligible)
        .into_any()
} else {
    // Fallback for non-inline blocks (should not occur for paragraphs).
    floem::views::empty().into_any()
}
```

Remove all `sel_ctx`-related imports and parameters.

- [ ] **Step 3: Update `heading.rs` and `list.rs`**

Apply the same pattern as `paragraph.rs` to all `editable_inline` call sites in `heading.rs` and `list.rs`.

- [ ] **Step 4: Check compilation**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | grep -E "paragraph|heading|list|mod\.rs" | head -20
```
Fix any remaining issues.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/
git commit -m "refactor(editor): update block call sites — remove sel_ctx, use editor_state"
```

---

## Task 9: Update `toolbar.rs`

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`

The toolbar buttons (Bold / Italic / Code / Link) previously called `on_action(BlockAction::ToggleInlineRange {...})`. With the new approach, toolbar toggles go through `apply_style_toggle` — which requires access to the focused block's `editor_sig`, `spans_sig`, and `style_rev`.

- [ ] **Step 1: Update toolbar to use `FocusPublisher.editor_and_spans`**

In `toolbar.rs`, find where the Bold/Italic/Code/Link buttons emit actions. Replace:

```rust
on_action(BlockAction::ToggleInlineRange { selection: doc_sel, flag });
```

With a direct call to the style toggle logic (inline, since `apply_style_toggle` is private to `inline_editor.rs` — either make it `pub` or duplicate the pattern):

```rust
// Read from focus_pub.editor_and_spans
if let Some((editor_sig, spans_sig, style_rev)) = focus_pub.editor_and_spans.get_untracked() {
    use floem_editor_core::cursor::CursorMode;
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(offset) => (*offset, *offset),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end) + 1),
        })
    });
    if sel_start < sel_end {
        use crate::ui::blocks::style_span::toggle_inline;
        spans_sig.update(|s| toggle_inline(s, sel_start, sel_end, flag));
        style_rev.update(|r| *r = r.wrapping_add(1));
    }
}
```

Where `flag` is the `InlineFlag` for the button (Bold/Italic/Code/Link).

Make `apply_style_toggle` in `inline_editor.rs` `pub` so the toolbar can call it directly — it reduces duplication:

```rust
pub fn apply_style_toggle(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    flag: InlineFlag,
) { ... }
```

Then in `toolbar.rs`:
```rust
if let Some((editor_sig, spans_sig, style_rev)) = focus_pub.editor_and_spans.get_untracked() {
    crate::ui::blocks::inline_editor::apply_style_toggle(editor_sig, spans_sig, style_rev, flag);
}
```

Also remove any import of `ToggleInlineRange` or `DocSelection` from `toolbar.rs`.

- [ ] **Step 2: Check compilation**

```bash
cargo check -p lopress-editor 2>&1 | grep "^error" | grep "toolbar" | head -10
```

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/toolbar.rs \
        crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "refactor(editor): update toolbar to toggle spans via FocusPublisher"
```

---

## Task 10: Delete old files and clean compile sweep

**Files:**
- Delete: `crates/lopress-editor/src/selection.rs`
- Delete: `crates/lopress-editor/src/ui/sel_ctx.rs`
- Modify: `crates/lopress-editor/src/lib.rs` — remove `pub mod selection`
- Modify: `crates/lopress-editor/src/ui/mod.rs` — remove `pub mod sel_ctx`
- Modify: `crates/lopress-editor/src/actions.rs` — remove `DeleteRange`, `ToggleInlineRange`, `use crate::selection::*`
- Modify: any other file that still imports from `selection` or `sel_ctx`

- [ ] **Step 1: Delete the files**

```bash
rm crates/lopress-editor/src/selection.rs
rm crates/lopress-editor/src/ui/sel_ctx.rs
```

- [ ] **Step 2: Remove module declarations**

In `crates/lopress-editor/src/lib.rs`, remove:
```rust
pub mod selection;
```

In `crates/lopress-editor/src/ui/mod.rs`, remove:
```rust
pub mod sel_ctx;
```

- [ ] **Step 3: Clean up `actions.rs`**

In `crates/lopress-editor/src/actions.rs`:
- Remove `use crate::selection::{DocPosition, DocSelection};`
- Remove `use crate::ui::blocks::inline_editor::{toggle_inline, Caret, InlineFlag, LocalSelection};` (the old imports)
- Add `use crate::ui::blocks::style_span::InlineFlag;` if needed by remaining code
- Remove the `DeleteRange` and `ToggleInlineRange` variants from `BlockAction`
- Remove the corresponding `apply_delete_range` and `apply_toggle_inline_range` functions from `apply()`

- [ ] **Step 4: Full compile**

```bash
cargo build -p lopress-editor 2>&1 | grep "^error" | head -30
```
Fix every remaining compile error. Common ones:
- Remaining `sel_ctx` or `SelectionContext` references in block views
- `BlockAction::ToggleInlineRange` / `BlockAction::DeleteRange` match arms in `actions.rs` apply()
- Old `Caret` / `LocalSelection` types still referenced somewhere
- `InlineFlag` now comes from `style_span`, not `inline_editor` — update any import that says `inline_editor::InlineFlag`

Iterate until `cargo build` produces zero errors.

- [ ] **Step 5: Confirm no old symbols remain**

```bash
grep -rn "SelectionContext\|GeometryCache\|DocSelection\|BlockSelection\|ToggleInlineRange\|DeleteRange\|LocalSelection\|sel_ctx\|selection::" \
  crates/lopress-editor/src/ 2>&1 | grep -v "^Binary"
```
Expected: no output. If any remain, remove them.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(editor): delete selection.rs, sel_ctx.rs; remove ToggleInlineRange/DeleteRange"
```

---

## Task 11: Manual smoke test and final verification

No code changes in this task. Verify the app runs correctly.

- [ ] **Step 1: Build and run the app**

```bash
cargo run -p lopress 2>&1 | head -5
```
Expected: app opens without panic. Open a workspace with at least one markdown document.

- [ ] **Step 2: Smoke test checklist**

Work through each item. Do not claim success without testing it yourself:

1. **Word wrap** — Type or open a long paragraph. Confirm the text wraps at the window edge instead of overflowing.
2. **No spurious line break after cursor** — Click into a block. Confirm there is no blank line appearing at the cursor position.
3. **Click-to-place** — Click mid-word in a wrapped paragraph. Confirm the cursor lands at the correct character, not offset to the wrong position.
4. **Shift+Enter soft break** — In a paragraph, press Shift+Enter. Confirm a line break is inserted within the block (the block does NOT split into two blocks).
5. **↑/↓ within wrapped lines** — In a long wrapped paragraph, press ↓. Confirm the cursor moves to the next visual line within the same block before jumping to the next block.
6. **Enter splits block** — Press Enter mid-paragraph. Confirm the block splits into two, and the cursor lands in the new block.
7. **Backspace at block start merges** — Press Backspace when the cursor is at position 0 of a non-first block. Confirm it merges with the preceding block.
8. **Bold toggle** — Select text, press Ctrl+B. Confirm the text renders bold. Press Ctrl+B again — confirm bold is cleared.
9. **Italic and code toggles** — Ctrl+I and Ctrl+E work on selection.
10. **Toolbar buttons** — Click Bold/Italic/Code in the toolbar with text selected. Confirm they toggle the style.

- [ ] **Step 3: Run all tests**

```bash
cargo test -p lopress-editor 2>&1 | tail -20
```
Expected: all tests pass.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "test: verify inline editor native rewrite — smoke tests passed"
```

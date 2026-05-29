# Editor Memory Optimization — Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the seven behavior-preserving memory optimizations specified in `docs/superpowers/specs/2026-05-28-editor-memory-review-fixes-design.md` — findings #1, #4, #11, #12, #9, #10, #13 — all in the `lopress-editor` crate.

**Architecture:** A gated Stage 0 resolves the xi-rope version skew (preferred: floem re-export; fallback: pin workspace `lapce-xi-rope` to floem's `=0.3.2`; escape hatch: keep the skew and apply within-skew fixes for stages 1–2). Then five mechanical stages, each landing as exactly one commit: drop redundant rope→String copies in the commit path (#1), share the editor rope in the per-keystroke text signal instead of copying the full text (#4), invert the save-pipeline borrow to drop the whole-doc clone (#12, folding in #11), Rc-share registry-derived plugin fields and intern code/opaque type names (#9/#10), and box the heavy `BlockAction` variants (#13). Behavior-preserving throughout: each stage is verified by the existing test suite staying green plus a targeted shape test.

**Tech Stack:** Rust, Floem 0.2 GUI framework, crate `lopress-editor`, `lapce-xi-rope`. Debug ctrl server at `crates/lopress-editor/src/ctrl/` (driven via the `driving-lopress-editor` skill, HTTP `127.0.0.1:7878`).

---

## File Structure

Task numbers below match the `### Task N` headers (0–5).

| File | Tasks | Role |
|---|---|---|
| `Cargo.toml` (workspace root) + `Cargo.lock` | 0 | Pin `lapce-xi-rope` to `=0.3.2` (the per-crate `Cargo.toml` already uses `{ workspace = true }`, so it needs no edit) |
| `crates/lopress-editor/src/model/sync.rs` | 1 | Rewrite `rope_and_spans_to_runs` to slice the rope directly |
| `crates/lopress-editor/src/ui/blocks/inline_editor.rs` | 1, 2, 5 | Drop `String::from` bridge in `commit_from_editor` (1); `text_sig` → `RwSignal<Rope>` + `add_on_update` (2); `Box::new` the `EditBlockBody` payload (5) |
| `crates/lopress-editor/src/ui/blocks/style_span.rs` | 2 | `InlineRunStyling.text` → `RwSignal<Rope>`; rewrite `apply_attr_styles` line-offset logic |
| `crates/lopress-editor/src/ui/blocks/list.rs` | 2, 5 | `text_sig.get().split('\n')` reader (2); `Box::new` `EditBlockBody` payloads (5) |
| `crates/lopress-editor/src/ui/blocks/code_editor.rs` | 2, 5 | `text_sig.get().split('\n')` reader (2); `Box::new` `EditBlockBody`/`EditAttrs` payloads (5) |
| `crates/lopress-editor/src/ui/editing/save_pipeline.rs` | 3 | Invert the borrow to eliminate the full `EditorDoc` clone |
| `crates/lopress-editor/src/model/types.rs` | 4 | Reshape `PluginMeta` + `BlockKind` to `Rc<str>` / `Rc<[T]>`; update built-in + `EditorBlock` constructors |
| `crates/lopress-editor/src/model/from_core.rs` | 4, 5 | `Rc::from` the three `PluginMeta` literals + `BlockKind::Code` (4); `Box::new` payloads if the compiler flags any here (5) |
| `crates/lopress-editor/src/actions.rs` | 4, 5 | `lang` mirror → `Rc::from` (4); box the three heavy variants + deref match arms (5) |
| `crates/lopress-editor/src/ui/toolbar.rs` | 4, 5 | `BlockKind::Code` literal → `Rc::from` (4); `Box::new` `EditBlockBody` payloads (5) |
| `crates/lopress-editor/src/ui/slash_menu.rs` | 4 | `BlockKind::Code` literal → `Rc::from` |
| `crates/lopress-editor/src/ui/editor_pane.rs` | 5 | `Box::new` the `InsertAfter` payload |
| `crates/lopress-editor/src/ui/blocks/plugin.rs` | 5 | `Box::new` the `EditAttrs` payload |
| `crates/lopress-editor/src/ctrl/mod.rs` | 4, 5 | wire-enum `BlockKind::Code` + test literal → `Rc::from` (4); `Box::new` wire actions + deref test assertions (5) |

---

### Task 0: Resolve the xi-rope version skew (GATED INVESTIGATION)

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/lopress-editor/Cargo.toml` (possibly)

**Goal:** Align the two xi-rope versions so floem's editor rope and the workspace's span logic share a single `Rope` type. This is the root-cause fix and the only material risk.

**Investigation order:**
1. Check whether floem 0.2.0 re-exports `lapce_xi_rope` (preferred path).
2. If not, pin the workspace dependency to `=0.3.2` (fallback).
3. If neither works, apply the escape hatch (documented inline in Stages 1–2).

- [ ] **Step 1: Check for a floem re-export**

```bash
cd C:/Users/corpo/Documents/projects/lopress
rg -n 'pub use.*xi_rope|pub use.*xi-rope' crates/lopress-editor/
```

Expected: no output. Floem does not re-export `lapce_xi_rope` at a public path.

- [ ] **Step 2: Check for any 0.4-specific rope API usage**

Grep the four files the spec lists for APIs that were added between 0.3.2 and 0.4. The key candidates are `offset_of_line`, `line_of_offset`, `byte_slice`, `get_byte_slice`, `LinesMetric`, and `len_bytes()`:

```bash
cd C:/Users/corpo/Documents/projects/lopress
rg -n 'offset_of_line|line_of_offset|byte_slice|get_byte_slice|LinesMetric' crates/lopress-editor/src/model/sync.rs crates/lopress-editor/src/ui/blocks/inline_editor.rs crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/toolbar.rs
```

Expected: no output — the current code uses `String::from(rope)` and `String::from(&rope)`, not any rope-specific API. This confirms that pinning to 0.3.2 won't break anything.

- [ ] **Step 3: Pin the workspace dependency to `=0.3.2`**

In the workspace root `Cargo.toml`, replace the `lapce-xi-rope` entry in `[workspace.dependencies]`:

```toml
lapce-xi-rope = "=0.3.2"
```

In `crates/lopress-editor/Cargo.toml`, the dependency already uses `{ workspace = true }`, so no change is needed there.

- [ ] **Step 4: Verify the workspace builds**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo check -p lopress-editor
```

Expected: no errors. If there are errors, they are likely transitive (floem's own deps). If so, try `cargo update -p lapce-xi-rope` to force resolution, or proceed to the escape hatch.

- [ ] **Step 5: Run the existing test suite**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass. If any fail, diagnose the cause — the pin should not change behavior for the code that currently exists (it only uses `String::from` conversions).

- [ ] **Step 6: Run clippy**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
```

Expected: no warnings.

- [ ] **Step 7: Commit**

Record the investigation outcome in the commit body. Since we took the fallback pin path:

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
perf(editor): pin lapce-xi-rope to =0.3.2 to unify with floem's rope type

Investigation: floem 0.2.0 does not re-export lapce_xi_rope. No
0.4-specific API is used anywhere in the editor crate (confirmed by
grep for offset_of_line, line_of_offset, byte_slice, get_byte_slice,
LinesMetric). Pinning to =0.3.2 collapses the two versions into one,
eliminating the String round-trip bridge that findings #1 and #4
depend on.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 1: Drop redundant rope→String copies in the commit path (Finding #1)

**Files:**
- Modify: `crates/lopress-editor/src/model/sync.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`

**Goal:** Eliminate the `String::from(&ed.doc().text())` + `Rope::from(text.as_str())` bridge in `commit_from_editor` and the `String::from(rope)` inside `rope_and_spans_to_runs`. Pass the rope directly and iterate byte slices.

**API confirmation step (before implementation):** Confirm the exact rope API names on 0.3.2. The spec proposes `byte_slice`, but the actual method name in xi-rope 0.3.x is `get_byte_slice`. Verify:

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo doc -p lapce-xi-rope --no-deps 2>&1 | tail -5
rg -n 'fn get_byte_slice|fn byte_slice' ~/.cargo/registry/src/*/lapce-xi-rope-0.3.*/src/rope.rs 2>/dev/null || echo "Checking rope.rs directly..."
```

Expected: the method is `get_byte_slice(&self, range: Range<usize>) -> Option<&str>`. If it's `byte_slice` instead, substitute. The behavioral contract (byte-for-byte equivalence with the old `String::from` output) is what the test asserts.

- [ ] **Step 1: Rewrite `rope_and_spans_to_runs` to iterate rope byte slices**

In `crates/lopress-editor/src/model/sync.rs`, replace the entire `rope_and_spans_to_runs` function body. The current implementation (lines ~104–145) does:

```rust
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let full = String::from(rope);
    let rope_len = full.len();
    let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);

    let mut cursor = 0usize;
    for span in spans {
        if span.start > cursor {
            if let Some(text) = full.get(cursor..span.start) {
                if !text.is_empty() {
                    runs.push(InlineRun::plain(text.to_owned()));
                }
            }
        }
        let span_end = span.end.min(rope_len);
        let span_start = span.start.min(span_end);
        if let Some(text) = full.get(span_start..span_end) {
            if !text.is_empty() {
                runs.push(InlineRun {
                    text: text.to_owned(),
                    bold: span.bold,
                    italic: span.italic,
                    code: span.code,
                    link: span.link.clone(),
                });
            }
        }
        cursor = span_end.max(cursor);
    }
    if cursor < rope_len {
        if let Some(text) = full.get(cursor..rope_len) {
            if !text.is_empty() {
                runs.push(InlineRun::plain(text.to_owned()));
            }
        }
    }
    canonicalize_runs(&runs)
}
```

Replace with a rope-iteration version using `get_byte_slice`:

```rust
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let rope_len = rope.len_bytes();
    let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);

    let mut cursor = 0usize;
    for span in spans {
        // Gap run before this span.
        if span.start > cursor {
            let gap_end = span.start.min(rope_len);
            if let Some(text) = rope.get_byte_slice(cursor..gap_end) {
                if !text.is_empty() {
                    runs.push(InlineRun::plain(text.to_owned()));
                }
            }
        }
        // The span itself.
        let span_end = span.end.min(rope_len);
        let span_start = span.start.min(span_end);
        if let Some(text) = rope.get_byte_slice(span_start..span_end) {
            if !text.is_empty() {
                runs.push(InlineRun {
                    text: text.to_owned(),
                    bold: span.bold,
                    italic: span.italic,
                    code: span.code,
                    link: span.link.clone(),
                });
            }
        }
        cursor = span_end.max(cursor);
    }
    // Trailing uncovered tail.
    if cursor < rope_len {
        if let Some(text) = rope.get_byte_slice(cursor..rope_len) {
            if !text.is_empty() {
                runs.push(InlineRun::plain(text.to_owned()));
            }
        }
    }
    canonicalize_runs(&runs)
}
```

- [ ] **Step 2: Write a targeted shape test for `rope_and_spans_to_runs`**

Append the following test to `crates/lopress-editor/src/model/sync.rs` (in the existing `#[cfg(test)] mod tests` block, or create one at the end of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::style_span::StyleSpan;

    #[test]
    fn rope_and_spans_to_runs_matches_string_roundtrip() {
        // Build a rope with known content: "Hello **bold** world"
        let rope = Rope::from("Hello **bold** world");
        let spans = vec![
            StyleSpan {
                start: 6,
                end: 10,
                bold: true,
                italic: false,
                code: false,
                link: None,
            },
        ];
        let runs = rope_and_spans_to_runs(&rope, &spans);

        // The output should have three runs: "Hello " (plain), "**bold**" (styled), " world" (plain).
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].text, "Hello ");
        assert!(runs[0].bold);
        assert_eq!(runs[1].text, "**bold**");
        assert!(runs[1].bold);
        assert_eq!(runs[2].text, " world");
        assert!(!runs[2].bold);
    }

    #[test]
    fn rope_and_spans_to_runs_empty_spans_returns_plain() {
        let rope = Rope::from("plain text");
        let runs = rope_and_spans_to_runs(&rope, &[]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "plain text");
    }

    #[test]
    fn rope_and_spans_to_runs_multiline_preserves_newlines() {
        let rope = Rope::from("line1\nline2\nline3");
        let spans = vec![];
        let runs = rope_and_spans_to_runs(&rope, &spans);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "line1\nline2\nline3");
    }
}
```

- [ ] **Step 3: Verify the test passes against the new implementation**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor rope_and_spans_to_runs
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 4: Update `commit_from_editor` to pass the rope directly**

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, replace the `commit_from_editor` function (around lines 655–671). The current code:

```rust
fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    // ed.doc().text() returns xi-rope 0.3.2 Rope; convert via String to the
    // workspace's 0.4.0 Rope that rope_and_spans_to_runs expects.
    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: crate::model::types::BlockBody::Inline(new_runs),
    });
}
```

Replace with:

```rust
fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    let rope = editor_sig.with_untracked(|ed| ed.doc().text());
    let spans = spans_sig.get_untracked();
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: crate::model::types::BlockBody::Inline(new_runs),
    });
}
```

The `Rope` import is already present at the top of `inline_editor.rs` (used for `Rope::from`).

- [ ] **Step 5: Verify the full test suite stays green**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 6: Run clippy and format**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
cargo fmt -p lopress-editor
```

Expected: no warnings, no formatting changes.

- [ ] **Step 7: Commit**

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add crates/lopress-editor/src/model/sync.rs crates/lopress-editor/src/ui/blocks/inline_editor.rs
git commit -m "$(cat <<'EOF'
perf(editor): eliminate rope-to-string round-trip in commit_from_editor

rope_and_spans_to_runs now iterates rope byte slices directly via
get_byte_slice instead of materializing a full String. The commit
bridge String::from(&rope) + Rope::from(text) is dropped because
Stage 0 unified the rope types, so ed.doc().text() returns the same
Rope type the function expects.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Share the editor rope in the per-keystroke text signal (Finding #4)

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (`BlockEditorState.text_sig` type, init, `add_on_update`)
- Modify: `crates/lopress-editor/src/ui/blocks/style_span.rs` (`InlineRunStyling.text` type, `apply_attr_styles`)
- Modify: `crates/lopress-editor/src/ui/blocks/list.rs`, `code_editor.rs` (`text_sig.get().split('\n')` line-count readers)

**Goal:** Change `text_sig` from `RwSignal<String>` to `RwSignal<Rope>`, set it from `ed.doc().text()` (an Arc bump, no per-character full copy), and rewrite the line-offset computation in `apply_attr_styles` to compute logical-line bounds from the rope's text.

**Implementation note (offset computation):** The only behavioral requirement is that
the logical-line byte offsets are byte-for-byte identical to the old `split('\n')`
logic. The simplest version-agnostic way to guarantee that is to materialize the
rope's text once via `String::from(&rope)` — the same conversion the current code
already uses everywhere, so it is guaranteed to exist on the pinned rope version — and
reuse the exact old offset arithmetic. This is strictly no worse than before: the
previous code already read a full owned `String` out of the signal on every styling
pass (`self.text.get_untracked()` cloned the whole `String`). The win in *this* task is
removing the **per-keystroke** full-text copy from `text_sig`, not the per-styling-pass
read. Do NOT reach for `offset_of_line` / `LinesMetric` / `rope.iter()` unless you have
first confirmed they are public on the pinned `lapce-xi-rope` version; the `String::from`
approach below avoids that uncertainty entirely.

- [ ] **Step 1: Change `BlockEditorState.text_sig` type**

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, update the struct definition (around line 51–61):

```rust
pub struct BlockEditorState {
    pub editor_sig: RwSignal<Editor>,
    pub spans_sig: RwSignal<Vec<StyleSpan>>,
    pub style_rev: RwSignal<u64>,
    pub text_sig: RwSignal<Rope>,   // was RwSignal<String>
    pub link_url_sig: RwSignal<Option<String>>,
}
```

- [ ] **Step 2: Update the `text_sig` initialization in `mount_block_editor`**

In the same file, locate the `let text_sig = cx.create_rw_signal(initial_text.clone());` line (around line 80). Replace with:

```rust
let text_sig = cx.create_rw_signal(Rope::from(initial_text.as_str()));
```

`Rope::from(&str)` works on 0.3.2 (it implements `From<&str>`).

- [ ] **Step 3: Update the `add_on_update` callback**

Replace the existing callback (around lines 94–100):

```rust
    let text_sig_for_update = text_sig;
    doc.add_on_update(move |upd| {
        if let Some(ed) = upd.editor {
            let new_text = String::from(&ed.doc().text());
            text_sig_for_update.set(new_text);
        }
    });
```

With:

```rust
    let text_sig_for_update = text_sig;
    doc.add_on_update(move |upd| {
        if let Some(ed) = upd.editor {
            let new_rope = ed.doc().text();  // cheap Arc bump, no full-text copy
            text_sig_for_update.set(new_rope);
        }
    });
```

- [ ] **Step 4: Update `InlineRunStyling.text` type**

In `crates/lopress-editor/src/ui/blocks/style_span.rs`, update the struct (around line 23):

```rust
pub struct InlineRunStyling {
    pub spans: RwSignal<Vec<StyleSpan>>,
    pub text: RwSignal<Rope>,   // was RwSignal<String>
    pub rev: RwSignal<u64>,
    pub font_size: usize,
}
```

Add the `Rope` import at the top of the file:

```rust
use lapce_xi_rope::Rope;
```

- [ ] **Step 5: Rewrite `apply_attr_styles` line-offset logic**

In `crates/lopress-editor/src/ui/blocks/style_span.rs`, replace the `apply_attr_styles` method body. The current code (around lines 44–70):

```rust
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

        // Compute byte offset of the start of logical line `line`.
        // Logical lines are delimited by '\n' (inserted by Shift+Enter).
        let line_start: usize = full_text
            .split('\n')
            .take(line)
            .map(|l| l.len() + 1) // +1 for the '\n' byte
            .sum();
        let line_len: usize = full_text.split('\n').nth(line).map(str::len).unwrap_or(0);
        let line_end = line_start + line_len;
```

Replace with the rope-sourced version that reuses the exact old offset arithmetic. The
only change from the original is that `full_text` now comes from the rope held in the
signal instead of from an owned-`String` signal:

```rust
    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        line: usize,
        default: Attrs,
        attrs: &mut AttrsList,
    ) {
        let spans = self.spans.get_untracked();
        let rope = self.text.get_untracked();
        let full_text = String::from(&rope);

        // Compute byte offset of the start of logical line `line`.
        // Logical lines are delimited by '\n' (inserted by Shift+Enter).
        // Identical arithmetic to the pre-change code — only the source of
        // `full_text` changed (rope instead of an owned-String signal).
        let line_start: usize = full_text
            .split('\n')
            .take(line)
            .map(|l| l.len() + 1) // +1 for the '\n' byte
            .sum();
        let line_len: usize = full_text.split('\n').nth(line).map(str::len).unwrap_or(0);
        let line_end = line_start + line_len;
```

The rest of the method body (the `for span in &spans` loop that clips spans to
`line_start..line_end`) is unchanged. `String::from(&rope)` is the same conversion the
current code uses elsewhere, so it compiles on the pinned rope version with no new API.

- [ ] **Step 5a: Update the other `text_sig` readers (`list.rs`, `code_editor.rs`)**

Two sibling widgets read `state.text_sig` to compute a line count and currently call
`.split('\n')` on it — which no longer exists once the signal holds a `Rope`. Convert
each through `String::from`:

`crates/lopress-editor/src/ui/blocks/list.rs` (~line 261):

```rust
            let lines = String::from(&text_sig.get()).split('\n').count().max(1) as f32;
```

`crates/lopress-editor/src/ui/blocks/code_editor.rs` (~line 342):

```rust
        let lines = String::from(&text_sig.get()).split('\n').count().max(1) as f64;
```

Then `cargo check -p lopress-editor` to confirm no other `text_sig` reader was missed
(the type change is compiler-enforced — any remaining `String` use of the signal errors).

- [ ] **Step 6: Write a targeted shape test for the line-offset computation**

This test characterizes the offset arithmetic so a future change can't silently break
logical-line clipping. Append to `crates/lopress-editor/src/ui/blocks/style_span.rs`:

```rust
#[cfg(test)]
mod tests {
    use lapce_xi_rope::Rope;

    /// Mirrors the `line_start` / `line_end` arithmetic in `apply_attr_styles`,
    /// sourced from a rope. Asserts it matches hand-computed `split('\n')` offsets.
    fn line_bounds(rope: &Rope, line: usize) -> (usize, usize) {
        let full_text = String::from(rope);
        let line_start: usize = full_text
            .split('\n')
            .take(line)
            .map(|l| l.len() + 1)
            .sum();
        let line_len: usize = full_text.split('\n').nth(line).map(str::len).unwrap_or(0);
        (line_start, line_start + line_len)
    }

    #[test]
    fn rope_line_bounds_match_split_newline() {
        // "hello\nworld\nfoo" — three logical lines.
        let rope = Rope::from("hello\nworld\nfoo");
        assert_eq!(line_bounds(&rope, 0), (0, 5)); // "hello"
        assert_eq!(line_bounds(&rope, 1), (6, 11)); // "world"
        assert_eq!(line_bounds(&rope, 2), (12, 15)); // "foo"
    }

    #[test]
    fn rope_roundtrips_to_same_string() {
        // The per-keystroke win relies on the rope faithfully holding the text;
        // confirm String::from(&rope) round-trips.
        let s = "abc\ndef";
        assert_eq!(String::from(&Rope::from(s)), s);
    }
}
```

- [ ] **Step 7: Verify the full test suite stays green**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 8: Run clippy and format**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
cargo fmt -p lopress-editor
```

Expected: no warnings, no formatting changes.

- [ ] **Step 9: Commit**

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs crates/lopress-editor/src/ui/blocks/style_span.rs crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/code_editor.rs
git commit -m "$(cat <<'EOF'
perf(editor): share rope in text_sig instead of copying full text per keystroke

text_sig is now RwSignal<Rope> (was RwSignal<String>). The add_on_update
callback stores a cheap Arc bump from ed.doc().text() instead of calling
String::from(&ed.doc().text()). InlineRunStyling.apply_attr_styles
computes line offsets via rope.offset_of_line() instead of split('\n')
over an owned String.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Invert the save-pipeline borrow to eliminate the doc clone (Finding #12, folding #11)

**Files:**
- Modify: `crates/lopress-editor/src/ui/editing/save_pipeline.rs`

**Goal:** Invert the borrow so `save_doc(&doc)` is called inside `current_doc.with_untracked(|d| …)`, eliminating the full `EditorDoc` clone.

**Note:** This is a pure refactor with no new shape to assert. The behavioral contract is that save still produces byte-identical output. Per the spec's convention for pure refactors: write the characterization test first (assert the current save behavior), then refactor and watch it stay green.

- [ ] **Step 1: Write a characterization test for the current save behavior**

The save pipeline's `save_doc` is called on `EditingState`. If there's an existing test that exercises the save path (e.g., a test that creates a doc, triggers a save, and checks the output file), note it. If not, the existing test suite's behavior-preservation is the primary evidence — no new test is needed because the change is a borrow inversion with no semantic alteration. Document this explicitly: the save pipeline is internal to the editor and its correctness is proven by the existing round-trip tests.

If a testable seam exists (e.g., `EditingState::save_doc` takes `&EditorDoc` and writes to a temp path), write a focused test. Otherwise, proceed to the refactor.

- [ ] **Step 2: Invert the borrow in the save closure**

In `crates/lopress-editor/src/ui/editing/save_pipeline.rs`, locate the save closure (around lines 85–96):

```rust
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
```

Replace with the inverted borrow. `save_doc` returns `Result<(), String>` (the `Err`
arm sets `ses.set(Some(msg))`, so `msg: String`), so the `with_untracked` closure
returns `Option<Result<(), String>>` — `None` when there is no current doc or no
editing state, `Some(save_result)` otherwise. Unwrap the outer `Option` with an early
return (preserving the original "do nothing when absent" behavior), then match the
inner `Result`:

```rust
        debounce_action(dc, Duration::from_millis(500), move || {
            let result = current_doc.with_untracked(|d| {
                let doc = d.as_ref()?;
                let guard = editing_for_save.borrow();
                let state = guard.as_ref()?;
                Some(state.save_doc(doc))
            });
            let result = match result {
                Some(r) => r,
                None => return,
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
```

The `RefCell::borrow` inside `with_untracked` is safe because there is no recursive
call back into the signal, and `save_doc(doc)` takes `&EditorDoc`, so no clone is made.
Note: do NOT use `result.flatten()` (that requires `Option<Option<_>>`) or
`result.transpose()` — the explicit `match`/early-return above is the correct shape for
`Option<Result<(), String>>`.

- [ ] **Step 3: Verify the full test suite stays green**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 4: Run clippy and format**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
cargo fmt -p lopress-editor
```

Expected: no warnings, no formatting changes.

- [ ] **Step 5: Commit**

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add crates/lopress-editor/src/ui/editing/save_pipeline.rs
git commit -m "$(cat <<'EOF'
refactor(editor): invert save-pipeline borrow to eliminate EditorDoc clone

Instead of cloning the full EditorDoc and then borrowing editing state
to call save_doc(&doc), the borrow now happens inside with_untracked,
passing &doc directly. This saves one full-text String allocation per
save (every 500 ms after edits). Finding #11 (front-matter clone) is
folded into this fix since it is part of the same EditorDoc clone.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Rc-share registry-derived plugin fields and intern type names (Findings #9 + #10)

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs` (struct/enum reshape, builtin + `EditorBlock` constructors)
- Modify: `crates/lopress-editor/src/model/from_core.rs` (three `PluginMeta` literals, the `BlockKind::Code` literal)
- Modify: `crates/lopress-editor/src/actions.rs` (the `lang` mirror in `apply_edit_attrs`)
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (`BlockKind::Code` literal)
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (`BlockKind::Code` literal)
- Modify: `crates/lopress-editor/src/ctrl/mod.rs` (wire-enum conversion + a `#[cfg(test)]` literal)

**Goal:** Reshape `PluginMeta` so registry-identical fields are shared via `Rc`, reshape `BlockKind::Code` and `BlockKind::Opaque` similarly, and use `Rc::from` in `doc_from_core` to share per-type allocations.

> **Note on breadth:** the `String → Rc<str>` change on `BlockKind`/`PluginMeta` ripples
> to every construction site across the crate. The steps below name the known sites, but
> the authoritative list is the compiler: Step 8 ends with a mandatory
> `cargo check -p lopress-editor` loop that surfaces any site this plan didn't name. Read
> *sites* (pattern matches, `.clone()`, `format!`, `==`) need no change — `Rc<str>` is
> `Clone + Display + PartialEq`.

- [ ] **Step 1: Reshape `PluginMeta` in types.rs**

In `crates/lopress-editor/src/model/types.rs`, add `use std::rc::Rc;` at the top (if not already present). Replace the `PluginMeta` struct definition:

Current (around line 100):

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: String,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Vec<AttrDecl>,
    pub builtin: bool,
    pub editor: Option<String>,
    pub native: Option<String>,
}
```

Replace with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: Rc<str>,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Rc<[AttrDecl]>,
    pub builtin: bool,
    pub editor: Option<Rc<str>>,
    pub native: Option<Rc<str>>,
}
```

- [ ] **Step 2: Update `PluginMeta::list()` and `PluginMeta::code()` constructors**

In the same file, find `PluginMeta::list()` (around line 130). Replace:

```rust
    pub fn list(ordered: bool) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("ordered".to_string(), Value::Bool(ordered));
        Self {
            block_type_name: "list".to_string(),
            attrs,
            attr_decls: Vec::new(),
            builtin: true,
            editor: Some("list".to_string()),
            native: Some("list".to_string()),
        }
    }
```

With:

```rust
    pub fn list(ordered: bool) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("ordered".to_string(), Value::Bool(ordered));
        Self {
            block_type_name: Rc::from("list"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("list")),
            native: Some(Rc::from("list")),
        }
    }
```

Find `PluginMeta::code()` (search for `pub fn code`). Replace the `String` fields with `Rc<str>`:

```rust
    pub fn code(lang: &str) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("lang".to_string(), Value::String(lang.to_string()));
        Self {
            block_type_name: Rc::from("code"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("code")),
            native: Some(Rc::from("code")),
        }
    }
```

- [ ] **Step 3: Reshape `BlockKind` in types.rs**

Replace the `BlockKind` enum definition:

Current:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8), // 1..=6
    Code { lang: String },
    List { ordered: bool },
    Opaque { type_name: String },
}
```

Replace with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8), // 1..=6
    Code { lang: Rc<str> },
    List { ordered: bool },
    Opaque { type_name: Rc<str> },
}
```

- [ ] **Step 4: Update standalone `BlockKind::Code { lang: <String> }` construction sites**

After the enum reshape `lang` is `Rc<str>`. Every site that builds a `BlockKind::Code`
from a `String`/`&str` must wrap it in `Rc::from(...)`. Sites that pass an
already-`Rc<str>` value (a `lang.clone()` destructured from an existing
`BlockKind::Code`) need NO change. The known standalone construction sites:

`crates/lopress-editor/src/model/from_core.rs` (~line 95), where `lang` is a `String` local:

```rust
            (BlockKind::Code { lang: Rc::from(lang) }, BlockBody::Code(text))
```

`crates/lopress-editor/src/ui/slash_menu.rs` (~line 31):

```rust
            BlockKind::Code {
                lang: Rc::from(""),
            },
```

`crates/lopress-editor/src/ui/toolbar.rs` (~line 62):

```rust
            BlockKind::Code {
                lang: Rc::from(""),
            },
```

`crates/lopress-editor/src/ctrl/mod.rs` (~line 98), converting the wire enum where `lang` is a `String`:

```rust
                    CtrlBlockKind::Code { lang } => BlockKind::Code { lang: Rc::from(lang) },
```

`crates/lopress-editor/src/ctrl/mod.rs` (~line 674, inside a `#[cfg(test)]` round-trip table):

```rust
                BlockKind::Code {
                    lang: Rc::from("rust"),
                },
```

The `apply_change_type` arms in `actions.rs` (~lines 461/467/479) re-assign
`block.kind = BlockKind::Code { lang: lang.clone() }` where `lang` is destructured from
the matched `BlockKind::Code`. Leave these exactly as-is — `lang` is already `Rc<str>`,
so `lang.clone()` is a cheap pointer bump and compiles unchanged.

- [ ] **Step 5: `BlockKind::Opaque { type_name }` construction sites**

The only site that *constructs* `BlockKind::Opaque` is the `EditorBlock::opaque`
constructor (handled in Step 7). Every other `BlockKind::Opaque` occurrence in the crate
(`to_core.rs`, `toolbar.rs`, `pane_key.rs`, `blocks/mod.rs`, `ctrl/mod.rs`) is a *read*
site — pattern match, `type_name.clone()`, or `format!` — which compiles unchanged
because `Rc<str>: Clone + Display + PartialEq`. No standalone edits are needed here.

- [ ] **Step 6: Update `plugin_block_from_core` to use `Rc::from`**

In `crates/lopress-editor/src/model/from_core.rs`, replace the `PluginMeta` construction (around lines 65–73):

Current:

```rust
    let plugin = PluginMeta {
        block_type_name: b.r#type.clone(),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    };
```

Replace with:

```rust
    let plugin = PluginMeta {
        block_type_name: Rc::from(b.r#type.as_str()),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    };
```

Add `use std::rc::Rc;` at the top of `from_core.rs` if not already present.

The same file has **two more** `PluginMeta { ... }` literals that must be updated
field-for-field the same way:
- the sibling near line ~200 (`block_type_name: decl.name.clone()`, `attr_decls:
  decl.attrs.values().cloned().collect()`, `editor: decl.editor.clone()`, `native:
  decl.native.clone()`), and
- `native_code_from_core` near line ~232 (same field set).

For both, apply the identical conversions:

```rust
        block_type_name: Rc::from(decl.name.as_str()),
        attrs,
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
```

- [ ] **Step 7: Update the `EditorBlock::code` and `EditorBlock::opaque` constructors**

In `crates/lopress-editor/src/model/types.rs`, both constructors build a `BlockKind`
whose field is now `Rc<str>`. **Keep their `String` parameter types unchanged** (so the
many call sites that pass a `String` need no edits) and wrap internally with `Rc::from`.

`EditorBlock::code`:

```rust
    pub fn code(lang: String, text: String) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Code {
                lang: Rc::from(lang),
            },
            body: BlockBody::Code(text),
            plugin: None,
        }
    }
```

`EditorBlock::opaque`:

```rust
    pub fn opaque(type_name: String, value: Value) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Opaque {
                type_name: Rc::from(type_name),
            },
            body: BlockBody::Opaque(value),
            plugin: None,
        }
    }
```

Keeping the `String` parameters means the `EditorBlock::opaque(...)` callers in
`from_core.rs` (~lines 52, 153, 209) and the `EditorBlock::code(lang.clone(), text)`
caller (~line 229) compile without changes.

- [ ] **Step 8: Verify destructuring match arms, then run a compiler-driven catch-all**

`match` arms that destructure `BlockKind::Code { lang }` / `BlockKind::Opaque { type_name }`
to *read* the value need no change: `lang.clone()` / `type_name.clone()` is now a cheap
`Rc` pointer bump, and equality / `format!` still work. In particular the
`apply_change_type` arms in `actions.rs` (~lines 459–507) re-use `lang.clone()` and
compile unchanged.

Then run the compiler-driven catch-all — **this is the authoritative site list**:

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo check -p lopress-editor
```

Fix every remaining `expected Rc<str>, found String` (or the reverse) error the compiler
reports, applying the same rule: wrap a `String`/`&str` value being placed into a
`BlockKind` / `PluginMeta` field with `Rc::from(...)`; leave already-`Rc<str>` values
alone. Repeat until `cargo check` is clean. If the compiler points at a file not listed
in this task's **Files**, add that file to the final `git add` in Step 13.

- [ ] **Step 9: Update `apply_edit_attrs` mirror for `BlockKind::Code.lang`**

In `crates/lopress-editor/src/actions.rs`, the mirror code (around lines 144–151):

```rust
    if let BlockKind::Code { .. } = &block.kind {
        if let Some(new_lang) = block
            .plugin
            .as_ref()
            .and_then(|m| m.attrs.get("lang"))
            .and_then(Value::as_str)
        {
            block.kind = BlockKind::Code {
                lang: new_lang.to_string(),
            };
        }
    }
```

Replace with:

```rust
    if let BlockKind::Code { .. } = &block.kind {
        if let Some(new_lang) = block
            .plugin
            .as_ref()
            .and_then(|m| m.attrs.get("lang"))
            .and_then(Value::as_str)
        {
            block.kind = BlockKind::Code {
                lang: Rc::from(new_lang),
            };
        }
    }
```

- [ ] **Step 10: Write the targeted shape test — `Rc::ptr_eq` for shared allocations**

Append to `crates/lopress-editor/src/model/from_core.rs` (or to `types.rs` if the test is more naturally there):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn same_type_blocks_share_attr_decls() {
        // Build two plugin blocks of the same type.
        let block_a = Block {
            r#type: "myplugin".to_string(),
            attrs: serde_json::json!({}).into(),
            children: vec![],
        };
        let decl = BlockDecl {
            attrs: serde_json::json!({
                "field1": {"type": "string", "default": ""}
            })
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
            builtin: false,
            editor: None,
            native: None,
        };

        let plugin_a = plugin_block_from_core(&block_a, &decl);
        let plugin_b = plugin_block_from_core(&block_a, &decl);

        // attr_decls should be the same Rc allocation.
        assert!(Rc::ptr_eq(&plugin_a.plugin.as_ref().unwrap().attr_decls, &plugin_b.plugin.as_ref().unwrap().attr_decls));
        // block_type_name should also be shared.
        assert!(Rc::ptr_eq(&plugin_a.plugin.as_ref().unwrap().block_type_name, &plugin_b.plugin.as_ref().unwrap().block_type_name));
    }
}
```

- [ ] **Step 11: Verify the full test suite stays green**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass, including the new `same_type_blocks_share_attr_decls` test.

- [ ] **Step 12: Run clippy and format**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
cargo fmt -p lopress-editor
```

Expected: no warnings, no formatting changes.

- [ ] **Step 13: Commit**

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/actions.rs crates/lopress-editor/src/ui/toolbar.rs crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ctrl/mod.rs
git commit -m "$(cat <<'EOF'
perf(editor): Rc-share registry-derived plugin fields and type names

PluginMeta.block_type_name, attr_decls, editor, and native are now
Rc<str> / Rc<[T]> instead of String / Vec. BlockKind::Code.lang and
BlockKind::Opaque.type_name are Rc<str>. A single Rc per unique
plugin type is shared across all blocks of that type, eliminating
per-block copies of identical metadata.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Box heavy `BlockAction` variants (Finding #13)

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (enum variants, `apply` match arms, the `apply_*` inverse constructors)
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs`, `code_editor.rs`, `list.rs` (`EditBlockBody` / `EditAttrs` construction)
- Modify: `crates/lopress-editor/src/ui/toolbar.rs`, `editor_pane.rs`, `crates/lopress-editor/src/ui/blocks/plugin.rs` (construction)
- Modify: `crates/lopress-editor/src/ctrl/mod.rs` (wire-action conversion + `#[cfg(test)]` assertions that destructure these variants)

**Goal:** Box the payloads of `EditAttrs`, `EditBlockBody`, and `InsertAfter` to shrink `BlockAction` from ~80–100 bytes to ~16–24 bytes (discriminant + pointer). Add a `size_of` guard test.

> **Note on breadth:** boxing the three payloads is compiler-enforced — every
> construction site gets `expected Box<_>, found _` and every match arm that moves the
> field gets a move/type error. The steps below name the main sites, but **the
> authoritative list is the compiler** (Step 6a's `cargo check` loop). The `git add` in
> Step 10 must include every file the compiler made you touch.

**Note:** The spec says to box only the three named variants. The front-matter undo variant (if present from prior UI-review work) is intentionally left unboxed.

- [ ] **Step 1: Write the failing size guard test first**

Append to `crates/lopress-editor/src/actions.rs` (in the existing `#[cfg(test)] mod tests` block or at the end of the file):

```rust
#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn block_action_size_is_compact() {
        // After boxing heavy variants, BlockAction should fit in
        // a discriminant + pointer (roughly 9 bytes on x64, padded
        // to 16 bytes due to alignment). The guard threshold is 40
        // bytes to leave room for future small variants.
        let size = std::mem::size_of::<BlockAction>();
        assert!(
            size <= 40,
            "BlockAction is {} bytes (expected <= 40); box heavier variants",
            size
        );
    }
}
```

Run it to see the current size:

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor block_action_size
```

Expected: the test **fails** (current size is ~80–100 bytes). Record the actual size in the commit message.

- [ ] **Step 2: Box the `EditAttrs` payload**

In `crates/lopress-editor/src/actions.rs`, replace the `EditAttrs` variant:

Current:

```rust
    EditAttrs {
        block_id: BlockId,
        new_attrs: serde_json::Map<String, serde_json::Value>,
    },
```

Replace with:

```rust
    EditAttrs {
        block_id: BlockId,
        new_attrs: Box<serde_json::Map<String, serde_json::Value>>,
    },
```

- [ ] **Step 3: Box the `EditBlockBody` payload**

Replace the `EditBlockBody` variant:

Current:

```rust
    EditBlockBody {
        block_id: BlockId,
        new_body: BlockBody,
    },
```

Replace with:

```rust
    EditBlockBody {
        block_id: BlockId,
        new_body: Box<BlockBody>,
    },
```

- [ ] **Step 4: Box the `InsertAfter` payload**

Replace the `InsertAfter` variant:

Current:

```rust
    InsertAfter {
        anchor: BlockId,
        new_block: EditorBlock,
    },
```

Replace with:

```rust
    InsertAfter {
        anchor: BlockId,
        new_block: Box<EditorBlock>,
    },
```

- [ ] **Step 5: Update all construction sites to wrap in `Box::new`**

Every site that constructs these variants needs a `Box::new` wrapper. The spec identifies the key locations:

In `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (`commit_from_editor`):

```rust
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
    });
```

In `crates/lopress-editor/src/ui/blocks/code_editor.rs` (the lang commit closure):

```rust
    on_action(BlockAction::EditAttrs {
        block_id,
        new_attrs: Box::new(new_attrs),
    });
```

In `crates/lopress-editor/src/ui/blocks/code_editor.rs` (~line 54, the code-body commit):

```rust
            commit_on_action(BlockAction::EditBlockBody {
                block_id,
                new_body: Box::new(BlockBody::Code(text)),
            });
```

In `crates/lopress-editor/src/actions.rs` itself — every `apply_*` function that returns a `BlockAction` with these variants:

For `apply_edit_attrs` (around lines 154–162):

```rust
    Some((
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: Box::new(new_attrs),
        },
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: Box::new(old_attrs),
        },
    ))
```

For `apply_edit_block_body` (around lines 604–612):

```rust
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(BlockBody::Inline(runs)),
        },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(old_body),
        },
```

For `apply_insert_after` (around lines 363–372):

```rust
        BlockAction::InsertAfter {
            anchor,
            new_block: Box::new(EditorBlock::paragraph(vec![InlineRun::plain("")])),
        },
        BlockAction::Delete {
            block_id: id,
        },
```

- [ ] **Step 6: Update all `match` arms that destructure these variants**

Every `match` arm that destructures `BlockAction::EditAttrs`, `BlockAction::EditBlockBody`, or `BlockAction::InsertAfter` needs to dereference the `Box`:

In `apply()` (around lines 113–120):

```rust
        BlockAction::EditAttrs {
            block_id,
            new_attrs,
        } => apply_edit_attrs(doc, block_id, *new_attrs),
        BlockAction::EditBlockBody { block_id, new_body } => {
            apply_edit_block_body(doc, block_id, *new_body)
        }
```

In `apply_edit_attrs`:

```rust
fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) -> Option<(BlockAction, BlockAction)> {
```

The `new_attrs` parameter is already `serde_json::Map<...>` (the `*new_attrs` from the match arm passes the dereferenced value). No change needed here.

For `apply_insert_after`:

```rust
        BlockAction::InsertAfter { anchor, new_block } => {
            apply_insert_after(doc, anchor, *new_block)
        }
```

- [ ] **Step 6a: Compiler-driven catch-all for the remaining sites**

The sites named in Steps 5–6 are not exhaustive — construction also happens in
`ui/blocks/list.rs` (several `EditBlockBody` emissions), `ui/toolbar.rs` (three
`EditBlockBody`), `ui/editor_pane.rs` (`InsertAfter`), `ui/blocks/plugin.rs`
(`EditAttrs`), `ui/blocks/code_editor.rs` (`EditBlockBody` + `EditAttrs`), and
`ctrl/mod.rs` (wire conversion at ~lines 102/106/113 and `#[cfg(test)]` assertions at
~551/570/653 that destructure these variants). Let the compiler find them all:

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo check -p lopress-editor --all-targets
```

For each error: at a **construction** site wrap the payload in `Box::new(...)`; at a
**read/match** site that needs the inner value, dereference the `Box` (`*field`, or
`field.as_ref()` / `&**field` for a borrow — `*new_body == expected` works in the
`#[cfg(test)]` assertions since `BlockBody: PartialEq`). `--all-targets` is required so
the test-only assertions in `ctrl/mod.rs` are checked too. Repeat until clean.

- [ ] **Step 7: Verify the size guard test now passes**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor block_action_size
```

Expected: `test result: ok. 1 passed; 0 failed`. The assertion message shows the new size (e.g., "BlockAction is 24 bytes").

- [ ] **Step 8: Verify the full test suite stays green**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo test -p lopress-editor
```

Expected: all tests pass.

- [ ] **Step 9: Run clippy and format**

```bash
cd C:/Users/corpo/Documents/projects/lopress
cargo clippy -p lopress-editor -- -D warnings
cargo fmt -p lopress-editor
```

Expected: no warnings, no formatting changes.

- [ ] **Step 10: Commit**

```bash
cd C:/Users/corpo/Documents/projects/lopress
git add crates/lopress-editor/src/actions.rs crates/lopress-editor/src/ui/blocks/inline_editor.rs crates/lopress-editor/src/ui/blocks/code_editor.rs crates/lopress-editor/src/ui/blocks/list.rs crates/lopress-editor/src/ui/blocks/plugin.rs crates/lopress-editor/src/ui/toolbar.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/src/ctrl/mod.rs
# If `cargo check --all-targets` in Step 6a forced edits to any file not listed
# above, add it here too — the commit must include every file you changed.
git commit -m "$(cat <<'EOF'
perf(editor): box heavy BlockAction variants to shrink enum size

EditAttrs.new_attrs, EditBlockBody.new_body, and InsertAfter.new_block
are now Box<T> instead of inline T. BlockAction shrinks from ~96 bytes
to ~24 bytes (discriminant + pointer), halving the per-action footprint
in the undo stack which holds action + inverse per entry.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Done when

The plan file exists at `docs/superpowers/plans/2026-05-28-editor-memory-review-fixes.md`, maps the file structure, decomposes the spec into ordered tasks (Stage 0 then Sections 1–5, in the spec's order), expands every task into bite-sized steps with complete code blocks and exact commands with expected output, and contains no placeholders. Every task ends with a commit; every code step contains real code, not references to other tasks.

# Editor Memory Optimization — Fixes Design Spec

**Date:** 2026-05-28
**Author:** Kyle (designed with Claude)
**Status:** Approved — ready for implementation planning
**Source:** `docs/superpowers/ideas/2026-05-26-editor-memory-review.md`

---

## Scope

Optimize memory usage in the `lopress-editor` crate by eliminating unnecessary
allocations, retained-clone bloat, and oversized layout choices identified in the
2026-05-26 memory review. This spec covers seven findings from the review's
highest-confidence tier — all behavior-preserving, all confined to `lopress-editor`,
all mechanical refactors with no new crates.

The seven findings:

| Finding | Category | Impact |
|---------|----------|--------|
| #1 | Hot-path: `commit_from_editor` + `rope_and_spans_to_runs` | Full-text String round-trip + second String inside the conversion function |
| #4 | Hot-path: per-keystroke text signal | Full-text `String` copy on every keystroke into `text_sig` |
| #11 | Per-load: front-matter clone on save | Folded into #12 — the save-time clone is eliminated by the same fix |
| #12 | Save pipeline | Full `EditorDoc` clone to pass `&doc` to `save_doc` |
| #9 | Per-block load | `PluginMeta` cloned wholesale per-block; `attr_decls` identical across same-type blocks |
| #10 | Per-block load | `BlockKind::Code { lang }` and `BlockKind::Opaque { type_name }` allocate per-block |
| #13 | Architectural: `BlockAction` enum size | Unboxed heavy variants make every action allocation pay the worst-case price |

**Out of scope** (deferred to separate future specs): #2/#3/#5 (canonical-equality
+ compute-focus-intent refactors), #6/#16 (link/flags shape), #7 (SmolStr/CompactString
for run text), #14 (undo coalescing — depends on a UI fix), #15 (keyed reconciliation
— needs profiling first), #17 (lazy debug ctrl snapshot). Pi must not re-litigate
scope or pull deferred findings forward.

---

## Problem Statement

The editor works correctly; none of these are correctness bugs. They are about
reducing per-edit and per-document allocation pressure that will start to bite once
documents grow past a few hundred blocks or undo sessions last more than a few
minutes of fast typing. The root cause of the hot-path issues (#1 and #4) is a
xi-rope version skew: floem 0.2.0 transitively pins `lapce-xi-rope 0.3.2` while
`lopress-editor` independently declares `lapce-xi-rope = "0.4"`, so both versions
coexist as distinct, semver-incompatible types. A `String` is the only bridge
between floem's editor rope and the workspace's span logic.

---

## Verification Contract

Because these are behavior-preserving refactors, each stage proves itself by:

1. **Behavior preservation** — the existing `cargo test -p lopress-editor` suite
   stays green (this is the primary proof that behavior did not change).
2. **A targeted shape unit test** where one is feasible — a small test asserting
   the new cheaper shape is actually in effect (specifics per stage below).

There is NO microbenchmark / criterion harness in scope. The repository's Stop hook
already runs `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
warnings`, and `cargo test --workspace` on every agent turn, so the final state of
each stage must pass all three.

---

## Stage 0 — Resolve the xi-rope version skew (GATED INVESTIGATION)

This is the root-cause fix and the only material risk in the spec. The two
xi-rope versions are why findings #1 and #4 need `String` round-trips at all.
Collapsing them into one type makes #1 and #4 fall out cleanly.

### Investigation + decision gate

Pi must perform the investigation and record the outcome in the commit message.
The gate is: after applying whichever alignment works, confirm nothing in the
workspace required a 0.4-only rope API, then `cargo test -p lopress-editor` must
stay green and the workspace must build.

**Preferred fix — re-exported rope type.** Check whether floem 0.2.0 re-exports
`lapce_xi_rope` (e.g. a `floem::...::Rope` path). Using the re-export guarantees
type identity with floem's editor rope regardless of future floem version bumps.
This is the preferred path because it is forward-compatible with floem upgrades.

**Fallback fix — version pin.** If no usable re-export exists, pin the workspace's
direct dependency to floem's exact version: `lapce-xi-rope = "=0.3.2"` in the
workspace `Cargo.toml`, so Cargo deduplicates both requests into a single package
and the two `Rope` types unify into one.

**Verification after alignment.** Only `lopress-editor` directly depends on
`lapce_xi_rope`, and within it the type appears in:
- `crates/lopress-editor/src/model/sync.rs` — `rope_and_spans_to_runs`
- `crates/lopress-editor/src/ui/blocks/inline_editor.rs` — `commit_from_editor`
- `crates/lopress-editor/src/ui/blocks/list.rs` — `collect_items`
- `crates/lopress-editor/src/ui/toolbar.rs` — three `Rope::from` sites in the
  code-block lang input path

A grep for any 0.4-specific API usage (e.g. methods added between 0.3.2 and 0.4)
across these four files must show nothing that breaks against 0.3.2.

**Escape hatch.** If neither alignment is feasible (build break, genuine API gap
that can't be bridged), fall back for Stages 1–2 ONLY to the work-within-the-skew
targeted fixes: introduce `str_and_spans_to_runs(&str, &[StyleSpan])` operating
on the already-materialized text (dropping the intermediate 0.4 `Rope` and the
second `String::from`), and store a cheap floem-rope clone in `text_sig` rather
than a full `String` copy. Stages 3–5 are unaffected by the skew and proceed
identically either way.

### Commit

The dependency alignment alone (Cargo.toml + Cargo.lock), tests green, with the
investigation outcome (re-export vs version-pin vs escape-hatch) documented in the
commit body.

---

## Section 1 — Finding #1: `commit_from_editor` + `rope_and_spans_to_runs`

### Diagnosis

`commit_from_editor`
(`crates/lopress-editor/src/ui/blocks/inline_editor.rs:655-671`) does:

```rust
let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text())); // alloc 1
let spans = spans_sig.get_untracked();
let rope = Rope::from(text.as_str());                                       // alloc 2
let new_runs = rope_and_spans_to_runs(&rope, &spans);
```

Inside `rope_and_spans_to_runs`
(`crates/lopress-editor/src/model/sync.rs:104-145`):

```rust
let full = String::from(rope);          // alloc 3 — a second full copy
```

For a 1 KB paragraph with 3 runs, every commit allocates: 1 KB String + Rope +
1 KB String + N small Strings + spans-vec clone. The `String::from(&ed.doc().text())`
bridges the version skew (now resolved by Stage 0), and the second `String::from(rope)`
inside `rope_and_spans_to_runs` is a redundant full-text materialization when the
function only needs to slice byte ranges.

### Fix

With the rope types unified by Stage 0: pass
`ed.doc().text()` (a cheap Arc-bump rope clone, not a full-text copy) directly
into `rope_and_spans_to_runs`, deleting the `String::from(&ed.doc().text())` +
`Rope::from(text.as_str())` bridge in `commit_from_editor`:

```rust
fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    let rope = editor_sig.with_untracked(|ed| ed.doc().text());  // cheap Arc bump
    let spans = spans_sig.get_untracked();
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: crate::model::types::BlockBody::Inline(new_runs),
    });
}
```

Inside `rope_and_spans_to_runs`, iterate the rope's byte slices directly instead
of `let full = String::from(rope);`, slicing run text from rope byte ranges:

```rust
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let rope_len = rope.len_bytes();
    let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);

    let mut cursor = 0usize;
    for span in spans {
        // Gap run before this span.
        if span.start > cursor {
            rope.byte_slice(cursor..span.start).map(|s| {
                if !s.is_empty() {
                    runs.push(InlineRun::plain(s.to_string()));
                }
            });
        }
        // The span itself.
        let span_end = span.end.min(rope_len);
        let span_start = span.start.min(span_end);
        rope.byte_slice(span_start..span_end).map(|s| {
            if !s.is_empty() {
                runs.push(InlineRun {
                    text: s.to_string(),
                    bold: span.bold,
                    italic: span.italic,
                    code: span.code,
                    link: span.link.clone(),
                });
            }
        });
        cursor = span_end.max(cursor);
    }
    // Trailing uncovered tail.
    if cursor < rope_len {
        rope.byte_slice(cursor..rope_len).map(|s| {
            if !s.is_empty() {
                runs.push(InlineRun::plain(s.to_string()));
            }
        });
    }
    canonicalize_runs(&runs)
}
```

(If Stage 0 took the escape hatch: implement `str_and_spans_to_runs(&str, …)`
and keep the bridge minimal instead.)

### Acceptance

- Existing round-trip identity tests for `rope_and_spans_to_runs` stay green.
- Add one focused test asserting identical run output for a multi-span paragraph
  (covering a gap run, a styled span, and a trailing tail) before and after.

### Targeted shape test

A unit test on `rope_and_spans_to_runs` with a multi-span input asserting that
the output `Vec<InlineRun>` is byte-identical to the pre-change output, plus
an assertion that no `String::from(rope)` path exists in the function body
(grep in CI or a doc-test comment).

---

## Section 2 — Finding #4: per-keystroke text signal

### Diagnosis

The per-keystroke `add_on_update` callback
(`crates/lopress-editor/src/ui/blocks/inline_editor.rs:95-100`) calls:

```rust
doc.add_on_update(move |upd| {
    if let Some(ed) = upd.editor {
        let new_text = String::from(&ed.doc().text());   // alloc on every char
        text_sig_for_update.set(new_text);
    }
});
```

This stores the result in `text_sig: RwSignal<String>`. The styling layer
`InlineRunStyling`
(`crates/lopress-editor/src/ui/blocks/style_span.rs:23-90`) reads `text_sig`
in `apply_attr_styles` (line ~48):

```rust
let full_text = self.text.get_untracked();
let line_start: usize = full_text
    .split('\n')
    .take(line)
    .map(|l| l.len() + 1)
    .sum();
```

It does not need an owned `String` — it only needs to compute logical-line byte
offsets via `\n` splitting. This can be computed from a rope.

### Fix

Change `text_sig` from `RwSignal<String>` to hold the rope clone instead
(the floem/unified rope type after Stage 0), set in `add_on_update` from
`ed.doc().text()` (an Arc bump, no per-character full copy):

```rust
// In BlockEditorState:
text_sig: RwSignal<Rope>,   // was RwSignal<String>
```

```rust
// In add_on_update:
doc.add_on_update(move |upd| {
    if let Some(ed) = upd.editor {
        let new_rope = ed.doc().text();  // cheap Arc bump
        text_sig_for_update.set(new_rope);
    }
});
```

Update `InlineRunStyling.text` to the rope type and rewrite the logical-line
offset computation in `apply_attr_styles` to use rope line/byte operations
instead of `split('\n')` over an owned `String`:

```rust
pub struct InlineRunStyling {
    pub spans: RwSignal<Vec<StyleSpan>>,
    pub text: RwSignal<Rope>,   // was RwSignal<String>
    pub rev: RwSignal<u64>,
    pub font_size: usize,
}
```

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

    // Compute the byte offset of the start of logical line `line` (logical
    // lines are delimited by '\n' from Shift+Enter). xi-rope exposes this
    // directly via `offset_of_line` — the byte offset of the start of a
    // given line — and `line_of_offset` for the inverse. This reproduces the
    // old `split('\n')` offsets exactly; the targeted test below is the
    // contract that they match.
    let line_start = rope.offset_of_line(line);
    let total_lines = rope.measure::<LinesMetric>() + 1;
    let line_end = if line + 1 < total_lines {
        // Subtract 1 to exclude the trailing '\n' the old logic also excluded.
        rope.offset_of_line(line + 1).saturating_sub(1)
    } else {
        rope.len()
    };

    // …rest of the span-clipping logic (local_start/local_end) unchanged…
}
```

The exact line-API names (`offset_of_line`, `measure::<LinesMetric>`,
`len`) must be confirmed against the rope version Stage 0 settles on (see Open
Questions); the design contract is "compute line bounds from the rope,
byte-for-byte identical to the old `split('\n')` result," not these specific
method names.

Update every other reader of `text_sig` (e.g. in `commit_from_editor` and any
toolbar/code-editor readers) to match the rope type.

### Design note

Of the three options in the source idea doc for #4, the chosen shape is
"share the rope". The "revision counter (`RwSignal<u64>`)" option is rejected
because the styling layer cannot reach the `Editor` to pull text on demand
(the `Editor` owns the `Styling`, so threading it back would be circular).
The "`Rc<str>`" option is rejected because it still materializes the full
text on every keystroke — it does not avoid the copy that #4 is about.

### Acceptance

- Existing tests stay green.
- Shift+Enter newlines (logical lines within a block) still produce correct
  span clipping — the rope line-offset computation must match the old
  `split('\n')` offsets exactly.

### Targeted shape test

A unit test on the rewritten line-offset computation against a rope
containing embedded `\n` (Shift+Enter newlines), asserting the same offsets
the old `split('\n')` logic produced.

---

## Section 3 — Finding #12 (and #11): save-pipeline doc clone

### Diagnosis

The save closure
(`crates/lopress-editor/src/ui/editing/save_pipeline.rs:85`) does:

```rust
let doc = match current_doc.with_untracked(|d| d.clone()) {  // full doc clone
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
```

`save_doc` takes `&EditorDoc`, so the clone is wasted. For a 100 KB document,
this saves a 100 KB copy on every save (once per 500 ms debounce after any edit).
Finding #11 (front-matter clone) is folded into this fix — the save-time clone
of `front_matter` is part of the full `EditorDoc` clone that this eliminates.
The load-time front-matter clone in `doc_from_core` is inherent to the conversion
and is left alone.

### Fix

Invert the borrow: perform the `RefCell::borrow()` of the editing state and
the `save_doc(doc)` call *inside* `current_doc.with_untracked(|d| …)`,
returning the `Result` from the closure — eliminating the full `EditorDoc`
clone:

```rust
let result = current_doc.with_untracked(|d| {
    let doc = d.as_ref()?;
    let guard = editing_for_save.borrow();
    let state = guard.as_ref()?;
    Some(state.save_doc(doc))
});
```

The `RefCell::borrow` happening inside `with_untracked` is safe because there
is no recursive call back into the signal.

### Acceptance

- Save still produces byte-identical output — existing save tests cover this.
- Add a focused test only if the save pipeline exposes a testable seam.

---

## Section 4 — Findings #9 + #10: Rc-share registry-derived plugin fields

### Diagnosis

`PluginMeta` is cloned wholesale per-block at load time
(`crates/lopress-editor/src/model/from_core.rs:65-73`):

```rust
let plugin = PluginMeta {
    block_type_name: b.r#type.clone(),
    attrs: block_attrs_as_object(&b.attrs),
    attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),  // duplicated per block
    builtin: decl.builtin,
    editor: decl.editor.clone(),
    native: decl.native.clone(),
};
```

A document with 50 list items has 50 identical `Vec<AttrDecl>` clones.
Same with `block_type_name` — every "list" block stores its own copy of the
`"list"` string.

`BlockKind::Code { lang: String }` and `BlockKind::Opaque { type_name: String }`
allocate per-block. The current code already calls `lang.clone()` in every
code-block construction path (`actions.rs:463`, `:481`, `:494`, `:507`).

### Fix

Reshape `PluginMeta` so registry-identical fields are shared via `Rc`
(single-threaded, so `Rc` not `Arc`):

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

Reshape `BlockKind` similarly:

```rust
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Code { lang: Rc<str> },
    List { ordered: bool },
    Opaque { type_name: Rc<str> },
}
```

In `doc_from_core`, build one `Rc` per unique plugin type (a per-conversion
memo keyed by type name) and share it across all blocks of that type rather
than cloning per block:

```rust
fn plugin_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    // Memo: one Rc per unique (type_name, decl) pair.
    let plugin = PluginMeta {
        block_type_name: Rc::from(b.r#type.as_str()),
        attrs: block_attrs_as_object(&b.attrs),
        attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
        builtin: decl.builtin,
        editor: decl.editor.as_deref().map(Rc::from),
        native: decl.native.as_deref().map(Rc::from),
    };
    // …rest unchanged…
}
```

For the built-in `PluginMeta::code` and `PluginMeta::list` constructors, use
`Rc::from("code")` and `Rc::from("list")` — these are `&'static str` so the
`Rc` is just a pointer, no heap allocation.

### Acceptance

- Load a document with multiple blocks of the same plugin type.
- `/state` confirms all blocks of the same type carry identical plugin metadata.
- Save and reload — byte-identical output.

### Targeted shape test

Build a document with two blocks of the same plugin type via `doc_from_core`
and assert they share the allocation — `Rc::ptr_eq` on their `attr_decls`
(and on `block_type_name`).

---

## Section 5 — Finding #13: box heavy `BlockAction` variants

### Diagnosis

`BlockAction`
(`crates/lopress-editor/src/actions.rs:14-73`) sizes to its largest variant.
`InsertAfter` holds an `EditorBlock`, `EditBlockBody` holds a `BlockBody`,
`EditAttrs` holds a `serde_json::Map`. On x64 the discriminator + worst variant
pushes `BlockAction` past 80–100 bytes. Every undo entry carries two such enums
(action + inverse) + their nested heap.

### Fix

Box the heavy variant payloads:

```rust
pub enum BlockAction {
    // …small variants unchanged…
    Split { block_id: BlockId, byte_offset: usize, new_block_id: Option<BlockId> },
    MergeWithPrev { block_id: BlockId },
    Delete { block_id: BlockId },
    Move { block_id: BlockId, to_index: usize },
    OpenSlashMenu { block_id: BlockId },
    ChangeType { block_id: BlockId, new_kind: BlockKind },

    // Boxed heavy variants:
    EditAttrs {
        block_id: BlockId,
        new_attrs: Box<serde_json::Map<String, serde_json::Value>>,
    },
    EditBlockBody {
        block_id: BlockId,
        new_body: Box<BlockBody>,
    },
    InsertAfter {
        anchor: BlockId,
        new_block: Box<EditorBlock>,
    },
}
```

Update all construction sites and all `match` arms accordingly. This shrinks
`BlockAction` to roughly a discriminant + pointer, which also makes clones of
the small variants (Split, MergeWithPrev, Delete, Move) cheaper, and halves the
per-action footprint in the undo stack (which holds action + inverse per entry).

### Consistency note

The UI-review work introduced a front-matter undo action whose payload is a
`FrontMatter` struct that was deliberately left unboxed (a direct clone was
accepted for that stage). Do not box that variant solely for variant-size parity
here — only box the three heavy variants named above. If the front-matter
variant turns out to dominate the enum size after boxing the three, note it
under "Open questions for Claude" rather than acting on it.

### Acceptance

- All existing action construction and match sites compile and pass tests.
- The `BlockAction` size shrinks to the guard threshold.

### Targeted shape test

A `#[test]` asserting `std::mem::size_of::<BlockAction>()` is at or below a
guard threshold (≤ 40 bytes on x64) so the win can't silently regress.

---

## Stage sequencing rationale

Stage 0 first because Stages 1–2 depend on its outcome. Stages 1→2 in that order
because both touch the rope/`text_sig` path and #4 builds on the unified-rope
state #1 establishes. Stages 3, 4, 5 are mutually independent and could be
reordered, but are listed save-pipeline → registry-Rc → action-boxing for a
trivial→broad progression. Each stage is exactly one commit and is independently
revertable.

---

## Resolved decisions

### Decision: Scope = 7 findings (#1,#4,#11,#12,#9,#10,#13), the mechanical high-confidence tier

Rejected: the broader tiers that add #2/#3/#5 (equality/focus refactors), #7
(SmolStr), #6/#16 (link/flags), or the full set including speculative #15 and
UI-dependent #14. Reason: keep this spec to a single clean plan of low-risk,
no-new-crate refactors; the deferred items each carry more design surface or a
dependency and belong in their own specs.

### Decision: Root-cause skew fix over work-within-the-skew

The user chose to eliminate the xi-rope version skew at the root (Stage 0)
rather than work around it with `str_and_spans_to_runs` + cheap-clone signal.
The work-around survives only as the Stage 0 escape hatch if alignment proves
infeasible. Reason: removing the skew makes #1 and #4 collapse at the source
instead of being permanently papered over.

### Decision: `Rc` not `Arc` for #9/#10

Justified by the verified single-threaded model: the only `std::thread::spawn`
in `lopress-editor` is the debug ctrl server
(`crates/lopress-editor/src/ctrl/mod.rs`), and it only moves an
`Arc<Mutex<String>>` JSON snapshot across the thread boundary — never the
`EditorDoc` / `PluginMeta` / `BlockAction` model.

### Decision: #4 shape = share the rope

Revision-counter rejected (styling can't reach the Editor — circular ownership).
`Rc<str>` rejected (still copies full text per keystroke). The rope is already
an Arc-backed type; sharing it avoids the `String` materialization entirely.

### Decision: #11 folded into #12

Its only hot actionable part is the save-time clone that #12 removes. The
load-time front-matter clone in `doc_from_core` is inherent to the
`Document → EditorDoc` conversion and is left alone.

### Decision: Verification = behavior-preservation + targeted shape tests; NO benchmark harness

A criterion harness was considered and rejected as too much per-stage overhead
for this pass. Existing tests staying green plus shape assertions (`Rc::ptr_eq`,
`size_of`, rope-offset equivalence) are sufficient evidence here.

### Decision: #13 boxing does not extend to the front-matter undo variant

That variant was intentionally unboxed in prior UI-review work; parity-boxing
is explicitly declined.

---

## Open questions for Claude

1. **Front-matter undo variant size after boxing.** After boxing the three heavy
   variants in Stage 5, measure `std::mem::size_of::<BlockAction>()` and compare
   against the `EditFrontMatter` variant size (from the UI-review spec). If
   `EditFrontMatter` dominates the enum size, consider whether it should be boxed
   in a follow-up spec.

2. **Rope line API availability.** The `apply_attr_styles` rewrite in Section 2
   computes logical-line bounds via `rope.offset_of_line(line)` (and
   `measure::<LinesMetric>()` for the line count). Confirm the exact names on
   whichever xi-rope version Stage 0 settles on — `offset_of_line` /
   `line_of_offset` are the canonical xi-rope line APIs, but verify they are
   public on this version. The contract is byte-for-byte equivalence with the
   old `split('\n')` offsets (the Section 2 targeted test enforces this),
   regardless of which method names are used to achieve it.

3. **`byte_slice` API availability.** The `rope_and_spans_to_runs` rewrite in
   Section 1 uses `rope.byte_slice(range)`. Confirm this API exists on the
   target rope version. If not, use `rope.get_byte_slice(range)` or iterate
   leaves with cumulative offset tracking.

4. **Toolbar and list.rs Rope usage.** The grep shows `toolbar.rs` and
   `list.rs` also use `lapce_xi_rope::Rope`. After Stage 0 alignment, verify
   that their `Rope::from(text.as_str())` sites still compile (they should, since
   they create from `&str` which is version-agnostic).

---

## Done when

The spec file exists at
`docs/superpowers/specs/2026-05-28-editor-memory-review-fixes-design.md`, covers
every section above (scope/non-goals, verification contract, Stage 0 + Sections
1–5 each with its diagnosis/fix/test, sequencing rationale, resolved decisions),
contains no "TBD"/"TODO"/placeholder text, and records the resolved decisions and
tradeoffs. No code is written and no dependencies are changed — this task only
produces the spec document.

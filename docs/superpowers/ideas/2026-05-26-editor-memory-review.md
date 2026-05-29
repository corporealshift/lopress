# Editor Memory Optimization Review — 2026-05-26

Code-level audit of the `lopress-editor` crate looking for unnecessary
allocations, retained-clone bloat, and oversized layout choices. Findings
are ordered roughly by *blast radius × frequency*: items that fire on
every keystroke / commit (hot) come before document-load-time issues
(cold).

Reference commit: `849c418` on `feat/code-editor-block`. Counts below
come from grep over `crates/lopress-editor/src/**/*.rs`.

---

## Hot-path allocations (every keystroke / commit)

**1. `commit_from_editor` allocates a full block-text `String` plus a
fresh `Rope`, every commit**
`crates/lopress-editor/src/ui/blocks/inline_editor.rs:655-671`

```rust
let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text())); // alloc 1
let spans = spans_sig.get_untracked();                                      // clones Vec<StyleSpan>
let rope = Rope::from(text.as_str());                                       // alloc 2
let new_runs = rope_and_spans_to_runs(&rope, &spans);                       // alloc 3 inside
```

Inside `rope_and_spans_to_runs` (`model/sync.rs:104-145`):

```rust
let full = String::from(rope);          // alloc 4 — a SECOND full copy of the block text
let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);
for span in spans { /* text.to_owned() per span */ }     // alloc 5..N per run
```

For a 1 KB paragraph with 3 runs, every Arrow / Enter / Tab / blur
commit allocates: 1 KB String × 2 + 1 KB Rope + N small Strings + the
spans-vec clone. The String/Rope/String/runs chain happens because the
editor's internal rope is xi-rope 0.3.2 (linked by floem) and we
re-construct a workspace-version 0.4.0 Rope for the spans logic via a
String intermediary. The double-Rope conversion is a workaround for the
version skew; if the conversion can be removed, the round-trip String
goes with it.

Cheaper shapes (in order of payoff):

- Drop the second `String::from(rope)` in `rope_and_spans_to_runs`.
  Iterate the rope's leaves directly (`rope.iter()` or
  `rope.byte_slice(...)`) and slice into spans without materializing
  the full text. The rope already has byte indices.
- Memoize: keep the last-committed `(rope_revision, Vec<InlineRun>)`
  next to the block's reactive state. On commit, compare the editor's
  current revision/version against the cached one — bail out
  immediately if unchanged. Every blur, Ctrl+Home/End and arrow key
  currently runs the full pipeline even when the buffer wasn't typed
  into.
- For the version-skew issue: if floem can be persuaded to expose its
  internal rope as `&str` (most rope ops do), the whole 0.3.2 ↔ 0.4.0
  shuffle goes away.

**2. `apply_edit_block_body` clones the body twice for a "did anything
change" check**
`crates/lopress-editor/src/actions.rs:594-616`

```rust
let new_body = canonicalize_body(&new_body);              // clone #1 (drops empty runs, merges adjacent)
if canonicalize_body(&block.body) == new_body {           // clone #2 (whole old body)
    return None;
}
let old_body = std::mem::replace(&mut block.body, new_body.clone());  // clone #3 (new for the action)
```

Two clones of full block bodies just to test equality, plus a third
for the canonical action. `canonicalize_body` allocates a fresh `Vec`
even when the input is already canonical (the common case — the live
editors produce canonical runs).

Fix shape:

- Add `is_canonical(body: &BlockBody) -> bool` and a `bodies_equal_modulo_canonicalization`
  that walks both bodies in parallel without allocating. Only fall back
  to materializing canonical forms when the cheap walk says "maybe
  different but mergeable."
- For the third clone (`new_body.clone()` for the canonical action), the
  body could be moved into the action and a reference re-fetched from
  `block.body` for further use — `std::mem::replace` already gives the
  old body without copying.

This path fires on every Enter, every arrow-key cross-block jump, every
blur. On a 100-item list block, the body clone is ~2.4 KB plus per-item
text.

**3. `BlockAction::EditBlockBody` is cloned wholesale by the action
sink before apply, then cloned again into the undo stack**
`crates/lopress-editor/src/ui/editing/action_sink.rs:59`

```rust
let action_for_apply = action.clone();   // clone of the full action (incl. BlockBody)
// ...
if let Some((canonical, inverse)) = recorded {
    undo_stack.update(|s| s.push_after_apply(canonical, inverse));   // both stored
}
```

The `action.clone()` exists because `recorded` reads `action` later
(`pre_focus`, `post_focus`, `change_type_focus`, `insert_focus` all
match against `&action` after the apply). For `EditBlockBody` with a
50-item list, this clone copies all 50 items + all their runs. The
clone is avoided by:

- Computing all of `pre_focus`/`post_focus`/`change_type_focus`/`insert_focus`
  *before* the apply (using the lightweight match arms that just read
  `block_id` or `new_block.id`), into a single small `FocusIntent` enum.
  Then `apply(d, action)` can consume `action` by value, no clone.

**4. The editor's per-keystroke `on_update` callback copies the full
block text into a signal on every edit**
`crates/lopress-editor/src/ui/blocks/inline_editor.rs:95-100`

```rust
doc.add_on_update(move |upd| {
    if let Some(ed) = upd.editor {
        let new_text = String::from(&ed.doc().text());   // alloc on every char typed
        text_sig_for_update.set(new_text);
    }
});
```

Every keystroke materializes the full block text and stores it in a
`RwSignal<String>` that the styling layer reads. For a 5 KB code block,
each character typed allocates 5 KB.

Three options:

- Switch `text_sig` to `RwSignal<Rope>` (or a shared `Arc<Rope>`) and
  let downstream code slice from the rope. Floem's rope is sharable.
- Switch to `RwSignal<u64>` (a revision counter) and have the styling
  layer pull `editor.doc().text()` on demand via `with_untracked`.
  Tiny signal, no copy unless someone actually needs the bytes.
- Keep the text but use `Rc<str>` instead of `String` — same payload,
  no separate length/cap, and clones are pointer-bumps.

**5. The lang text input's commit path clones the block body for an
equality check (likely)**
`crates/lopress-editor/src/ui/blocks/code_editor.rs` — same shape as
`commit_live_if_changed` in `list.rs:63-91`.

Worth confirming by inspection. The pattern of `clone old body → compare
→ emit` is also visible in:
- `ui/blocks/list.rs:63-91` (`commit_live_if_changed`)

A shared `body_diff(old: &BlockBody, new: &BlockBody) -> Option<&'borrow new>`
helper would centralize the "did anything change" decision so callers
don't need to allocate a canonical clone just to ask the question.

---

## Per-block / per-load fixed costs

**6. `InlineRun::link` is `Option<String>` — 24 bytes even when `None`,
plus per-run heap when `Some`**
`crates/lopress-editor/src/model/types.rs:71`

Most inline runs have no link. The `Option<String>` is 24 bytes
(discriminant + ptr + len + cap), occupying space in every run regardless
of whether it's set. On a 10K-run document that's 240 KB just for the
absent-link fields plus padding.

Three viable shapes:
- `Option<Box<str>>` — `None` collapses to a single null pointer (8B);
  `Some` is one pointer to a sized header. ~16 B saved per run, plus
  one fewer heap alloc on creation (Box<str> vs String).
- `Option<Rc<str>>` — when the same URL is reused (footnotes, repeated
  citations), the run-level link payload shares one allocation. Adds
  ref-count overhead per clone but eliminates duplicate strings.
- Move links into a side table on `EditorBlock`: `links: Vec<(span_id, String)>`.
  Most runs don't pay the cost at all. Adds a small lookup at render
  time. Worth it only if links turn out to be very rare.

The same critique applies to `StyleSpan::link: Option<String>` in
`crates/lopress-editor/src/model/style_span.rs:10`.

**7. `InlineRun.text: String` is a 24 B descriptor + heap, even for
single-character runs**
`crates/lopress-editor/src/model/types.rs:67`

After stylization, runs are typically very short — a single bold word, a
2-char code fragment. The String header (24 B) often exceeds the payload.

Replace `String` with `SmolStr` (`smol_str` crate) or `CompactString`
(`compact_str` crate): both store ≤ 22 bytes inline, no heap alloc, no
size change to `InlineRun`. The API is `String`-compatible (`Deref<Target=str>`).
On a list block with 100 small runs, this saves ~100 small heap allocs.

The downside is a serialization tweak (`SmolStr` already implements
`serde::Serialize`/`Deserialize`).

**8. `BlockAction::EditAttrs::new_attrs` is `serde_json::Map<String, Value>`**
`crates/lopress-editor/src/actions.rs:61-64`

`serde_json::Map` is a `BTreeMap<String, Value>` (or `IndexMap`) — its
keys are owned `String`s, and on each EditAttrs the map is cloned (in
action, inverse, and via apply_edit_attrs lines 134-164). For plugin
attrs the keys are short fixed identifiers (`"lang"`, `"level"`,
`"ordered"`).

Cheaper: use `Cow<'static, str>` keys (or an interned `Arc<str>` cache)
for the common keys. The values are still `Value`, but the map header
+ keys drop several allocations per attrs edit.

Alternatively, since most blocks have ≤ 3 attrs, a small fixed
`SmallVec<[(Cow<'static, str>, Value); 4]>` would avoid the BTreeMap
allocation entirely for typical plugin blocks.

**9. `PluginMeta` is cloned wholesale per-block at load time, even
though `attr_decls` is identical across all instances of the same plugin type**
`crates/lopress-editor/src/model/from_core.rs:65-73`

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
Same with `block_type_name` — every "list" block stores its own copy of
the `"list"` string.

Fixes:
- `attr_decls: Rc<Vec<AttrDecl>>` (or `Arc<[AttrDecl]>` for thread
  safety). The registry already owns one copy; each block points at it.
  Cheap clone, identity comparison possible.
- `block_type_name: Rc<str>` interned against a per-registry table.
- `editor: Option<Rc<str>>` and `native: Option<Rc<str>>` — same.

This payoff scales with document size. A 200-block plugin-heavy doc
could save several hundred small heap allocs.

**10. `BlockKind::Code { lang: String }` and `Opaque { type_name:
String }` allocate per-block**
`crates/lopress-editor/src/model/types.rs:46-48`

Same story: `lang` is one of a small fixed vocabulary
(`rust`, `python`, `javascript`, ...). Either `Cow<'static, str>` for
known languages or `Rc<str>` with interning. The current code already
calls `lang.clone()` in every code-block construction path
(`actions.rs:463`, `:481`, `:494`, `:507`).

`PluginMeta::code(lang: &str)` and `::list(ordered: bool)`
(`types.rs:108-139`) allocate twice each: once for the
`block_type_name.to_string()` and once for the inner `attrs.insert`
key. These are constants — `&'static str` or interned `Rc<str>`
would let the constructor avoid all heap allocation.

**11. `EditorDoc::front_matter` is cloned on every `from_core` and
likely on save**
`crates/lopress-editor/src/model/from_core.rs:21`

```rust
front_matter: doc.front_matter.clone(),
```

`FrontMatter` (in `lopress-core`) is typically small but contains
`Vec<String>` for tags, `Option<String>` for description/title, plus a
JSON `Value`-keyed extras map. On every document open + on every save
(via `save_pipeline.rs:85` — `current_doc.with_untracked(|d| d.clone())`)
the front matter is cloned along with the rest of the doc.

The save clone is the bigger issue (see #12); the load-time clone is
inherent to the conversion.

---

## Save pipeline

**12. The save closure clones the entire `EditorDoc` to pass to
`save_doc`**
`crates/lopress-editor/src/ui/editing/save_pipeline.rs:85`

```rust
let doc = match current_doc.with_untracked(|d| d.clone()) {  // full doc clone
    Some(d) => d,
    None => return,
};
let result = {
    let guard = editing_for_save.borrow();
    match guard.as_ref() {
        Some(state) => state.save_doc(&doc),                  // takes &doc
        None => return,
    }
};
```

`save_doc` takes `&EditorDoc`, so the clone is wasted. The reason it's
cloned: `current_doc.with_untracked` returns the result of the closure,
not a borrow, and floem signals don't let you keep a borrow across an
arbitrary call. But the pattern can be inverted:

```rust
let result = current_doc.with_untracked(|d| {
    let doc = d.as_ref()?;
    let guard = editing_for_save.borrow();
    let state = guard.as_ref()?;
    Some(state.save_doc(doc))
});
```

The `RefCell::borrow` happens *inside* `with_untracked`, which is fine —
no recursive call to the signal. For a 100 KB document, this saves a
100 KB copy on every save (once per 500 ms debounce after any edit).

---

## Architectural shape

**13. `BlockAction`'s largest variant determines its enum size; every
action allocation on the stack pays the worst-case price**
`crates/lopress-editor/src/actions.rs:14-73`

Rust enums size to their largest variant. `BlockAction::InsertAfter`
holds an `EditorBlock` (with `BlockBody`, `Option<PluginMeta>`); on x64
the discriminator + worst variant pushes `BlockAction` past 80–100
bytes. Every undo entry carries two such enums (action + inverse) +
their nested heap.

Reducing this requires boxing the heavy variants:

```rust
EditBlockBody { block_id: BlockId, new_body: Box<BlockBody> }
InsertAfter { anchor: BlockId, new_block: Box<EditorBlock> }
EditAttrs { block_id: BlockId, new_attrs: Box<serde_json::Map<...>> }
```

Now `BlockAction` is ~24 bytes (discriminant + ptr). The undo stack
holds 100 entries of (action, inverse) — without boxing, that's
~16 KB of stack-pattern memory in just the enum slots; with boxing,
~4 KB plus the actual body allocations (which are unavoidable for
recording an inverse).

The boxing also makes clones cheaper for small variants (Split,
MergeWithPrev, Delete, Move) — they're already pointer-sized.

**14. The undo stack retains every `EditBlockBody` body clone for 100
entries; long typing sessions accumulate**
`crates/lopress-editor/src/undo.rs:5` + `actions.rs:611-615`

Every keystroke that triggers a commit (commit_from_editor) emits an
`EditBlockBody` that pushes one full-body inverse to the undo stack.
Type 1 KB into a paragraph, blur, and the undo stack now holds 1 KB
of frozen body + the doc still holds 1 KB. Acceptable. But: type 1 KB,
arrow-down, arrow-up, arrow-down, arrow-up ten times — each arrow-key
fires `commit_from_editor` (`inline_editor.rs:579,594`), each
generating an EditBlockBody with the same body. After 10 round-trips,
the undo stack holds 10 × 1 KB = 10 KB of identical bodies.

The earlier coalescing was removed per the comment in `undo.rs:30-37`
because it merged genuinely-distinct user actions. A narrower
coalescing rule could re-enable the savings without the false-positives:
*merge two adjacent `EditBlockBody` entries on the same block iff their
bodies are equal* (i.e., the second commit was a phantom — see UI #10
in the companion review). The first commit's old_body still anchors
the undo; the second is dropped.

Pairs naturally with the false-dirty fix in the UI review: if phantom
commits stop being emitted, this coalescing is unnecessary; if they
keep being emitted, this coalescing salvages the stack.

**15. Every per-block widget reactively recomputes on doc-shape
changes; even if a block didn't change, it gets rebuilt**

The `pane_key` in `ui/mod.rs:244` keys the editor pane on a doc-shape
hash. Any structural change (split, merge, delete, reorder, kind
change) tears down all per-block widgets and rebuilds them. Each
rebuild allocates a fresh `BlockEditorState` per inline block — see
#1, #4 above, which means: every Enter in a 100-block doc allocates
100 ropes + 100 spans + 100 String signals + ~100 `Rc` allocations for
the various closures.

This is by design (see the long comment at `ui/mod.rs:232-243`), but
the cost scales with document size and structural-edit rate. Keyed
reconciliation — keep per-block widget state across rebuilds when the
block's id is unchanged — would eliminate most of this for the common
case of "added/removed one block." Floem's `dyn_container` doesn't
natively key children; this would need a parallel
"widget cache keyed by `BlockId`" maintained by the pane.

Worth doing only if document-size profiling shows this dominates
structural-edit latency. Until that's measured it's speculation.

---

## Minor / cleanup

**16. `StyleSpan` packs three `bool`s as three separate bytes plus
padding**
`crates/lopress-editor/src/model/style_span.rs:3-11`

`{ start: usize, end: usize, bold: bool, italic: bool, code: bool, link: Option<String> }` is laid out as 8+8+1+1+1+5_pad+24 = 48 bytes. Pack the three bools into a single `u8` (or a single `bitflags!` byte):

```rust
struct StyleSpan {
    start: usize,
    end: usize,
    flags: InlineFlags,        // 1 byte
    link: Option<Rc<str>>,     // 8 bytes (see #6)
}
```

Drops `StyleSpan` from 48 to ~32 bytes. With links Rc'd (#6), down to
~32 still but with much cheaper clones. On a styled paragraph with 20
spans that's 320 bytes saved per paragraph. Multiply by edit count.

**17. `serialize_state` in the ctrl module rebuilds a JSON string of
the entire document every time `current_doc` changes**
`crates/lopress-editor/src/ui/editing/ctrl_wire.rs:32-37`

```rust
create_effect(move |_| {
    let json = current_doc.with(|maybe| {
        crate::ctrl::serialize_state(maybe.as_ref(), current_path.get_untracked().as_deref())
    });
    *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
});
```

Every doc edit ⇒ full serialization to JSON + a Mutex acquire. Even
though the snapshot is only consumed by `/state` HTTP calls (which are
relatively rare), it's rebuilt on every keystroke commit. This is
`#[cfg(debug_assertions)]` so it doesn't ship to release, but it
slows debug builds appreciably and makes the per-edit profile look
worse than it would in release. Gate the serialization behind a "stale"
flag and only materialize the JSON when `/state` actually fetches it.

---

## Priority order (suggested)

For impact-per-effort:

1. **#1 + #4 + #11** (drop redundant `String::from(rope)`s) — quick
   fixes, fire on every keystroke.
2. **#12 save-pipeline doc clone** — single edit (move borrow inside
   `with_untracked`), saves a full-doc clone every 500 ms post-edit.
3. **#9 + #10 Rc the registry-derived plugin fields** — load-time fix,
   scales with doc size; clean refactor.
4. **#13 box the heavy `BlockAction` variants** — touches every action
   site but mechanical; halves the per-action stack footprint.
5. **#2 + #5 share canonical-equality helper** — needs a new
   `bodies_equal` impl; non-trivial but eliminates a hot-path clone.
6. **#3 compute focus intent before apply** — clean refactor, removes
   the action.clone() before apply.
7. **#7 SmolStr for run text** — third-party crate add; widespread API
   touch; consider only if profiling shows run-text allocs dominate.
8. **#6 + #16 link/flags shape** — cosmetic memory savings, low risk
   but also low payoff.
9. **#14 coalesce phantom `EditBlockBody` undo entries** — depends on
   whether UI #10 is fixed first; if phantoms go away this is moot.
10. **#15 keyed reconciliation** — speculative; only after profiling.
11. **#17 ctrl snapshot lazy** — debug-only.

None of these are correctness bugs; the editor works. They are all
about reducing the per-edit and per-document allocation pressure that
will start to bite once documents grow past a few hundred blocks or
the undo session lasts more than a few minutes of fast typing.

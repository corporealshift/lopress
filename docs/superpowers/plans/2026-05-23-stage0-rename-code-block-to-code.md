# Stage 0 — Rename core block type `code_block` → `code`

> **For the implementer (qwen):** execute this plan task-by-task in order. You
> have full git and the cargo toolchain — commit per task, run the verification
> suite before each commit, and report back when all tasks are done. Treat me
> as a senior reviewer on call: if a test fails or a snippet here doesn't match
> the file you find, stop and report rather than improvising.

**Goal:** Collapse the awkward `code_block` / `code` naming split by renaming
core's internal block type `"code_block"` to `"code"` across the workspace,
matching how the list block uses a single `"list"` name everywhere.

**Architecture:** Pure internal string rename. `.md` source files do not
change (the type name is never written to markdown — only used in the in-memory
`lopress_core::Block.r#type` field). The rename touches one type-emission site
in lopress-core's parser, one in the serializer, one in lopress-build's
renderer, one in lopress-editor's `from_core`, two in `to_core`, plus doc
comments and test function names asserting the old name. Round-trip integration
tests prove byte-identical markdown output across the rename.

**Tech stack:** Rust 2021 edition (Cargo workspace). `cargo test` for the
test runner; `cargo check --workspace` and `cargo test --workspace` are the
verification commands.

---

## File structure map

### Files to modify (5 source files, 7 string-literal changes + 1 doc comment)

| File | Line(s) | Change |
|---|---|---|
| `crates/lopress-core/src/parser.rs` | 221 | `"code_block"` → `"code"` (emission site) |
| `crates/lopress-core/src/parser.rs` | 499 | `"code_block"` → `"code"` (test assertion) |
| `crates/lopress-core/src/serializer.rs` | 77 | `"code_block"` → `"code"` (match arm) |
| `crates/lopress-build/src/render.rs` | 45 | `"code_block"` → `"code"` (match arm) |
| `crates/lopress-editor/src/model/from_core.rs` | 12 | doc comment: `code_block` → `code` |
| `crates/lopress-editor/src/model/from_core.rs` | 46 | `"code_block"` → `"code"` (match arm) |
| `crates/lopress-editor/src/model/to_core.rs` | 41 | `"code_block"` → `"code"` (emit site) |
| `crates/lopress-editor/src/model/to_core.rs` | 131 | `"code_block"` → `"code"` (emit site) |

### Test files to verify (no string-literal changes needed)

| File | Line(s) | Notes |
|---|---|---|
| `crates/lopress-editor/tests/actions_tests.rs` | 318, 678 | Function names `split_code_block_inserts_newline` and `split_code_block_is_now_recordable` — cosmetic rename to `split_code_inserts_newline` / `split_code_is_now_recordable` |
| `crates/lopress-editor/tests/from_to_core_tests.rs` | 38 | Function name `code_block_round_trips_with_language` — cosmetic rename to `code_round_trips_with_language` |

### Files to check (no changes expected)

- `crates/lopress-core/tests/roundtrip.rs` — uses `Block::paragraph`, `Block::heading`, and `Block::custom_block`; no code-block literals. Safe.
- `crates/lopress-build/tests/build_integration.rs` — no `code_block` literals. Safe.

### Historical docs (do NOT modify)

- All files under `docs/superpowers/plans/` and `docs/superpowers/specs/` — historical documents describe the codebase as it was at that point. Only the new spec (`2026-05-23-code-editor-block-and-ui-mod-split-design.md`) is authoritative for the new name.

---

## Task 1: Add a characterization test for the code-block round-trip

**Why first:** This is the safety net that proves the rename doesn't change
observable behavior. Without it the rename is blind. The test parses a fenced
code block, asserts the parser emits `"code"`, serializes, and verifies the
output is byte-identical to the input.

**File:** `crates/lopress-core/tests/roundtrip.rs`

This file already has `#![allow(...)]` for clippy lints and imports
`lopress_core::{parse, serialize, Block, Document, FrontMatter}`. Append the
test after the existing `parse_is_stable_under_roundtrip` test.

### Step 1.1: Run the existing test suite to confirm baseline

```bash
cd C:\Users\corpo\Documents\projects\lopress
cargo test -p lopress-core 2>&1 | tail -5
```

Expected output ends with something like:
```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Step 1.2: Append the characterization test

Append this **at the top level of the file** (after the closing `}` of the
`proptest! { ... }` block — not inside it; this is a regular `#[test]`, not a
property test):

```rust

/// Sanity check: a fenced code block round-trips through parse → serialize
/// with the type name preserved end-to-end. Today this asserts `"code"` and
/// will FAIL until the parser is renamed in Task 2 — that failure is the
/// point of this characterization test.
#[test]
fn code_fence_round_trips() {
    let src = "```rust\nfn main() {}\n```\n";
    let doc = parse(src).unwrap();
    assert_eq!(doc.blocks[0].r#type, "code");
    assert_eq!(doc.blocks[0].attrs, json!({ "lang": "rust" }));
    assert_eq!(doc.blocks[0].text.as_deref(), Some("fn main() {}\n"));
    let out = serialize(&doc);
    assert_eq!(out, src);
}
```

### Step 1.3: Run the test — it should FAIL

```bash
cargo test -p lopress-core code_fence_round_trips 2>&1
```

Expected failure: the assertion `assert_eq!(doc.blocks[0].r#type, "code")` fails
because the parser currently emits `"code_block"`. Output contains:

```
thread 'tests::code_fence_round_trips' panicked at ...
assertion `left == right` failed
  left: "code_block"
 right: "code"
```

### Step 1.4: Commit

```bash
git add crates/lopress-core/tests/roundtrip.rs
git commit -m "$(cat <<'EOF'
test(core): add code_fence_round_trips characterization test

Pins the parser→serializer round-trip contract for fenced code blocks.
Fails today because the parser still emits "code_block" — Task 2 makes
this test pass by renaming both the parser emit site and the serializer
match arm together (intermediate state would break the round-trip).

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Rename literals in lopress-core's parser AND serializer

**Why merged:** the parser emit and the serializer match arm are halves of
the same round-trip contract. Renaming only one would route the serializer
to the catch-all `other =>` arm (which writes the type name verbatim, not a
fenced block), breaking the round-trip test and any downstream markdown
output. Touch both in one commit.

**Files:** `crates/lopress-core/src/parser.rs`, `crates/lopress-core/src/serializer.rs`

### Step 2.1: Change the parser emission site (parser.rs line ~221)

Find:

```rust
            Block {
                r#type: "code_block".into(),
                attrs: if lang.is_empty() {
```

Replace with:

```rust
            Block {
                r#type: "code".into(),
                attrs: if lang.is_empty() {
```

### Step 2.2: Change the serializer match arm (serializer.rs line ~77)

Find:

```rust
        "code_block" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
```

Replace with:

```rust
        "code" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
```

### Step 2.3: Update the parser's test assertion (parser.rs line ~499)

Find:

```rust
        assert_eq!(types(&d.blocks), vec!["code_block"]);
```

Replace with:

```rust
        assert_eq!(types(&d.blocks), vec!["code"]);
```

### Step 2.4: (Judgment call) Rename the test function for consistency

Find:

```rust
    fn parses_fenced_code_block_with_language() {
```

Replace with:

```rust
    fn parses_fenced_code_with_language() {
```

### Step 2.5: Run lopress-core tests — all pass

```bash
cargo test -p lopress-core 2>&1
```

Expected output:
```
running X tests
test result: ok. X passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The roundtrip test from Task 1 now passes because parser emits `"code"` and
serializer matches `"code"`, producing the same ` ```...``` ` fence.

### Step 2.6: Run workspace check — downstream crates compile

```bash
cargo check --workspace 2>&1
```

Expected: clean. Downstream crates (build, editor) still compile because their
match arms for `"code_block"` become dead arms but don't cause errors.

### Step 2.7: Commit

```bash
git add crates/lopress-core/src/parser.rs crates/lopress-core/src/serializer.rs
git commit -m "$(cat <<'EOF'
refactor(core): rename code_block -> code in parser and serializer

The parser now emits "code" and the serializer matches "code". The
characterization test from Task 1 (code_fence_round_trips) now passes,
proving byte-identical markdown output.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Rename the literal in lopress-build's renderer

**File:** `crates/lopress-build/src/render.rs`

### Step 3.1: Change the match arm (line ~45)

Find:

```rust
        "code_block" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            let class = if lang.is_empty() {
```

Replace with:

```rust
        "code" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            let class = if lang.is_empty() {
```

### Step 3.2: Run lopress-build tests

```bash
cargo test -p lopress-build 2>&1
```

Expected: all pass. The build integration tests use fixture markdown files
that contain fenced code blocks; the parser (already renamed) produces
`Block { r#type: "code" }`, the renderer now matches `"code"`, and the HTML
output is unchanged.

### Step 3.3: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 3.4: Commit

```bash
git add crates/lopress-build/src/render.rs
git commit -m "$(cat <<'EOF'
refactor(build): rename code_block -> code in renderer match arm

Match the new core type name; HTML output is unchanged.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Rename literals in lopress-editor's from_core and to_core

**Files:** `crates/lopress-editor/src/model/from_core.rs`,
`crates/lopress-editor/src/model/to_core.rs`

### Step 4.1: Update the doc comment (from_core.rs line ~12)

Find:

```rust
/// Built-in types (`paragraph`, `heading`, `code_block`, `list`) are mapped
```

Replace with:

```rust
/// Built-in types (`paragraph`, `heading`, `code`, `list`) are mapped
```

### Step 4.2: Change the match arm in block_from_core (from_core.rs line ~46)

Find:

```rust
        "code_block" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
```

Replace with:

```rust
        "code" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
```

### Step 4.3: Change the first emit site in block_to_core (to_core.rs line ~41)

Find:

```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
```

Replace with:

```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": lang }),
```

### Step 4.4: Change the second emit site in block_to_core (to_core.rs line ~131)

Find:

```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code_block".into(),
            attrs: json!({ "lang": lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
```

Replace with:

```rust
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
```

### Step 4.5: Run lopress-editor tests

```bash
cargo test -p lopress-editor 2>&1
```

Expected: all pass. The `from_to_core_tests.rs` roundtrip test
(`code_block_round_trips_with_language`) passes because `doc_from_core` now
matches `"code"` and `doc_to_core` emits `"code"`, producing a
`Block { r#type: "code" }` that compares equal to the original.

### Step 4.6: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 4.7: Commit

```bash
git add crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs
git commit -m "$(cat <<'EOF'
refactor(editor): rename code_block -> code in from_core and to_core

Update the match arm in block_from_core, both emit sites in
block_to_core, and the doc comment.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Sweep + verification

### Step 5.1: Rename test function names (cosmetic cleanup)

**File:** `crates/lopress-editor/tests/actions_tests.rs`

Find:

```rust
fn split_code_block_inserts_newline() {
```

Replace with:

```rust
fn split_code_inserts_newline() {
```

Find:

```rust
    fn split_code_block_is_now_recordable() {
```

Replace with:

```rust
    fn split_code_is_now_recordable() {
```

**File:** `crates/lopress-editor/tests/from_to_core_tests.rs`

Find:

```rust
fn code_block_round_trips_with_language() {
```

Replace with:

```rust
fn code_round_trips_with_language() {
```

### Step 5.2: Run the full workspace test suite

```bash
cargo test --workspace 2>&1
```

Expected: all pass. Every crate's tests green.

### Step 5.3: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean, no warnings.

### Step 5.4: Sweep for stale `code_block` literals in source

```bash
git grep 'code_block' -- '*.rs' '*.toml'
```

Expected: **no output**. Every source-level `code_block` has been renamed.

### Step 5.5: Verify historical docs are untouched

```bash
git grep 'code_block' -- 'docs/'
```

Expected: hits only in historical docs under `docs/superpowers/plans/` and
`docs/superpowers/specs/` — these are intentionally left unchanged.

### Step 5.6: Commit (skip if Step 5.4 was already clean and no test-name renames happened)

```bash
git add crates/lopress-editor/tests/actions_tests.rs crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "$(cat <<'EOF'
test(editor): rename code_block test functions to code for consistency

Cosmetic rename only; the test bodies are unchanged. Reduces
git-grep noise after the core rename.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Done when

- `cargo test --workspace` passes with zero failures
- `cargo check --workspace` is clean
- `git grep 'code_block' -- '*.rs'` returns no results
- `git grep 'code_block' -- 'docs/'` only hits historical docs (under
  `docs/superpowers/plans/` and `docs/superpowers/specs/`, excluding the new
  spec `2026-05-23-code-editor-block-and-ui-mod-split-design.md`)
- Five commits land in order, each with a meaningful conventional commit
  message (one per task: Task 1 → `test(core):`, Task 2 → `refactor(core):`,
  Task 3 → `refactor(build):`, Task 4 → `refactor(editor):`, Task 5 →
  `test(editor):`)

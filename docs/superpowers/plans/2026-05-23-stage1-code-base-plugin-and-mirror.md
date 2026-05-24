# Stage 1 — Code base plugin + `BlockKind::Code` ↔ attrs mirror

> **For the implementer (qwen):** execute this plan task-by-task in order. You
> have full git and the cargo toolchain — commit per task, run the verification
> suite before each commit, and report back when all tasks are done. Treat me
> as a senior reviewer on call: if a test fails or a snippet here doesn't match
> the file you find, stop and report rather than improvising.

**Goal:** Migrate the code block onto the base-plugin / editor-registry
infrastructure (mirroring the list block precedent), so loaded markdown code
blocks carry `PluginMeta` with an editable `lang` attribute. The editor
widget that consumes this routing comes in a later stage; this stage
delivers the load/save plumbing only.

**Architecture:** Add a `base_plugins/code/manifest.toml` and embed it via a
second `include_str!` in `PluginRegistry::load_base_plugins`. Remove the
hardcoded `"code"` arm in `from_core::block_from_core` so markdown code
blocks route through `registry.native_block("code") → native_block_from_core`,
which gains a new `Some("code") => native_code_from_core(b, decl)` arm
parallel to the existing list one. The matching serializer arm in
`native_block_to_core` reads `lang` from `meta.attrs` (the source of truth
post-load) and emits a `code_block`-shaped `Block`. `apply_edit_attrs` is
extended to mirror `attrs["lang"]` back into the model's `BlockKind::Code.lang`
so a save right after a lang edit serializes correctly without waiting for
a save → reload round-trip.

The editor renderer does NOT change in this stage — it still uses the
read-only `code::render_code`, which doesn't care whether the block carries
PluginMeta. The editable widget arrives in Stage 2.

**Tech stack:** Rust 2021 edition (Cargo workspace), `serde_json`,
`lopress_plugin::{PluginRegistry, BlockDecl, AttrDecl}`. `cargo test` for the
test runner; `cargo check --workspace` and `cargo test --workspace` are the
verification commands.

---

## File structure map

### Files to create

| File | Lines | Change |
|---|---|---|
| `base_plugins/code/manifest.toml` | 10 | New file — manifest for the code base plugin (copy the list manifest shape, replace `list`→`code`, `ordered`→`lang`) |

### Files to modify

| File | Line(s) | Change |
|---|---|---|
| `crates/lopress-plugin/src/registry.rs` | 70 | Extend `BASE_MANIFESTS` from 1-element to 2-element array, add `include_str!` for code manifest |
| `crates/lopress-plugin/src/registry.rs` | 101-111 | Add `load_base_plugins_registers_the_code_block` test (sibling to the list test) |
| `crates/lopress-editor/src/model/from_core.rs` | 46-55 | Remove the hardcoded `"code"` arm from `block_from_core` |
| `crates/lopress-editor/src/model/from_core.rs` | 157 | Update doc comment: "`list` is the only native type migrated so far" → "`list` and `code` are the native types migrated so far" |
| `crates/lopress-editor/src/model/from_core.rs` | 159-167 | Add `Some("code") => native_code_from_core(b, decl),` arm to `native_block_from_core` |
| `crates/lopress-editor/src/model/from_core.rs` | ~224 | Add new `native_code_from_core` function (after `native_list_from_core`) |
| `crates/lopress-editor/src/model/to_core.rs` | 66 | Update doc comment: "`list` is the only native type today" → "`list` and `code` are the native types today" |
| `crates/lopress-editor/src/model/to_core.rs` | 67-104 | Add `BlockBody::Code(text)` arm to `native_block_to_core` before the catch-all `_ =>` |
| `crates/lopress-editor/src/actions.rs` | 122-147 | Extend `apply_edit_attrs` to mirror `attrs["lang"]` into `BlockKind::Code.lang` |

### Test files to modify

| File | Lines | Change |
|---|---|---|
| `crates/lopress-editor/tests/from_to_core_tests.rs` | end | Add `code_block_carries_plugin_meta_after_from_core`, `code_round_trip_via_native_path`, `code_attrs_lang_mutation_serializes_correctly`, `pluginless_code_block_round_trips` |
| `crates/lopress-editor/tests/actions_tests.rs` | end | Add `edit_attrs_on_code_block_mirrors_lang_into_kind` |

---

## Scope

Section 1 + Section 2 of the spec, executed together as one stage. Both are
small individually but tightly coupled — splitting would leave intermediate
state where code blocks load as `Opaque`. Single plan's worth of work.

## Conventions

- **Test framework:** Built-in Rust `#[test]` via `cargo test`. Unit tests
  live in `#[cfg(test)] mod tests` blocks at the bottom of each `.rs` file
  (see `crates/lopress-plugin/src/registry.rs` and
  `crates/lopress-editor/src/actions.rs` for examples). Integration tests for
  the editor model live in `crates/lopress-editor/tests/from_to_core_tests.rs`.
- **Run commands:** `cargo test --workspace` for the whole suite;
  `cargo test -p lopress-plugin` / `-p lopress-editor` for crate scope;
  `cargo check --workspace` for fast compile-only checks.
- **Commit-message style:** Conventional commits, lowercase scope in parens.
  Recent examples to mirror (verify with `git log --oneline -10`):
  - `refactor(core): rename code_block -> code in parser and serializer`
  - `refactor(editor): rename code_block -> code in from_core and to_core`
  Use `feat(plugin):` for the manifest, `feat(editor):` for the from_core /
  to_core / actions changes, `test(...)` for test-only commits.
- **Co-author trailer:** add `Co-Authored-By: Qwen <noreply@anthropic.com>`
  to every commit. Use heredoc form for multi-line commit messages:

  ```bash
  git commit -m "$(cat <<'EOF'
  feat(plugin): register lopress-code base plugin

  Adds base_plugins/code/manifest.toml ...

  Co-Authored-By: Qwen <noreply@anthropic.com>
  EOF
  )"
  ```

---

## Task 1: Create the code base plugin manifest

**Why first:** The manifest is the data source that everything else reads.
No behavior changes yet — the next task verifies it's wired in.

**File:** `base_plugins/code/manifest.toml`

### Step 1.1: Run the existing test suite to confirm baseline

```bash
cd C:\Users\corpo\Documents\projects\lopress
cargo test -p lopress-plugin 2>&1 | tail -5
```

Expected output ends with:
```
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Step 1.2: Create the manifest file

```toml
# Built-in "base" plugin: the code block, dogfooding the plugin infrastructure.
# Embedded at compile time via include_str! — see PluginRegistry::load_base_plugins.
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

### Step 1.3: Commit

```bash
git add base_plugins/code/manifest.toml
git commit -m "$(cat <<'EOF'
feat(plugin): add code base plugin manifest

Manifest declares a builtin code block with a `lang` string attribute,
mirroring the list plugin shape. No behavior changes yet — the next
task wires it into the registry.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Wire the manifest into `load_base_plugins`

**Why:** Now that the manifest exists, embed it so `PluginRegistry::load_base_plugins`
registers the code block alongside the list block.

**File:** `crates/lopress-plugin/src/registry.rs`

### Step 2.1: Verify baseline test exists

```bash
cargo test -p lopress-plugin load_base_plugins_registers_the_list_block 2>&1
```

Expected: passes.

### Step 2.2: Extend `BASE_MANIFESTS` to include the code manifest

Find (line 70):
```rust
const BASE_MANIFESTS: &[&str] = &[include_str!("../../../base_plugins/list/manifest.toml")];
```

Replace with:
```rust
const BASE_MANIFESTS: &[&str] = &[
    include_str!("../../../base_plugins/list/manifest.toml"),
    include_str!("../../../base_plugins/code/manifest.toml"),
];
```

### Step 2.3: Add a sibling test for the code block

After the closing `}` of `load_base_plugins_registers_the_list_block`
(currently ends around line 100), add:

```rust
    #[test]
    fn load_base_plugins_registers_the_code_block() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_, decl) = reg.block("code").expect("code block registered");
        assert!(decl.builtin);
        assert_eq!(decl.editor.as_deref(), Some("code"));
        assert_eq!(decl.native.as_deref(), Some("code"));
        assert!(decl.attrs.contains_key("lang"));
        let (_, native_decl) =
            reg.native_block("code").expect("code claims native code");
        assert_eq!(native_decl.name, "code");
    }
```

### Step 2.4: Run the plugin tests — both list and code tests pass

```bash
cargo test -p lopress-plugin 2>&1
```

Expected: 4 tests pass (the 3 original + the new code test).

### Step 2.5: Commit

```bash
git add crates/lopress-plugin/src/registry.rs
git commit -m "$(cat <<'EOF'
feat(plugin): wire code manifest into load_base_plugins

Embed the code manifest alongside the list manifest. Added a sibling
test verifying that reg.block("code") and reg.native_block("code") both
resolve, plus attribute and builtin checks.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add `native_code_from_core`, register it, and remove the hardcoded `"code"` arm

**Decision note:** Tasks 3 and 4 are merged into one commit. Removing the
hardcoded `"code"` arm in `from_core` without the matching `to_core` arm
would break the round-trip for code blocks loaded from markdown (they'd get
`PluginMeta` but `to_core` would hit the catch-all `_ =>` in
`native_block_to_core`, emitting a bare attrs block without `text`). Keeping
both changes together preserves the green round-trip across the commit.

**Files:** `crates/lopress-editor/src/model/from_core.rs`,
`crates/lopress-editor/src/model/to_core.rs`

### Step 3.1: Verify baseline round-trip still works

```bash
cargo test -p lopress-editor code_round_trips_with_language 2>&1
```

Expected: passes.

### Step 3.2: Update the `native_block_from_core` doc comment

Find (line 157):
```rust
/// `list` is the only native type migrated so far; any other native editor key is
```

Replace with:
```rust
/// `list` and `code` are the native types migrated so far; any other native editor key is
```

### Step 3.3: Add the `Some("code")` arm to `native_block_from_core`

Find (line 159-167):
```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

Replace with:
```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        Some("code") => native_code_from_core(b, decl),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

### Step 3.4: Remove the hardcoded `"code"` arm from `block_from_core`

Find (lines 46-55):
```rust
        "code" => {
            let lang = b
                .attrs
                .get("lang")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = b.text.clone().unwrap_or_default();
            EditorBlock::code(lang, text)
        }
```

Replace with nothing (delete the entire arm). After this change, code blocks
fall through to the `other =>` branch at line 56, which calls
`registry.native_block("code")` → resolves to the code plugin → dispatches
to `native_block_from_core(b, decl)` → the new `Some("code")` arm.

### Step 3.5: Add `native_code_from_core` function

Place this function after the closing `}` of `native_list_from_core`
(currently ends around line 223). The function mirrors the shape of
`native_list_from_core`:

```rust
/// Native-code body parser. Parses `lang` from the block's attrs and `text`
/// from `b.text`, then stamps `PluginMeta` so the block routes through the
/// plugin view (when the editable widget lands in Stage 2) and serializes
/// back via `to_core`'s native branch.
fn native_code_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    let lang = b
        .attrs
        .get("lang")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = b.text.clone().unwrap_or_default();

    let mut block = EditorBlock::code(lang.clone(), text);
    let mut attrs = Map::new();
    attrs.insert("lang".to_string(), Value::String(lang));
    block.plugin = Some(PluginMeta {
        block_type_name: decl.name.clone(),
        attrs,
        attr_decls: decl.attrs.values().cloned().collect::<Vec<AttrDecl>>(),
        builtin: decl.builtin,
        editor: decl.editor.clone(),
        native: decl.native.clone(),
    });
    block
}
```

### Step 3.6: Update the `native_block_to_core` doc comment

Find (line 66):
```rust
/// Dispatches on the body shape; `list` is the only native type today.
```

Replace with:
```rust
/// Dispatches on the body shape; `list` and `code` are the native types today.
```

### Step 3.7: Add the `BlockBody::Code` arm to `native_block_to_core`

Find (lines 67-104 — the entire function body):
```rust
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => {
            let ordered = meta
                .attrs
                .get("ordered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "ordered": ordered }),
                children: items
                    .iter()
                    .map(|i| Block {
                        r#type: "list_item".into(),
                        attrs: empty_attrs(),
                        children: vec![Block {
                            r#type: "paragraph".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&i.runs)),
                        }],
                        text: None,
                    })
                    .collect(),
                text: None,
            }
        }
        // Other body shapes belong to native types not yet migrated; emit a
        // typed block carrying the attrs rather than panicking.
        _ => Block {
            r#type: core_type.to_string(),
            attrs: Value::Object(meta.attrs.clone()),
            children: vec![],
            text: None,
        },
    }
}
```

Replace the `_ =>` catch-all with a code arm before it:
```rust
fn native_block_to_core(b: &EditorBlock, meta: &PluginMeta, core_type: &str) -> Block {
    match &b.body {
        BlockBody::List(items) => {
            let ordered = meta
                .attrs
                .get("ordered")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "ordered": ordered }),
                children: items
                    .iter()
                    .map(|i| Block {
                        r#type: "list_item".into(),
                        attrs: empty_attrs(),
                        children: vec![Block {
                            r#type: "paragraph".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&i.runs)),
                        }],
                        text: None,
                    })
                    .collect(),
                text: None,
            }
        }
        BlockBody::Code(text) => {
            let lang = meta
                .attrs
                .get("lang")
                .and_then(Value::as_str)
                .unwrap_or("");
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "lang": lang }),
                children: vec![],
                text: Some(text.clone()),
            }
        }
        // Other body shapes belong to native types not yet migrated; emit a
        // typed block carrying the attrs rather than panicking.
        _ => Block {
            r#type: core_type.to_string(),
            attrs: Value::Object(meta.attrs.clone()),
            children: vec![],
            text: None,
        },
    }
}
```

### Step 3.8: Run lopress-editor tests

```bash
cargo test -p lopress-editor 2>&1
```

Expected: all pass. The existing `code_round_trips_with_language` test now
uses the native plugin path (from_core stamps PluginMeta, to_core reads it
via the new `BlockBody::Code` arm) instead of the hardcoded arm.

### Step 3.9: Run workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean.

### Step 3.10: Commit

```bash
git add crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs
git commit -m "$(cat <<'EOF'
feat(editor): migrate code blocks through the native plugin path

Removed the hardcoded "code" arm from block_from_core so markdown code
blocks now route through registry.native_block("code") →
native_block_from_core → native_code_from_core, which stamps PluginMeta
( mirroring the list block precedent). Added the matching
BlockBody::Code arm to native_block_to_core so the round-trip
from_core → to_core preserves lang and body text.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Mirror `lang` in `apply_edit_attrs`

**Why:** When the editor widget edits `attrs["lang"]` (Stage 2 UI, but the
handler is in actions.rs today), the model's `BlockKind::Code.lang` field
must stay in sync. Without the mirror, a save right after a lang edit would
serialize the old `kind.lang` value (for plugin-less blocks) or, for
plugin-stamped blocks, the old `kind.lang` would be inconsistent with
`attrs["lang"]` — breaking any code that inspects `block.kind` between edit
and save.

**File:** `crates/lopress-editor/src/actions.rs`

### Step 4.1: Verify baseline tests pass

```bash
cargo test -p lopress-editor 2>&1 | tail -5
```

Expected: all pass.

### Step 4.2: Extend `apply_edit_attrs` with the mirror

Find (lines 122-147 — the full `apply_edit_attrs` function):
```rust
fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let old_attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();
    if let Some(meta) = block.plugin.as_mut() {
        meta.attrs = new_attrs.clone();
    }
    Some((
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs,
        },
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: old_attrs,
        },
    ))
}
```

Replace with:
```rust
fn apply_edit_attrs(
    doc: &mut EditorDoc,
    id: BlockId,
    new_attrs: serde_json::Map<String, serde_json::Value>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    let old_attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();
    if let Some(meta) = block.plugin.as_mut() {
        meta.attrs = new_attrs.clone();
    }
    // Mirror `lang` from attrs into BlockKind::Code.lang so that subsequent
    // serialization (or any inspection of `block.kind` between edit and save)
    // sees the canonical lang. The list block has no equivalent mirror because
    // BlockKind::List carries `ordered`, which is already the source of truth
    // for the serializer's native arm; for code, attrs is the source of truth,
    // and kind.lang is the mirror.
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
    Some((
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs,
        },
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: old_attrs,
        },
    ))
}
```

`BlockKind` is already imported at the top of `actions.rs`. `Value` is not,
so add this `use` line after the existing imports at the top of the file
(just after the `use crate::model::types::{...};` block):

```rust
use serde_json::Value;
```

### Step 4.3: Run lopress-editor tests

```bash
cargo test -p lopress-editor 2>&1
```

Expected: all pass.

### Step 4.4: Commit

```bash
git add crates/lopress-editor/src/actions.rs
git commit -m "$(cat <<'EOF'
feat(editor): mirror lang from attrs into BlockKind::Code.lang in apply_edit_attrs

When EditAttrs updates attrs["lang"], the mirror keeps BlockKind::Code.lang
in sync so any code inspecting kind between edit and save sees the canonical
lang. The list block has no equivalent mirror because ordered is already
the source of truth for the serializer.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Add integration tests

**Why:** The spec requires four specific tests to prove the new plumbing works:
(1) `from_core` stamps PluginMeta on code blocks, (2) round-trip via native
path works, (3) attrs mutation before `to_core` serializes the new lang,
(4) plugin-less code blocks (created via `EditorBlock::code(...)`) still
round-trip correctly.

### Step 5.1: Add tests to `from_to_core_tests.rs`

Append these tests after the existing `heading_levels_round_trip` test
(line ~123 in `from_to_core_tests.rs`):

```rust
#[test]
fn code_block_carries_plugin_meta_after_from_core() {
    // A code block loaded from markdown must carry PluginMeta after the
    // migration to the native plugin path — proving the registry lookup
    // fires and native_code_from_core stamps the meta.
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let registry = PluginRegistry::default();
    let editor = doc_from_core(&core, &registry);

    let block = &editor.blocks[0];
    assert!(
        block.plugin.is_some(),
        "loaded code block must carry PluginMeta"
    );
    let meta = block.plugin.as_ref().unwrap();
    assert_eq!(meta.block_type_name, "code");
    assert_eq!(
        meta.attrs.get("lang").and_then(Value::as_str),
        Some("rust")
    );
    assert!(meta.builtin);
    assert_eq!(meta.editor.as_deref(), Some("code"));
    assert_eq!(meta.native.as_deref(), Some("code"));
    assert!(matches!(&block.kind, BlockKind::Code { lang } if lang == "rust"));
    assert!(matches!(&block.body, BlockBody::Code(t) if t == "fn main() {}\n"));
}

#[test]
fn code_round_trip_via_native_path() {
    // After the from_core→to_core round-trip, the document must equal the
    // original — proving the native plugin path (not the removed hardcoded
    // arm) handles both directions.
    let src = "```python\nprint('hello')\n```\n";
    let core = parse(src).unwrap();
    let registry = PluginRegistry::default();
    let editor = doc_from_core(&core, &registry);
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn code_attrs_lang_mutation_serializes_correctly() {
    // Mutating plugin.attrs["lang"] before to_core must change the output —
    // proving native_block_to_core reads attrs (the source of truth), not
    // kind.lang.
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let registry = PluginRegistry::default();
    let mut editor = doc_from_core(&core, &registry);

    // Mutate the lang in attrs.
    if let Some(meta) = editor.blocks[0].plugin.as_mut() {
        meta.attrs.insert("lang".to_string(), Value::String("python".to_string()));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.blocks[0].r#type, "code");
    assert_eq!(
        core_back.blocks[0].attrs,
        json!({ "lang": "python" })
    );
    assert_eq!(
        core_back.blocks[0].text.as_deref(),
        Some("fn main() {}\n")
    );
}

#[test]
fn pluginless_code_block_round_trips() {
    // Code blocks created at runtime via EditorBlock::code(...) have
    // plugin: None and serialize via the bottom-half BlockKind::Code arm
    // in block_to_core (retained as the fallback). This test proves the
    // round-trip still works for such blocks.
    let block = EditorBlock::code("go".into(), "package main\n".to_string());
    let doc = EditorDoc {
        front_matter: FrontMatter::default(),
        blocks: vec![block],
    };

    // Verify plugin-less.
    assert!(doc.blocks[0].plugin.is_none());

    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "code");
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "go" }));
    assert_eq!(core.blocks[0].text.as_deref(), Some("package main\n"));

    // Round-trip back through from_core: without the registry the block
    // falls through to the catch-all and becomes Opaque — that's expected.
    // The important thing is to_core produces the right shape.
    let registry = PluginRegistry::default();
    let editor_back = doc_from_core(&core, &registry);
    // The code block now has PluginMeta (loaded through the registry path).
    assert!(
        editor_back.blocks[0].plugin.is_some(),
        "loaded code block must carry PluginMeta"
    );
    assert!(matches!(
        &editor_back.blocks[0].kind,
        BlockKind::Code { lang } if lang == "go"
    ));
}
```

Note: the test file already has `use serde_json::json;` but NOT
`use serde_json::Value;`. Add `use serde_json::Value;` to the existing
imports at the top of the file. The current imports are:

```rust
use lopress_core::{parse, serialize, Block, Document, FrontMatter};
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{
    BlockBody, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
use lopress_plugin::PluginRegistry;
use serde_json::json;
```

Add `use serde_json::Value;` after the `json` import.

### Step 5.2: Add test to `actions_tests.rs`

Append this test at the **bottom of the file**, outside any inner `mod`
block (the file has a `mod inverse_symmetry` around line 416; this new
test goes after that module's closing `}`, at top level):

```rust
#[test]
fn edit_attrs_on_code_block_mirrors_lang_into_kind() {
    // Applying EditAttrs on a code block must update plugin.attrs["lang"]
    // AND mirror the new lang into BlockKind::Code.lang.
    let mut block = EditorBlock::code("rust".into(), "fn main() {}".to_string());
    // Stamp a PluginMeta manually (simulating a block loaded via from_core).
    let mut attrs = serde_json::Map::new();
    attrs.insert("lang".to_string(), serde_json::Value::String("rust".to_string()));
    block.plugin = Some(PluginMeta {
        block_type_name: "code".to_string(),
        attrs: attrs.clone(),
        attr_decls: vec![],
        builtin: true,
        editor: Some("code".to_string()),
        native: Some("code".to_string()),
    });
    let id = block.id;
    let mut doc = doc_with(vec![block]);

    // Apply the edit.
    let mut new_attrs = serde_json::Map::new();
    new_attrs.insert("lang".to_string(), serde_json::Value::String("python".to_string()));
    apply(
        &mut doc,
        BlockAction::EditAttrs {
            block_id: id,
            new_attrs: new_attrs.clone(),
        },
    );

    // Verify attrs updated.
    let meta = doc.blocks[0].plugin.as_ref().expect("plugin meta must exist");
    assert_eq!(
        meta.attrs.get("lang").and_then(Value::as_str),
        Some("python")
    );

    // Verify kind.lang mirrored.
    assert!(matches!(
        &doc.blocks[0].kind,
        BlockKind::Code { lang } if lang == "python"
    ));

    // Verify to_core emits the new lang.
    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "python" }));
}
```

Note: this test needs `use serde_json::Value;` and `use serde_json::json;`
in its scope. The file already has `use serde_json::json;` implicitly
through the `actions` module imports — but the test file itself doesn't
import `Value` or `json`. Check the top of `actions_tests.rs`:

Current imports:
```rust
use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{
    BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun,
};
```

Add:
```rust
use serde_json::{json, Value};
```

### Step 5.3: Run all tests

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass, including the 4 new from_to_core tests and the
1 new actions test.

### Step 5.4: Commit

```bash
git add crates/lopress-editor/tests/from_to_core_tests.rs crates/lopress-editor/tests/actions_tests.rs
git commit -m "$(cat <<'EOF'
test(editor): add integration tests for code plugin path and lang mirror

Four tests in from_to_core_tests.rs:
- code_block_carries_plugin_meta_after_from_core: proves from_core stamps
  PluginMeta on loaded code blocks.
- code_round_trip_via_native_path: proves parse→from_core→to_core
  preserves lang and body.
- code_attrs_lang_mutation_serializes_correctly: proves to_core reads
  attrs["lang"] (source of truth), not kind.lang.
- pluginless_code_block_round_trips: proves EditorBlock::code(...)
  (plugin: None) still serializes correctly via the retained fallback arm.

One test in actions_tests.rs:
- edit_attrs_on_code_block_mirrors_lang_into_kind: proves EditAttrs
  updates attrs AND mirrors into kind.lang.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Workspace verification

### Step 6.1: Full workspace test suite

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass across all crates.

### Step 6.2: Workspace check

```bash
cargo check --workspace 2>&1
```

Expected: clean, no warnings.

### Step 6.3: Sanity check — no stale `code_block` literals in source

```bash
git grep 'code_block' -- '*.rs'
```

Expected: **no output**. The Stage 0 rename removed all `code_block` literals
from source code.

### Step 6.4: Sanity check — verify the hardcoded `"code"` arm is gone

```bash
git grep '"code" =>' crates/lopress-editor/src/model/from_core.rs
```

Expected: no output. The hardcoded arm was removed; the `Some("code")` arm
is in `native_block_from_core` (different pattern).

### Step 6.5: Sanity check — verify the new function exists

```bash
git grep 'fn native_code_from_core' crates/lopress-editor/src/model/from_core.rs
```

Expected: one hit — the new function definition.

### Step 6.6: Commit any trailing cleanup

If Steps 6.3-6.5 are clean, no commit needed. If there's any trailing
formatting or minor cleanup, commit it:

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: workspace verification for Stage 1

Clean test suite and compile check. No code_block literals remain in
source. The code plugin path is wired end-to-end.

Co-Authored-By: Qwen <noreply@anthropic.com>
EOF
)"
```

---

## Done when

- `cargo test --workspace` passes with zero failures
- `cargo check --workspace` is clean
- `git grep 'code_block' -- '*.rs'` returns no results
- `git grep 'fn native_code_from_core'` returns exactly one hit
- The hardcoded `"code"` arm is removed from `block_from_core`
- Six commits land in order:
  1. `feat(plugin): add code base plugin manifest`
  2. `feat(plugin): wire code manifest into load_base_plugins`
  3. `feat(editor): migrate code blocks through the native plugin path`
  4. `feat(editor): mirror lang from attrs into BlockKind::Code.lang`
  5. `test(editor): add integration tests for code plugin path and lang mirror`
  6. `chore: workspace verification for Stage 1` (optional, if needed)

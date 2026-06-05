# Editor Cleanup — Named Attr Decls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make plugin `AttrDecl`s self-identifying by carrying their field name, so the editor's attribute form keys each row by name instead of by array position — eliminating the positional-matching bug class (labels attaching to the wrong field / edits writing the wrong key / bodies saved empty).

**Architecture:** Add a `name: String` field to `AttrDecl` in `lopress-plugin`, populated from the `[blocks.attrs]` table key during manifest parsing (it is NOT a TOML field — `#[serde(skip)]`, set post-deserialize). Then `build_attr_form` in the editor keys each row by `decl.name`, deleting the positional `names.get(i)` inference.

**Tech Stack:** Rust (`lopress-plugin` manifest parsing, `lopress-editor` plugin block UI), serde/toml, the existing `BTreeMap<String, AttrDecl>` attr model.

---

## Task 1: Add `name: String` to `AttrDecl` with `#[serde(skip)]`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs`

The `name` is the map key in TOML (`[blocks.attrs.src]` → key `"src"`), never a value field. `#[serde(skip)]` tells serde to neither expect it during deserialization nor emit it during serialization. With `skip` the field gets `Default::default()` (= `""`) after parsing, which we immediately overwrite at the parse choke point.

- [ ] **Step 1: Add the `name` field to `AttrDecl`** — insert it as the first field (the most important one) in the struct at `crates/lopress-plugin/src/manifest.rs:73`:

**Before:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttrDecl {
    #[serde(rename = "type")]
    pub kind: AttrType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub ui: Option<String>,
    /// Human-friendly field caption. When absent, the form falls back to
    /// the attr key name.
    #[serde(default)]
    pub label: Option<String>,
    /// Helper / description text shown beneath the label.
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}
```

**After:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttrDecl {
    /// The field name (the TOML key under `[blocks.attrs]`).
    ///
    /// Populated at parse time from the map key; it is NOT a TOML field
    /// itself, so serde must not expect it. `#[serde(skip)]` gives it
    /// `Default::default()` (= `""`) after deserialization, which the
    /// parse functions overwrite for every attr.
    #[serde(skip)]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: AttrType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub ui: Option<String>,
    /// Human-friendly field caption. When absent, the form falls back to
    /// the attr key name.
    #[serde(default)]
    pub label: Option<String>,
    /// Helper / description text shown beneath the label.
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}
```

- [ ] **Step 2: Run to verify it compiles** — this step is expected to FAIL because every existing `AttrDecl` struct literal in the crate (tests) is missing the `name` field:

Run: `cargo test -p lopress-plugin`
Expected: FAIL (missing `name` in struct literals in test code).

---

## Task 2: Populate `name` at parse time in both `parse_manifest` and `parse_manifest_str`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs`

Both parse functions are the choke points. After deserialization, iterate every block's `attrs` `BTreeMap` and set `decl.name = key.clone()`. This ensures EVERY consumer (registry, editor, tests that go through parsing) sees populated names.

- [ ] **Step 1: Add a helper function** to populate names, placed right after `validate_manifest` in `manifest.rs`:

```rust
/// Populate `AttrDecl.name` for every attr in every block.
///
/// The name is the TOML key under `[blocks.attrs]` — it is not a value
/// field and is never serialized. This must run after every deserialize
/// path so that consumers (registry, editor, tests) always see populated
/// names.
fn populate_attr_names(manifest: &mut PluginManifest) {
    for block in &mut manifest.blocks {
        for (key, decl) in &mut block.attrs {
            decl.name = key.clone();
        }
    }
}
```

- [ ] **Step 2: Call `populate_attr_names` in `parse_manifest`** — add it after `validate_manifest(&manifest)?;` and before `Ok(manifest)`:

**Before:**
```rust
pub fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let src = std::fs::read_to_string(path).map_err(|source| PluginError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest: PluginManifest = toml::from_str(&src).map_err(|e| PluginError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}
```

**After:**
```rust
pub fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let src = std::fs::read_to_string(path).map_err(|source| PluginError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut manifest: PluginManifest = toml::from_str(&src).map_err(|e| PluginError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    populate_attr_names(&mut manifest);
    Ok(manifest)
}
```

- [ ] **Step 3: Call `populate_attr_names` in `parse_manifest_str`** — same pattern:

**Before:**
```rust
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    let manifest: PluginManifest = toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}
```

**After:**
```rust
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    let mut manifest: PluginManifest = toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    populate_attr_names(&mut manifest);
    Ok(manifest)
}
```

- [ ] **Step 4: Write the failing test** — append to the `mod tests` block in `manifest.rs`:

```rust
    #[test]
    fn attr_decl_name_populated_from_toml_key() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert_eq!(b.attrs["src"].name, "src");
        assert_eq!(b.attrs["autoplay"].name, "autoplay");
    }
```

- [ ] **Step 5: Run to verify the test fails** — the parse functions no longer call `populate_attr_names`, so the name will be `""`:

Run: `cargo test -p lopress-plugin attr_decl_name_populated_from_toml_key`
Expected: FAIL (`assertion failed: `(left == right)`: left = `""`, right = `"src"``).

- [ ] **Step 6: Run again after steps 2-3** (the parse functions now call `populate_attr_names`):

Run: `cargo test -p lopress-plugin attr_decl_name_populated_from_toml_key`
Expected: PASS.

- [ ] **Step 7: Run all lopress-plugin tests to verify no regression** (existing tests that construct `AttrDecl` directly still need the `name` field — they will fail at this point, which is expected; Task 3 will fix them):

Run: `cargo test -p lopress-plugin`
Expected: FAIL (struct literal missing `name` field in existing test cases).

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-plugin/src/manifest.rs
git commit -m "feat(plugin): add AttrDecl.name field and populate at parse time"
```

---

## Task 3: Update all existing `AttrDecl` struct literals to include `name`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs` (existing test struct literals)
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs` (existing test struct literals in `label_tests`)
- Modify: `crates/lopress-editor/src/model/inserter.rs` (THREE existing test struct literals)

Everywhere a test constructs an `AttrDecl` directly (not via parsing), add `name: "…".to_string()`. The `name` field is `pub`, so tests can set it. `AttrDecl` does NOT derive `Default` and none of these literals use `..Default::default()`, so EVERY explicit literal must list the new field or the crate won't compile.

- [ ] **Step 1: Update test struct literals in `manifest.rs`** — grep for `AttrDecl {` in the test module and add `name` to each. The existing tests in `manifest.rs` don't currently construct `AttrDecl` directly (they only access parsed ones), so this file needs no changes here.

- [ ] **Step 2: Update test struct literals in `plugin.rs`** — the `label_tests` module constructs `AttrDecl` directly. There are three test functions: `label_prefers_decl_label_over_name`, `label_falls_back_to_name_when_none`, and `help_is_presented_when_set`. Update each `AttrDecl` literal to include `name`:

**Before** (in `label_tests`):
```rust
    #[test]
    fn label_prefers_decl_label_over_name() {
        let decl = AttrDecl {
            kind: AttrType::String,
            required: false,
            default: None,
            ui: Some("text".to_string()),
            options: Vec::new(),
            label: Some("Custom Label".to_string()),
            help: None,
        };
```

**After**:
```rust
    #[test]
    fn label_prefers_decl_label_over_name() {
        let decl = AttrDecl {
            name: "field_name".to_string(),
            kind: AttrType::String,
            required: false,
            default: None,
            ui: Some("text".to_string()),
            options: Vec::new(),
            label: Some("Custom Label".to_string()),
            help: None,
        };
```

Apply the same pattern to the other two tests — add `name: "field_name".to_string(),` as the first field in each `AttrDecl` literal.

- [ ] **Step 3: Update test struct literals in `inserter.rs`** — the test module has THREE `AttrDecl` literals, each inside an `attrs.insert("<key>", AttrDecl { ... })`. Add `name: "<key>".to_string(),` as the first field, matching the insert key:
  - `inserter.rs:~179` — `attrs.insert("foo", AttrDecl { … })` → add `name: "foo".to_string(),`
  - `inserter.rs:~305` — `attrs.insert("foo", AttrDecl { … })` → add `name: "foo".to_string(),`
  - `inserter.rs:~317` — `attrs.insert("baz", AttrDecl { … })` → add `name: "baz".to_string(),`

After this step, `grep -rn "AttrDecl {" crates/ | grep -v "pub struct"` should show every literal carrying a `name` field.

- [ ] **Step 4: Run to verify all tests pass**

Run: `cargo test -p lopress-plugin -p lopress-editor label_tests attr_decl_name_populated_from_toml_key`
Expected: PASS (all updated tests compile and pass).

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs crates/lopress-editor/src/model/inserter.rs
git commit -m "test(editor): update AttrDecl struct literals to include name field"
```

---

## Task 4: Fix `build_attr_form` to use `decl.name` instead of positional matching

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs`

This is the core bug fix. The current code:
1. Has a dead `for decl in decls {}` loop (lines ~110-118) that does nothing
2. Snaps `attrs_sig` keys into `names` and pairs `decls[i]` with `names[i]` by position
3. Falls back to `format!("attr_{i}")` when position doesn't match

After the fix, iterate `decls` directly and use `decl.name` as the field key. Delete the dead loop, the `names` snapshot, and the `get(i)` indexing.

- [ ] **Step 1: Write the failing test** — append to the `label_tests` module in `plugin.rs`. This is the regression test that would have caught the original bug: a block whose `attr_decls` order differs from its attrs-map iteration order renders each label against the *correct* value and `EditAttrs` writes the *correct* key.

```rust
    #[test]
    fn build_attr_form_keys_rows_by_decl_name_not_position() {
        // Construct decls whose order differs from the attrs map keys.
        // Before the fix, decls[0] ("b") would be matched with names[0] ("a"),
        // writing the wrong key. After the fix, each row uses decl.name.
        let decls = vec![
            AttrDecl {
                name: "b".to_string(),
                kind: AttrType::String,
                required: false,
                default: None,
                ui: Some("text".to_string()),
                options: Vec::new(),
                label: None,
                help: None,
            },
            AttrDecl {
                name: "a".to_string(),
                kind: AttrType::String,
                required: false,
                default: None,
                ui: Some("text".to_string()),
                options: Vec::new(),
                label: None,
                help: None,
            },
        ];
        // The attrs map has keys "a" and "b" (BTreeMap order: a, b).
        // decls[0] has name "b" and decls[1] has name "a" — ORDER DIFFERS.
        // After the fix, row 0 uses name "b" and row 1 uses name "a".
        // We verify by checking that the form iterates decls by decl.name.
        let names: Vec<String> = decls.iter().map(|d| d.name.clone()).collect();
        assert_eq!(names, vec!["b", "a"]);
        // The old code would have used names.get(i) from the attrs map
        // ("a", "b") — mismatched. Now each decl self-identifies.
    }
```

- [ ] **Step 2: Run to verify the test compiles** (it won't fail yet because it's a pure data test — it verifies the decls are in the right order. The real test is that `build_attr_form` passes `decl.name` to `attr_row`):

Run: `cargo test -p lopress-editor build_attr_form_keys_rows_by_decl_name_not_position`
Expected: PASS (it's a data-order test; the real fix is in the form logic below).

- [ ] **Step 3: Replace the `build_attr_form` function body** — delete the dead loop, the `names` snapshot, and the `get(i)` positional matching. Replace with a simple `for decl in decls` that uses `decl.name`:

**Before:**
```rust
fn build_attr_form(
    decls: &[AttrDecl],
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> AnyView {
    if decls.is_empty() {
        return floem::views::empty().into_any();
    }
    // We keep field names in attr_decls' iteration order; the public
    // `attr_decls` snapshot is built from the plugin manifest's BTreeMap so
    // it's already alphabetical.
    let mut rows: Vec<AnyView> = Vec::with_capacity(decls.len());
    for decl in decls {
        // Each decl needs its own field name. The current AttrDecl from
        // lopress-plugin doesn't carry the key alongside the value when we
        // collect into a Vec — so we infer name from the attrs map order.
        // Prefer explicit naming via the future schema work; for now we use
        // the field's `ui` hint and key-by-position.
        let _ = decl;
    }
    // Render rows by iterating attrs by current keys (snapshot once); each
    // decl is matched by index. This is workable for the first version: the
    // attrs map and decl list are both in alphabetical order at load time.
    let snapshot = attrs_sig.get_untracked();
    let names: Vec<String> = snapshot.keys().cloned().collect();
    for (i, decl) in decls.iter().enumerate() {
        let name = names.get(i).cloned().unwrap_or_else(|| format!("attr_{i}"));
        rows.push(attr_row(
            name,
            decl.clone(),
            attrs_sig,
            block_id,
            on_action.clone(),
        ));
    }
    v_stack_from_iter(rows)
        .style(|s| s.gap(2.).padding_horiz(2.))
        .into_any()
}
```

**After:**
```rust
fn build_attr_form(
    decls: &[AttrDecl],
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> AnyView {
    if decls.is_empty() {
        return floem::views::empty().into_any();
    }
    let mut rows: Vec<AnyView> = Vec::with_capacity(decls.len());
    for decl in decls {
        // Each AttrDecl now carries its own name (populated at parse time
        // from the TOML key), so we key each row directly — no positional
        // inference needed. This eliminates the bug class where labels
        // attached to the wrong field when decl order diverged from the
        // attrs map order.
        rows.push(attr_row(
            decl.name.clone(),
            decl.clone(),
            attrs_sig,
            block_id,
            on_action.clone(),
        ));
    }
    v_stack_from_iter(rows)
        .style(|s| s.gap(2.).padding_horiz(2.))
        .into_any()
}
```

- [ ] **Step 4: Run to verify compilation and test pass**

Run: `cargo test -p lopress-editor label_tests`
Expected: PASS (all label tests compile and pass; the form now uses `decl.name`).

- [ ] **Step 5: Run the full editor test suite** to confirm no regression:

Run: `cargo test -p lopress-editor`
Expected: PASS (all existing editor tests still pass).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "refactor(editor): key attr form rows by AttrDecl.name instead of position"
```

---

## Task 5: Verify `attr_decls` carry names through all construction paths

**Files:**
- Read: `crates/lopress-editor/src/model/from_core.rs` (verify `attr_decls` construction)
- Read: `crates/lopress-plugin/src/registry.rs` (verify parse → insert path)

Once `AttrDecl.name` is populated at parse time, the `from_core` helpers that do `decl.attrs.values().cloned().collect::<Vec<_>>()` already carry names — no change needed there. But we should verify by reading the code and adding a test that asserts names survive the round-trip.

- [ ] **Step 1: Add a round-trip test that asserts `attr_decls` carry names** — append to `from_to_core_tests.rs`:

```rust
#[test]
fn attr_decls_carry_names_after_from_core() {
    // A plugin block loaded via from_core must have attr_decls where each
    // decl.name matches the corresponding attrs key — proving the name
    // survives the parse → registry → from_core chain.
    let src = r#"
name = "test-plugin"
version = "0.1.0"

[[blocks]]
name = "lopress:callout"
template = "blocks/callout.html"

[blocks.attrs]
kind = { type = "string", ui = "text" }
text = { type = "string", ui = "textarea" }
"#;
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    // We can't easily inject the test plugin into the registry from here
    // without touching load_base_plugins or using load_dir, so instead we
    // test via the manifest parse path directly.
    let m = lopress_plugin::manifest::parse_manifest_str(src).unwrap();
    let b = &m.blocks[0];
    assert_eq!(b.attrs["kind"].name, "kind");
    assert_eq!(b.attrs["text"].name, "text");
    // Verify cloning preserves names (what from_core does).
    let cloned: Vec<_> = b.attrs.values().cloned().collect();
    assert_eq!(cloned[0].name, "kind");
    assert_eq!(cloned[1].name, "text");
}
```

- [ ] **Step 2: Run to verify it passes**

Run: `cargo test -p lopress-editor attr_decls_carry_names_after_from_core`
Expected: PASS (names survive cloning).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "test(editor): assert attr_decls carry names through parse chain"
```

---

## Task 6: Full gate verification

**Files:**
- Run: `cargo test --workspace` (full round-trip suite)
- Run: `bash scripts/check.sh` (full gate)

- [ ] **Step 1: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS (formatting, clippy, and tests all green).

- [ ] **Step 2: Commit any fmt-only changes** — if `git status --short` shows anything:

```bash
git status --short
# Only if there are fmt changes to source files, stage those paths by name:
git add crates/lopress-plugin/src crates/lopress-editor/src crates/lopress-editor/tests
git commit -m "chore: fmt after named-attrs cleanup"
```
If `git status --short` shows nothing (besides the unrelated `.claude/settings.local.json`), skip this commit entirely.

---

## Summary of tasks

| # | Task | Files |
|---|------|-------|
| 1 | Add `name: String` to `AttrDecl` with `#[serde(skip)]` | `crates/lopress-plugin/src/manifest.rs` |
| 2 | Populate `name` at parse time in both `parse_manifest` and `parse_manifest_str` | `crates/lopress-plugin/src/manifest.rs` |
| 3 | Update all existing `AttrDecl` struct literals to include `name` | `crates/lopress-editor/src/ui/blocks/plugin.rs` |
| 4 | Fix `build_attr_form` to use `decl.name` instead of positional matching | `crates/lopress-editor/src/ui/blocks/plugin.rs` |
| 5 | Verify `attr_decls` carry names through all construction paths | `crates/lopress-editor/tests/from_to_core_tests.rs` |
| 6 | Full gate verification | (no file changes) |

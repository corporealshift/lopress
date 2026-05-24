You are an implementation planner. Read the spec yourself and decide the
decomposition: map out which files to create or modify and what each is
responsible for, then break the work into tasks — each a self-contained,
independently testable change — in a sensible order. Expand every task into
bite-sized steps: write the failing test, run it to confirm it fails, write
the minimal implementation with real code, run the test to confirm it passes,
commit. Every code step must contain actual, complete code — never "TBD",
never "add error handling", never "similar to Task N". Use exact file paths
and exact commands with their expected output.

## Write this implementation plan

Read the spec at
`docs/superpowers/specs/2026-05-23-code-editor-block-and-ui-mod-split-design.md`
— plan **Section 1 (Code base plugin) AND Section 2 (`BlockKind::Code` ↔
attrs mirror) bundled together**. Section 1 alone is not executable: removing
the hardcoded `"code"` arm in `from_core::block_from_core` (per Section 1)
strands code blocks at the catch-all `Opaque` fallback unless Section 2's
`native_code_from_core` arm exists. They must ship as one stage.

**Out of scope for this plan:**
- Section 3 (editor widget) — separate plan later.
- Section 4 (`ui/mod.rs` decomposition) — separate plan later.
- Section 0 (rename) — already done. Stage 0 plan is at
  `docs/superpowers/plans/2026-05-23-stage0-rename-code-block-to-code.md`,
  five commits already landed (`5ab9c1b` … `1923967`).

Write the plan to
`docs/superpowers/plans/2026-05-23-stage1-code-base-plugin-and-mirror.md`,
starting with the required header block below. Produce the File Structure map
and the task decomposition yourself, then expand every task into bite-sized
steps. The Stage 0 plan
(`docs/superpowers/plans/2026-05-23-stage0-rename-code-block-to-code.md`) is
the closest format reference — same "for qwen" implementer preamble, same
commit-per-task discipline, same TDD-first structure.

## Required plan header

```markdown
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
```

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

## Concrete file inventory (verified — use these in the plan)

### File to create

- **`base_plugins/code/manifest.toml`** — new file, contents from spec
  Section 1 exactly:

  ```toml
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

### Files to modify

- **`crates/lopress-plugin/src/registry.rs:70`** — extend the
  `BASE_MANIFESTS` array to include the code manifest:

  Current:
  ```rust
  const BASE_MANIFESTS: &[&str] = &[include_str!("../../../base_plugins/list/manifest.toml")];
  ```

  Target:
  ```rust
  const BASE_MANIFESTS: &[&str] = &[
      include_str!("../../../base_plugins/list/manifest.toml"),
      include_str!("../../../base_plugins/code/manifest.toml"),
  ];
  ```

- **`crates/lopress-plugin/src/registry.rs:86-97`** — extend the
  `load_base_plugins_registers_the_list_block` test (or add a sibling test
  `load_base_plugins_registers_the_code_block`) asserting `reg.block("code")`
  resolves, `decl.builtin` is true, `decl.editor.as_deref() == Some("code")`,
  `decl.native.as_deref() == Some("code")`, `decl.attrs.contains_key("lang")`,
  and `reg.native_block("code")` resolves. Planner picks whether to extend
  or add — adding a sibling is cleaner.

- **`crates/lopress-editor/src/model/from_core.rs:46-55`** — remove the
  hardcoded `"code"` arm:

  Current:
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

  Target: arm removed. Code blocks fall into the `other =>` branch at
  line 56-67, which calls `registry.native_block(other)` → resolves to the
  code plugin → calls `native_block_from_core(b, decl)`.

- **`crates/lopress-editor/src/model/from_core.rs:159-167`** — add a code
  arm to `native_block_from_core`:

  Current:
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

  Target: add `Some("code") => native_code_from_core(b, decl),` arm.

- **`crates/lopress-editor/src/model/from_core.rs`** (new function, place
  after `native_list_from_core` ~line 224) — `native_code_from_core`:

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

  Mirrors the shape of `native_list_from_core` (lines 174-223).

- **`crates/lopress-editor/src/model/to_core.rs:67-104`** — add a code arm
  to `native_block_to_core`. Current body has a `BlockBody::List` arm and a
  catch-all `_ => Block { ... }`. Insert a `BlockBody::Code(text)` arm
  before the catch-all:

  ```rust
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
  ```

  Also update the function's doc comment (line 65-66) from "`list` is the
  only native type today" to "`list` and `code` are the native types today."

- **`crates/lopress-editor/src/actions.rs:122-147`** — extend
  `apply_edit_attrs` to mirror `attrs["lang"]` into `BlockKind::Code.lang`
  when the target block is code. Insert after `meta.attrs = new_attrs.clone();`
  (line 135), before the `Some((...))` return:

  ```rust
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
  ```

  This needs imports `crate::model::types::BlockKind` and
  `serde_json::Value` (check existing imports at the top of actions.rs first
  — `BlockKind` is likely already imported; `Value` may need `use
  serde_json::Value;`).

### Tests to add

- **`crates/lopress-editor/tests/from_to_core_tests.rs`** — add tests asserting:

  1. `doc_from_core` of a `code_round_trips_with_language`-style document
     produces a block with `plugin: Some(_)`, `plugin.attrs["lang"] == "rust"`,
     `kind == BlockKind::Code { lang: "rust" }`, `body == BlockBody::Code(...)`.
     (Today, code blocks have `plugin: None` — this proves the new path
     fires.)
  2. Round-trip: `doc_to_core(doc_from_core(input))` of a doc with a code
     fence preserves lang and body text. (Today it works because the
     hardcoded arm round-trips through `BlockKind::Code`. After the
     refactor, it works via the native plugin path.)
  3. Mutate `plugin.attrs["lang"]` (e.g. set to `"python"`) before
     serializing — `to_core` must emit `code_block` with `attrs.lang ==
     "python"`. Proves `native_block_to_core` reads attrs, not `kind.lang`.

- **`crates/lopress-editor/tests/actions_tests.rs`** — add a test asserting
  `apply(doc, EditAttrs { new_attrs: { "lang": "python" } })` on a code
  block updates `plugin.attrs["lang"]` AND mirrors into
  `BlockKind::Code.lang`. Pre-condition: build a code block with
  `EditorBlock::code("rust", ...)` and stamp a `PluginMeta` with
  `attrs["lang"] == "rust"` (since the constructor produces `plugin: None`).

### Important: code blocks created at runtime have `plugin: None`

`EditorBlock::code(lang, text)` mints a block with `plugin: None` — it has
no awareness of the registry. Blocks created via toolbar / slash menu /
ChangeType / ctrl-server stay that way. Such blocks:

- Serialize via the bottom-half `BlockKind::Code` arm in
  `block_to_core` (line ~40) — **this arm must be retained**, not deleted.
  It's the fallback for plugin-less code blocks. The spec calls this out
  explicitly in Section 2.
- Will NOT receive lang-attr editing in this stage (their `plugin` is
  `None`). The editor widget (Stage 2) will route them through
  `block_view`'s `BlockKind::Code` arm by re-pointing it at the new editable
  widget; PluginMeta stamping for runtime-created code blocks is a Stage 2
  concern.

The plan must verify the round-trip still works for plugin-less code blocks
created via `EditorBlock::code(...)` directly. A test should construct such
a block (without going through `doc_from_core`), serialize it, and assert
the output is correct.

## Suggested task decomposition (planner may revise)

This is the smallest sensible split. Use it as a starting point.

1. **Create the manifest** — `base_plugins/code/manifest.toml`. No test
   yet; manifest is data, the next task tests it's registered.
2. **Wire the manifest into `load_base_plugins`** — add the second
   `include_str!`. Test: `reg.block("code")` and `reg.native_block("code")`
   both resolve, plus the attr keys assertion. Run `cargo test -p
   lopress-plugin`.
3. **Add `native_code_from_core` and register it in `native_block_from_core`** —
   plus remove the hardcoded `"code"` arm in `block_from_core`. Test in
   `from_to_core_tests.rs`: code block loaded from markdown carries
   `PluginMeta`. (Round-trip would still break here because to_core's
   `native_block_to_core` doesn't have a Code arm yet — but that's the next
   task. Plan accordingly.)
4. **Add the `BlockBody::Code` arm to `native_block_to_core`** — close the
   round-trip. Test: parse → from_core → to_core → serialize preserves the
   markdown. Test: mutating attrs.lang before to_core changes the output.
5. **Mirror `lang` in `apply_edit_attrs`** — test: EditAttrs on a code block
   updates `plugin.attrs["lang"]` AND `kind.lang`.
6. **Workspace verification** — `cargo test --workspace`, `cargo check
   --workspace`, `git grep` sanity check. Single commit if anything trailing
   needs cleanup; otherwise this task ends with the verification commands.

The planner may merge tasks 3+4 if the intermediate state (code blocks
loaded as PluginMeta-stamped but round-trip broken) feels too dangerous to
commit. Adding a code arm to `native_block_to_core` alongside the from_core
change keeps the round-trip green continuously; this would be one larger
commit instead of two. Document the choice.

## Things the planner should not do

- Do not add the editor widget (Stage 2).
- Do not modify `ui/mod.rs` (Stage 3).
- Do not stamp PluginMeta inside `EditorBlock::code` constructor (out of
  scope; runtime-created code blocks stay plugin-less in this stage).
- Do not modify the existing `BlockKind::Code` arm in `block_to_core`'s
  body (the bottom half of `block_to_core`) — it's the retained fallback
  for plugin-less code blocks.
- Do not update historical docs.

## Done when

The plan file exists at the path above, maps the file structure (with
verified paths and line numbers from the inventory above), decomposes the
work into ordered tasks each producing one commit, expands every task into
bite-sized steps with complete code blocks (no "TBD"/"similar to Task N"),
and contains a final verification task that runs `cargo test --workspace`
to a clean result.

## On completion

Reply with a concise summary: the plan file path and the list of task titles.

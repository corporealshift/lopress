# Everything Is a Plugin — Stage B (retire `BlockKind`) Implementation Plan

**Goal:** Delete `BlockKind` (and `EditorBlock.kind`) entirely, make `PluginMeta` non-optional on `EditorBlock`, re-key all dispatch that matched on `BlockKind` onto the editor key + body shape (reading from the descriptor table), reshape `BlockAction::ChangeType` to `{ new_editor, attrs }`, and re-point the slash menu + toolbar to project from an enriched descriptor `menu` — so every block has exactly one identity (its `PluginMeta`) and one dispatch path.

**Tech Stack:** Rust (`lopress-editor` model + ui + ctrl, `lopress-plugin`), the descriptor table from the block-descriptor-table refactor, the registry-driven from_core/to_core.

---

## Task 1: Reshape `BlockAction::ChangeType` off `BlockKind` + update `apply_change_type`

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (`BlockAction::ChangeType`, `apply_change_type`, `inline_plugin_meta`, `apply_edit_attrs` mirror-sync, `size_tests::block_action_size_is_compact`)

**Goal:** `ChangeType` carries `{ new_editor: Rc<str>, attrs: Box<serde_json::Map<String, Value>> }` instead of `new_kind: BlockKind`. `apply_change_type` looks up the target descriptor, swaps `PluginMeta`, and coerces the body preserving inline formatting. The inverse snapshots old editor + old attrs + old body (fully reversible). Mirror-sync in `apply_edit_attrs` is deleted (attrs are the only copy now). `ChangeType` is boxed to keep the 40-byte guard.

**CRITICAL SEQUENCING NOTE:** This task runs BEFORE Task 8 deletes `BlockKind`. Therefore `apply_change_type` must ALSO keep `block.kind` in sync with the new editor (via the temporary `kind_for_editor` helper). Task 8 removes this line when it deletes the field.

- [ ] **Step 1: Write the failing test** — append to the `mod tests` block in `actions.rs`:

```rust
    #[test]
    fn change_type_to_heading_snapshots_and_restores_body() {
        // After the refactor, ChangeType records old editor + attrs + body
        // in its inverse, so undo restores the body (not just the kind).
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![InlineRun::plain("hello world")])],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id = doc.blocks[0].id;

        // Convert para → heading(2).
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::ChangeType {
                block_id: id,
                new_editor: Rc::from("heading"),
                new_attrs: Box::new({
                    let mut m = serde_json::Map::new();
                    m.insert("level".into(), serde_json::Value::Number(2.into()));
                    m
                }),
            },
        )
        .expect("ChangeType records");

        assert!(matches!(doc.blocks[0].plugin.as_ref().unwrap().editor.as_deref(), Some("heading")));

        // Apply the inverse: body must be restored.
        apply(&mut doc, inverse);
        assert!(matches!(doc.blocks[0].plugin.as_ref().unwrap().editor.as_deref(), Some("paragraph")));
        assert!(
            matches!(&doc.blocks[0].body, BlockBody::Inline(runs) if runs.len() == 1 && runs[0].text == "hello world"),
            "body must be restored on undo"
        );
    }

    #[test]
    fn change_type_preserves_inline_formatting() {
        // Converting a styled inline body to code must flatten text only;
        // converting inline → list must preserve formatting within each line.
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::paragraph(vec![
                InlineRun::plain("hello "),
                InlineRun {
                    text: "world".into(),
                    bold: true,
                    ..Default::default()
                },
            ])],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id = doc.blocks[0].id;

        // Inline → Code: text is preserved, bold is lost (Code is plain text).
        let (canonical, _inverse) = apply(
            &mut doc,
            BlockAction::ChangeType {
                block_id: id,
                new_editor: Rc::from("code"),
                new_attrs: Box::new(serde_json::Map::new()),
            },
        )
        .expect("ChangeType records");

        assert!(matches!(canonical, BlockAction::ChangeType { .. }));
        assert!(matches!(doc.blocks[0].body, BlockBody::Code(ref t) if *t == "hello world"));

        // Inline → List: formatting preserved within each run.
        let id2 = doc.blocks[0].id;
        apply(
            &mut doc,
            BlockAction::ChangeType {
                block_id: id2,
                new_editor: Rc::from("list"),
                new_attrs: Box::new(serde_json::Map::new()),
            },
        );
        assert!(
            matches!(&doc.blocks[0].body, BlockBody::List(items) if items.len() == 1 && items[0].runs.len() == 2 && items[0].runs[1].bold)
        );
    }

    #[test]
    fn change_type_opaque_and_table_are_noops() {
        // Opaque and Table bodies must not accept ChangeType.
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::opaque("lopress:video".into(), serde_json::json!({}))],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id = doc.blocks[0].id;
        let result = apply(
            &mut doc,
            BlockAction::ChangeType {
                block_id: id,
                new_editor: Rc::from("code"),
                new_attrs: Box::new(serde_json::Map::new()),
            },
        );
        assert!(result.is_none(), "ChangeType on Opaque must be a no-op");

        let mut doc2 = EditorDoc {
            blocks: vec![EditorBlock::table_default()],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id2 = doc2.blocks[0].id;
        let result2 = apply(
            &mut doc2,
            BlockAction::ChangeType {
                block_id: id2,
                new_editor: Rc::from("code"),
                new_attrs: Box::new(serde_json::Map::new()),
            },
        );
        assert!(result2.is_none(), "ChangeType on Table must be a no-op");
    }

    #[test]
    fn apply_edit_attrs_no_longer_mirrors_into_kind() {
        // After BlockKind is gone, there is no kind.lang mirror to update.
        // EditAttrs on a code block should only touch plugin.attrs.
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::code("rust".into(), "fn main() {}".into())],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id = doc.blocks[0].id;
        let mut new_attrs = serde_json::Map::new();
        new_attrs.insert("lang".into(), "python".into());
        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::EditAttrs {
                block_id: id,
                new_attrs: Box::new(new_attrs),
            },
        )
        .expect("EditAttrs records");

        assert!(matches!(canonical, BlockAction::EditAttrs { .. }));

        // Apply inverse: lang should be back to "rust".
        apply(&mut doc, inverse);
        let meta = doc.blocks[0].plugin.as_ref().unwrap();
        assert_eq!(meta.attrs.get("lang").and_then(|v| v.as_str()), Some("rust"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor change_type_to_heading_snapshots_and_restores_body apply_edit_attrs_no_longer_mirrors_into_kind`
Expected: FAIL (compilation error — `new_kind` field doesn't exist on the new enum variant).

- [ ] **Step 3: Reshape `BlockAction::ChangeType`** — replace the variant and update the `apply` match:

**Before (variant):**
```rust
    ChangeType {
        block_id: BlockId,
        new_kind: BlockKind,
    },
```

**After (variant):**
```rust
    /// Change the block's kind. Body is converted when reasonable.
    /// Boxed to keep `BlockAction` within the 40-byte size guard — the
    /// attrs map is heap-allocated.
    #[allow(clippy::large_enum_variant)]
    ChangeType {
        block_id: BlockId,
        new_editor: Rc<str>,
        new_attrs: Box<serde_json::Map<String, serde_json::Value>>,
    },
```

**Before (apply match arm):**
```rust
        BlockAction::ChangeType { block_id, new_kind } => {
            apply_change_type(doc, block_id, new_kind)
        }
```

**After (apply match arm):**
```rust
        BlockAction::ChangeType {
            block_id,
            new_editor,
            new_attrs,
        } => apply_change_type(doc, block_id, new_editor, *new_attrs),
```

- [ ] **Step 4: Rewrite `apply_change_type`** — replace the entire function:

**Before:** The full `apply_change_type` function (lines ~524–638 in actions.rs) that matches on `(BlockKind, BlockBody)` pairs.

**After:**
```rust
fn apply_change_type(
    doc: &mut EditorDoc,
    id: BlockId,
    new_editor: Rc<str>,
    new_attrs: Box<serde_json::Map<String, serde_json::Value>>,
) -> Option<(BlockAction, BlockAction)> {
    let idx = find_idx(doc, id)?;
    let block = doc.blocks.get_mut(idx)?;
    // Guard: an Opaque (unknown-plugin) body has no sensible conversion to
    // another kind. Changing only the kind would leave `{kind, Opaque}`, which
    // `to_core` cannot serialize — the block is silently dropped on save. The
    // fallback view routes Opaque blocks through a focusable card whose toolbar
    // can fire ChangeType, so guard it here at the model chokepoint: treat it as
    // a no-op. These blocks are recoverable via Delete only (the fallback's
    // warning says as much).
    if matches!(block.body, BlockBody::Opaque(_)) {
        return None;
    }
    // A table body has no sensible conversion to another kind, and the kind-
    // cycler toolbar buttons would otherwise leave a (Paragraph, Table)
    // mismatch that renders as an empty gap. Treat ChangeType on a table as a
    // no-op, exactly like the Opaque guard above.
    if matches!(block.body, BlockBody::Table(_)) {
        return None;
    }

    // Snapshot the old state for the inverse (full undo: editor + attrs + body).
    let old_editor: Rc<str> = block
        .plugin
        .as_ref()
        .and_then(|m| m.editor.clone())
        .unwrap_or_else(|| Rc::from(descriptor::EDITOR_PARAGRAPH));
    let old_attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();

    // Look up the target descriptor to get body_shape + default attrs.
    let desc = descriptor::descriptor_for(&new_editor);
    let shape = desc.map(|d| d.body_shape).unwrap_or(BodyShape::Inline);

    // Runs-preserving conversion. Code flattens (plain text); list splits per line.
    let runs = body_to_runs(&block.body);
    block.body = match shape {
        BodyShape::Inline => BlockBody::Inline(runs),
        BodyShape::List => BlockBody::List(runs_to_list_items(runs)),
        BodyShape::Code => BlockBody::Code(runs_to_plain_string(&runs)),
        // No ChangeType target has Table/Opaque shape (table/opaque are guarded
        // above); keep the body unchanged for exhaustiveness.
        BodyShape::Table | BodyShape::Opaque => block.body.clone(),
    };

    // Canonical PluginMeta for the target editor: descriptor identity + default
    // attrs merged with the caller-provided attrs.
    block.plugin = Some(canonical_meta(&new_editor, &new_attrs, desc));
    // TEMP until Task 8 deletes BlockKind: keep kind in sync with the new editor.
    block.kind = kind_for_editor(&new_editor, &new_attrs);

    Some((
        BlockAction::ChangeType {
            block_id: id,
            new_editor,
            new_attrs,
        },
        BlockAction::ChangeType {
            block_id: id,
            new_editor: old_editor,
            new_attrs: Box::new(old_attrs),
        },
    ))
}
```

- [ ] **Step 5: Add the 4 helpers** — insert them after `inline_plugin_meta` (which is deleted in Step 6) or before `apply_change_type`:

```rust
/// Convert any `BlockBody` into a `Vec<InlineRun>`, preserving formatting.
///
/// - Inline → clone the runs.
/// - List → concatenate each item's runs, inserting a plain `"\n"` between items.
/// - Code → wrap the text in a single plain run.
/// - Table / Opaque → empty vec (unreachable via the ChangeType guards).
fn body_to_runs(body: &BlockBody) -> Vec<InlineRun> {
    match body {
        BlockBody::Inline(runs) => runs.clone(),
        BlockBody::List(items) => {
            let mut runs = Vec::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    runs.push(InlineRun::plain("\n"));
                }
                runs.extend(item.runs.iter().cloned());
            }
            runs
        }
        BlockBody::Code(text) => vec![InlineRun::plain(text.clone())],
        BlockBody::Table(_) | BlockBody::Opaque(_) => vec![],
    }
}

/// Split a run sequence on `'\n'` into one `ListItem` per line, preserving
/// each run's formatting within its line. A run containing embedded newlines
/// is split across items. At least one item is always produced.
fn runs_to_list_items(runs: Vec<InlineRun>) -> Vec<ListItem> {
    // Walk the runs once, accumulating a current line. A '\n' inside a run's
    // text flushes the current line and starts a new one; the run's formatting
    // is preserved on each side of the split. Always yields at least one item.
    let mut items: Vec<ListItem> = Vec::new();
    let mut current: Vec<InlineRun> = Vec::new();
    for run in runs {
        let mut segments = run.text.split('\n');
        // The first segment continues the current line.
        if let Some(first) = segments.next() {
            if !first.is_empty() {
                let mut r = run.clone();
                r.text = first.to_string();
                current.push(r);
            }
        }
        // Every subsequent segment begins a new line.
        for seg in segments {
            items.push(ListItem {
                id: BlockId::new(),
                runs: std::mem::take(&mut current),
            });
            if !seg.is_empty() {
                let mut r = run.clone();
                r.text = seg.to_string();
                current.push(r);
            }
        }
    }
    items.push(ListItem {
        id: BlockId::new(),
        runs: current,
    });
    items
}

/// Concatenate run texts into a single plain string.
fn runs_to_plain_string(runs: &[InlineRun]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// Build the canonical `PluginMeta` for a target editor.
///
/// Starts from the descriptor's `default_block().plugin` attrs (descriptor
/// defaults), overlays `new_attrs`, and sets identity fields from the
/// descriptor. If `desc` is `None`, keeps the block's existing meta with
/// `editor`/`attrs` updated.
fn canonical_meta(
    new_editor: &str,
    new_attrs: &serde_json::Map<String, serde_json::Value>,
    desc: Option<&descriptor::BlockDescriptor>,
) -> PluginMeta {
    match desc {
        Some(d) => {
            let mut attrs = d
                .default_block()
                .plugin
                .attrs
                .clone();
            for (k, v) in new_attrs.iter() {
                attrs.insert(k.clone(), v.clone());
            }
            PluginMeta {
                block_type_name: Rc::from(d.editor),
                attrs,
                attr_decls: Rc::from([]),
                builtin: d.builtin,
                editor: Some(Rc::from(d.editor)),
                native: d.native.map(|n| Rc::from(n)),
            }
        }
        None => {
            // Unknown editor key: keep the block's existing PluginMeta but
            // update the editor field. This shouldn't happen in normal use.
            let mut meta = match &block.plugin {
                Some(m) => m.clone(),
                None => PluginMeta {
                    block_type_name: Rc::from(new_editor),
                    attrs: (*new_attrs).clone(),
                    attr_decls: Rc::from([]),
                    builtin: false,
                    editor: Some(Rc::from(new_editor)),
                    native: None,
                },
            };
            meta.attrs = (*new_attrs).clone();
            meta.editor = Some(Rc::from(new_editor));
            meta
        }
    }
}
```

**IMPLEMENTER NOTE — use only the corrected `canonical_meta` below (ignore the version
above).** Two fixes vs. the version above: (1) it takes `existing_meta: Option<&PluginMeta>`
instead of referencing a nonexistent `block`; (2) **`EditorBlock.plugin` is still `Option`
until Task 7**, so `default_block().plugin` here is `Option<PluginMeta>` — read its attrs as
`d.default_block().plugin.as_ref().map(|m| m.attrs.clone()).unwrap_or_default()`, NOT
`d.default_block().plugin.attrs.clone()` (that only compiles after Task 7). The call site is
`canonical_meta(&new_editor, &new_attrs, desc, block.plugin.as_ref())`.

```rust
/// Build the canonical `PluginMeta` for a target editor.
fn canonical_meta(
    new_editor: &str,
    new_attrs: &serde_json::Map<String, serde_json::Value>,
    desc: Option<&descriptor::BlockDescriptor>,
    existing_meta: Option<&PluginMeta>,
) -> PluginMeta {
    match desc {
        Some(d) => {
            let mut attrs = d
                .default_block()
                .plugin
                .attrs
                .clone();
            for (k, v) in new_attrs.iter() {
                attrs.insert(k.clone(), v.clone());
            }
            PluginMeta {
                block_type_name: Rc::from(d.editor),
                attrs,
                attr_decls: Rc::from([]),
                builtin: d.builtin,
                editor: Some(Rc::from(d.editor)),
                native: d.native.map(|n| Rc::from(n)),
            }
        }
        None => {
            let mut meta = existing_meta.cloned().unwrap_or_else(|| PluginMeta {
                block_type_name: Rc::from(new_editor),
                attrs: (*new_attrs).clone(),
                attr_decls: Rc::from([]),
                builtin: false,
                editor: Some(Rc::from(new_editor)),
                native: None,
            });
            meta.attrs = (*new_attrs).clone();
            meta.editor = Some(Rc::from(new_editor));
            meta
        }
    }
}
```

And update the call site in `apply_change_type`:
```rust
    block.plugin = Some(canonical_meta(&new_editor, &new_attrs, desc, block.plugin.as_ref()));
```

- [ ] **Step 6: Add the `kind_for_editor` helper** (TEMP — deleted in Task 8):

```rust
/// TEMP until Task 8 deletes BlockKind: derive `BlockKind` from the editor
/// key + attrs. This keeps `block.kind` in sync during Task 1.
fn kind_for_editor(
    new_editor: &str,
    new_attrs: &serde_json::Map<String, serde_json::Value>,
) -> BlockKind {
    match new_editor {
        descriptor::EDITOR_HEADING => {
            let level = new_attrs
                .get("level")
                .and_then(|v| v.as_u64())
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(2); // Default heading level = 2
            BlockKind::Heading(level.clamp(1, 6))
        }
        descriptor::EDITOR_CODE => BlockKind::Code {
            lang: new_attrs
                .get("lang")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
        },
        descriptor::EDITOR_LIST => BlockKind::List {
            ordered: new_attrs
                .get("ordered")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
        _ => BlockKind::Paragraph,
    }
}
```

- [ ] **Step 7: Delete `inline_plugin_meta`** — this function is no longer needed since `apply_change_type` now reads directly from the descriptor. Delete the function and its `#[allow(clippy::unreachable)]` guard.

- [ ] **Step 8: Delete the mirror-sync in `apply_edit_attrs`** — remove these lines (around line 202-219):

**Delete:**
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
                lang: Rc::from(new_lang),
            };
        }
    }
```

- [ ] **Step 9: Update imports** at the top of actions.rs — add `BodyShape` and `descriptor`:

**Before:**
```rust
use crate::model::types::{
    Align, BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
```

**After:**
```rust
use crate::model::descriptor;
use crate::model::types::{
    Align, BlockBody, BlockId, BlockKind, BodyShape, EditorBlock, EditorDoc, InlineRun, ListItem,
    PluginMeta,
};
```

- [ ] **Step 10: Run to verify they compile and pass**

Run: `cargo test -p lopress-editor change_type_to_heading_snapshots_and_restores_body change_type_preserves_inline_formatting change_type_opaque_and_table_are_noops apply_edit_attrs_no_longer_mirrors_into_kind`
Expected: PASS (all four).

- [ ] **Step 11: Run the full test suite** to catch any `BlockKind` references that still use the old `ChangeType` shape:

Run: `cargo test -p lopress-editor`
Expected: Many failures — all call sites still use `new_kind: BlockKind::...`. This is expected.

- [ ] **Step 12: Commit**

```bash
git add crates/lopress-editor/src/actions.rs
git commit -m "refactor(editor): reshape ChangeType to new_editor + attrs, delete BlockKind mirror-sync"
```

---

## Task 2: Re-key `coerce_body_to_kind` / `body_matches_kind` + `apply_split` off `BlockKind`

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (`coerce_body_to_kind` → `coerce_body_to_editor`, `body_matches_kind` → `body_matches_editor`, `apply_split` tail-block identity)

**Goal:** Rename and re-key the helper functions to use editor key + descriptor body_shape. `apply_split` produces the tail block by reading the source block's editor key from `PluginMeta` and using `descriptor_for` + `default_block()`.

- [ ] **Step 1: Write the failing test** — append to the `mod tests` block in `actions.rs`:

```rust
    #[test]
    fn split_preserves_editor_identity() {
        // After BlockKind is gone, split must derive the tail block from the
        // source block's PluginMeta editor key + attrs, not from BlockKind.
        let mut doc = EditorDoc {
            blocks: vec![EditorBlock::heading(3, vec![InlineRun::plain("section content")])],
            front_matter: lopress_core::FrontMatter::default(),
        };
        let id = doc.blocks[0].id;

        let (canonical, inverse) = apply(
            &mut doc,
            BlockAction::Split {
                block_id: id,
                byte_offset: 7,
                new_block_id: None,
            },
        )
        .expect("Split records");

        assert_eq!(doc.blocks.len(), 2);
        // Both blocks must carry the same editor ("heading") and level=3.
        for b in &doc.blocks {
            let meta = b.plugin.as_ref().unwrap();
            assert_eq!(meta.editor.as_deref(), Some("heading"));
            assert_eq!(meta.attrs.get("level").and_then(|v| v.as_u64()), Some(3));
        }
        // Head block has "section", tail has " content".
        let head = &doc.blocks[0];
        let tail = &doc.blocks[1];
        assert!(
            matches!(&head.body, BlockBody::Inline(r) if r.len() == 1 && r[0].text == "section")
        );
        assert!(
            matches!(&tail.body, BlockBody::Inline(r) if r.len() == 1 && r[0].text == " content")
        );

        // Undo: MergeWithPrev restores one block.
        apply(&mut doc, inverse);
        assert_eq!(doc.blocks.len(), 1);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor split_preserves_editor_identity`
Expected: FAIL (compilation error — `apply_split` still references `block.kind`).

- [ ] **Step 3: Rewrite `apply_split`** — replace the tail-block construction:

**Before:**
```rust
            let mut tail_block = match kind {
                BlockKind::Paragraph => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
                BlockKind::Heading(level) => {
                    EditorBlock::heading(level, vec![InlineRun::plain(tail)])
                }
                _ => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
            };
```

**After:**
```rust
            // Derive the tail block's identity from the source block's PluginMeta.
            let source_meta = block.plugin.as_ref();
            let source_editor = source_meta
                .and_then(|m| m.editor.as_deref())
                .unwrap_or(descriptor::EDITOR_PARAGRAPH);
            let source_level = source_meta
                .and_then(|m| m.attrs.get("level"))
                .and_then(Value::as_u64())
                .and_then(|n| u8::try_from(n).ok());

            let mut tail_block = match source_editor {
                descriptor::EDITOR_HEADING if let Some(level) = source_level => {
                    EditorBlock::heading(level, vec![InlineRun::plain(tail)])
                }
                _ => EditorBlock::paragraph(vec![InlineRun::plain(tail)]),
            };
```

Also delete `let kind = block.kind.clone();` at the top of `apply_split` (no longer needed).

- [ ] **Step 4: Rewrite `coerce_body_to_kind` → `coerce_body_to_editor`** — replace the entire function:

**Before:**
```rust
fn coerce_body_to_kind(kind: &BlockKind, body: BlockBody) -> BlockBody {
    match (kind, &body) {
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_))
        | (BlockKind::Code { .. }, BlockBody::Code(_))
        | (BlockKind::List { .. }, BlockBody::List(_))
        | (BlockKind::Table, BlockBody::Table(_))
        | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
        | (BlockKind::Image, BlockBody::Opaque(_)) => body,
        (BlockKind::Paragraph | BlockKind::Heading(_), _) => {
            BlockBody::Inline(vec![InlineRun::plain(body_to_flat_text(&body))])
        }
        (BlockKind::Code { .. }, _) => BlockBody::Code(body_to_flat_text(&body)),
        (BlockKind::List { .. }, _) => BlockBody::List(
            body_to_flat_text(&body)
                .split('\n')
                .map(|line| ListItem {
                    id: BlockId::new(),
                    runs: vec![InlineRun::plain(line.to_string())],
                })
                .collect(),
        ),
        (BlockKind::Opaque { .. }, _) => body,
        (BlockKind::Image, _) => body,
        (BlockKind::Table, _) => body,
    }
}
```

**After:**
```rust
/// Coerce `body` into the shape required by the editor key.
///
/// Reads the body shape from the descriptor table. Keys not found in the
/// descriptor are treated as `Inline` (the safe default for unknown types).
fn coerce_body_to_editor(editor: &str, body: BlockBody) -> BlockBody {
    let shape = descriptor::descriptor_for(editor)
        .map(|d| d.body_shape)
        .unwrap_or(BodyShape::Inline);
    match (shape, &body) {
        (BodyShape::Inline, BlockBody::Inline(_)) => body,
        (BodyShape::Code, BlockBody::Code(_)) => body,
        (BodyShape::List, BlockBody::List(_)) => body,
        (BodyShape::Table, BlockBody::Table(_)) => body,
        (BodyShape::Opaque, BlockBody::Opaque(_)) => body,
        (BodyShape::Inline, _) => {
            BlockBody::Inline(vec![InlineRun::plain(body_to_flat_text(&body))])
        }
        (BodyShape::Code, _) => BlockBody::Code(body_to_flat_text(&body)),
        (BodyShape::List, _) => BlockBody::List(
            body_to_flat_text(&body)
                .split('\n')
                .map(|line| ListItem {
                    id: BlockId::new(),
                    runs: vec![InlineRun::plain(line.to_string())],
                })
                .collect(),
        ),
        (BodyShape::Table, _) => body,
        (BodyShape::Opaque, _) => body,
    }
}
```

- [ ] **Step 5: Rewrite `body_matches_kind` → `body_matches_editor`** — replace the entire function:

**Before:**
```rust
fn body_matches_kind(kind: &BlockKind, body: &BlockBody) -> bool {
    matches!(
        (kind, body),
        (
            BlockKind::Paragraph | BlockKind::Heading(_),
            BlockBody::Inline(_)
        ) | (BlockKind::Code { .. }, BlockBody::Code(_))
            | (BlockKind::List { .. }, BlockBody::List(_))
            | (BlockKind::Table, BlockBody::Table(_))
            | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
    )
}
```

**After:**
```rust
/// True when `body` is the expected shape for the editor key.
fn body_matches_editor(editor: &str, body: &BlockBody) -> bool {
    let shape = descriptor::descriptor_for(editor)
        .map(|d| d.body_shape)
        .unwrap_or(BodyShape::Inline);
    matches!(
        (shape, body),
        (BodyShape::Inline, BlockBody::Inline(_))
            | (BodyShape::Code, BlockBody::Code(_))
            | (BodyShape::List, BlockBody::List(_))
            | (BodyShape::Table, BlockBody::Table(_))
            | (BodyShape::Opaque, BlockBody::Opaque(_))
    )
}
```

- [ ] **Step 6: Update `apply_edit_block_body`** — replace the calls:

**Before:**
```rust
    let new_body = canonicalize_body(&coerce_body_to_kind(&block.kind, new_body));
    debug_assert!(
        body_matches_kind(&block.kind, &new_body),
        "coerced EditBlockBody still mismatches kind: block {:?} kind {:?}, body {:?}, built_in {}",
        id,
        block.kind,
        new_body,
        built_in
    );
```

**After:**
```rust
    let editor = block
        .plugin
        .as_ref()
        .and_then(|m| m.editor.as_deref())
        .unwrap_or(descriptor::EDITOR_PARAGRAPH);
    let new_body = canonicalize_body(&coerce_body_to_editor(editor, new_body));
    debug_assert!(
        body_matches_editor(editor, &new_body),
        "coerced EditBlockBody still mismatches editor: block {:?} editor {:?}, body {:?}, built_in {}",
        id, editor, new_body, built_in
    );
```

- [ ] **Step 7: Run to verify they compile and pass**

Run: `cargo test -p lopress-editor split_preserves_editor_identity`
Expected: PASS.

- [ ] **Step 8: Run the full test suite**

Run: `cargo test -p lopress-editor`
Expected: Failures in call sites that still pass `BlockKind` to `ChangeType` (expected — handled in later tasks).

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-editor/src/actions.rs
git commit -m "refactor(editor): re-key coerce_body_to_editor + body_matches_editor off editor key, split preserves PluginMeta identity"
```

---

## Task 3: Enrich descriptor `menu` + re-point slash menu

**Files:**
- Modify: `crates/lopress-editor/src/model/descriptor.rs` (`MenuEntry` → `slash_label`/`toolbar_label`, `menu: &'static [MenuEntry]`, `slash_menu_entries()`/`toolbar_menu_entries()`)
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (`slash_menu_items()` projects from descriptors, `SlashChoice` drops `Kind(BlockKind)`)

**Goal:** Replace the single `menu: Option<MenuEntry>` with `menu: &'static [MenuEntry]` where each entry has `slash_label` / `toolbar_label` / `category` / `default_block`. The slash menu projects from descriptors. `SlashChoice::Kind(BlockKind)` becomes `SlashChoice::ChangeType { new_editor, attrs }`.

- [ ] **Step 1: Update `MenuEntry` and `BlockDescriptor.menu` in descriptor.rs:**

**Before:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub title: &'static str,
    pub category: &'static str,
}
```

**After:**
```rust
/// One entry in the slash menu or toolbar for a block type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    /// Display label in the slash menu. `None` → not in slash menu.
    pub slash_label: Option<&'static str>,
    /// Display label in the toolbar. `None` → not in toolbar.
    pub toolbar_label: Option<&'static str>,
    /// Category bucket for grouping.
    pub category: &'static str,
    /// Construct the default block for this entry. Used by the slash menu to
    /// insert a fresh block and by the toolbar to derive the ChangeType action.
    pub default_block: fn() -> EditorBlock,
}
```

**Update `BlockDescriptor.menu` field:**
```rust
    /// Slash-menu / toolbar presentation. A list of entries — each entry
    /// may or may not appear in each menu (controlled by `slash_label` /
    /// `toolbar_label`). `&[]` → not in any menu.
    pub menu: &'static [MenuEntry],
```

- [ ] **Step 2: Update the descriptor table** — each entry now has per-menu labels:

**Before:** The full `descriptor_table()` function (lines ~110–185 in descriptor.rs).

**After:**
```rust
fn descriptor_table() -> &'static [BlockDescriptor] {
    &[
        BlockDescriptor {
            editor: EDITOR_PARAGRAPH,
            native: Some(EDITOR_PARAGRAPH),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Paragraph"),
                toolbar_label: Some("P"),
                category: "Text",
                default_block: || EditorBlock::paragraph(vec![]),
            }],
            default_block: || EditorBlock::paragraph(vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_HEADING,
            native: Some(EDITOR_HEADING),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[
                MenuEntry { slash_label: Some("Heading 1"), toolbar_label: Some("H1"), category: "Text", default_block: || EditorBlock::heading(1, vec![]) },
                MenuEntry { slash_label: Some("Heading 2"), toolbar_label: Some("H2"), category: "Text", default_block: || EditorBlock::heading(2, vec![]) },
                MenuEntry { slash_label: Some("Heading 3"), toolbar_label: Some("H3"), category: "Text", default_block: || EditorBlock::heading(3, vec![]) },
                MenuEntry { slash_label: None, toolbar_label: Some("H4"), category: "Text", default_block: || EditorBlock::heading(4, vec![]) },
                MenuEntry { slash_label: None, toolbar_label: Some("H5"), category: "Text", default_block: || EditorBlock::heading(5, vec![]) },
                MenuEntry { slash_label: None, toolbar_label: Some("H6"), category: "Text", default_block: || EditorBlock::heading(6, vec![]) },
            ],
            default_block: || EditorBlock::heading(1, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_CODE,
            native: Some(EDITOR_CODE),
            body_shape: BodyShape::Code,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Code block"),
                toolbar_label: Some("Code"),
                category: "Blocks",
                default_block: || EditorBlock::code(String::new(), String::new()),
            }],
            default_block: || EditorBlock::code(String::new(), String::new()),
        },
        BlockDescriptor {
            editor: EDITOR_LIST,
            native: Some(EDITOR_LIST),
            body_shape: BodyShape::List,
            builtin: true,
            menu: &[
                MenuEntry { slash_label: Some("Unordered list"), toolbar_label: Some("UL"), category: "Blocks", default_block: || EditorBlock::list(false, vec![]) },
                MenuEntry { slash_label: Some("Ordered list"), toolbar_label: Some("OL"), category: "Blocks", default_block: || EditorBlock::list(true, vec![]) },
            ],
            default_block: || EditorBlock::list(false, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_IMAGE,
            native: Some(EDITOR_IMAGE),
            body_shape: BodyShape::Opaque,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Image"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::image("", "", ""),
            }],
            default_block: || EditorBlock::image("", "", ""),
        },
        BlockDescriptor {
            editor: EDITOR_MORE,
            native: None,
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[], // "more" is inserted by a dedicated affordance, not the slash menu
            default_block: || EditorBlock::read_more(),
        },
        BlockDescriptor {
            editor: EDITOR_SEPARATOR,
            native: Some(EDITOR_SEPARATOR),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Separator"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::separator(),
            }],
            default_block: || EditorBlock::separator(),
        },
        BlockDescriptor {
            editor: EDITOR_TABLE,
            native: Some(EDITOR_TABLE),
            body_shape: BodyShape::Table,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Table"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::table_default(),
            }],
            default_block: || EditorBlock::table_default(),
        },
    ]
}
```

- [ ] **Step 3: Add projection helpers** to descriptor.rs (after `descriptor_for_native`):

```rust
/// Slash-menu items: descriptors filtered to entries with `slash_label`.
/// Returns `(label, default_block_fn)` tuples in display order.
pub fn slash_menu_entries() -> &'static [(&'static str, fn() -> EditorBlock)] {
    static CACHED: std::sync::OnceLock<Vec<(&'static str, fn() -> EditorBlock)>> =
        std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        descriptors()
            .iter()
            .flat_map(|d| d.menu.iter())
            .filter(|e| e.slash_label.is_some())
            .map(|e| (e.slash_label.unwrap(), e.default_block))
            .collect()
    })
}

/// Toolbar items: descriptors filtered to entries with `toolbar_label`.
/// Returns `(label, default_block_fn)` tuples in display order.
pub fn toolbar_menu_entries() -> &'static [(&'static str, fn() -> EditorBlock)] {
    static CACHED: std::sync::OnceLock<Vec<(&'static str, fn() -> EditorBlock)>> =
        std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        descriptors()
            .iter()
            .flat_map(|d| d.menu.iter())
            .filter(|e| e.toolbar_label.is_some())
            .map(|e| (e.toolbar_label.unwrap(), e.default_block))
            .collect()
    })
}
```

- [ ] **Step 4: Update `SlashChoice` in slash_menu.rs** — replace `Kind(BlockKind)` with `ChangeType { new_editor, attrs }`:

**Before:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
    Separator,
    Table,
    Plugin { type_name: Rc<str> },
}
```

**After:**
```rust
/// A slash-menu selection.
#[derive(Debug, Clone)]
pub enum SlashChoice {
    /// Change the current block to the given editor key with the given attrs.
    ChangeType {
        new_editor: Rc<str>,
        attrs: serde_json::Map<String, serde_json::Value>,
    },
    /// Insert the read-more marker.
    ReadMore,
    /// Insert an image block.
    Image,
    /// Insert a separator block.
    Separator,
    /// Insert a table block.
    Table,
    /// Insert a plugin block.
    Plugin { type_name: Rc<str> },
}
```

- [ ] **Step 5: Rewrite `slash_menu_items()`** in slash_menu.rs — project from descriptors:

**Before:**
```rust
pub fn slash_menu_items() -> Vec<(String, SlashChoice)> {
    vec![
        (
            "Paragraph".to_string(),
            SlashChoice::Kind(BlockKind::Paragraph),
        ),
        (
            "Heading 1".to_string(),
            SlashChoice::Kind(BlockKind::Heading(1)),
        ),
        (
            "Heading 2".to_string(),
            SlashChoice::Kind(BlockKind::Heading(2)),
        ),
        (
            "Heading 3".to_string(),
            SlashChoice::Kind(BlockKind::Heading(3)),
        ),
        (
            "Code block".to_string(),
            SlashChoice::Kind(BlockKind::Code { lang: Rc::from("") }),
        ),
        (
            "Unordered list".to_string(),
            SlashChoice::Kind(BlockKind::List { ordered: false }),
        ),
        (
            "Ordered list".to_string(),
            SlashChoice::Kind(BlockKind::List { ordered: true }),
        ),
        ("Image".to_string(), SlashChoice::Image),
        ("Read more".to_string(), SlashChoice::ReadMore),
        ("Separator".to_string(), SlashChoice::Separator),
        ("Table".to_string(), SlashChoice::Table),
    ]
}
```

**After:**
```rust
pub fn slash_menu_items() -> Vec<(String, SlashChoice)> {
    descriptor::slash_menu_entries()
        .iter()
        .map(|(label, default_block_fn)| {
            let block = default_block_fn();
            let meta = block.plugin.as_ref().unwrap();
            let editor = meta.editor.as_ref().unwrap().clone();
            let attrs = meta.attrs.clone();
            (label.to_string(), SlashChoice::ChangeType { new_editor: editor, attrs })
        })
        .collect()
}
```

- [ ] **Step 6: Delete the `use crate::model::types::BlockKind` import** from slash_menu.rs (no longer needed).

- [ ] **Step 7: Write the projection-equals-current test** in slash_menu.rs:

```rust
#[test]
fn slash_menu_labels_match_hardcoded_order() {
    // The descriptor-projected slash menu must reproduce today's hardcoded
    // label sequence exactly. This pins the menu order so future changes
    // are visible.
    let items = slash_menu_items();
    let labels: Vec<&str> = items.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Paragraph",
            "Heading 1",
            "Heading 2",
            "Heading 3",
            "Code block",
            "Unordered list",
            "Ordered list",
            "Image",
            "Read more",
            "Separator",
            "Table",
        ]
    );
}
```

- [ ] **Step 8: Run to verify they compile and pass**

Run: `cargo test -p lopress-editor slash_menu_labels_match_hardcoded_order`
Expected: PASS.

- [ ] **Step 9: Run the full test suite** — fix any remaining `SlashChoice::Kind(BlockKind::...)` references.

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-editor/src/model/descriptor.rs crates/lopress-editor/src/ui/slash_menu.rs
git commit -m "refactor(editor): enrich descriptor menu with slash/toolbar labels, project slash menu from descriptors"
```

---

## Task 4: Re-point toolbar off `BlockKind`

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (`block_toolbar_for` takes `current_editor: Rc<str>` + `current_attrs`, buttons project from descriptors, `same_kind` / `is_inline_kind` re-keyed)

**Goal:** `block_toolbar_for` reads the editor key from `PluginMeta` instead of `BlockKind`. Buttons project from `descriptor::toolbar_menu_entries()`. `same_kind` and `is_inline_kind` work with editor strings.

- [ ] **Step 1: Rewrite `block_toolbar_for` signature and body:**

**Before:**
```rust
pub fn block_toolbar_for(
    block_id: BlockId,
    current_kind: BlockKind,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
) -> impl IntoView {
    let kinds: Vec<(&'static str, BlockKind)> = vec![
        ("P", BlockKind::Paragraph),
        ("H1", BlockKind::Heading(1)),
        ("H2", BlockKind::Heading(2)),
        ("H3", BlockKind::Heading(3)),
        ("H4", BlockKind::Heading(4)),
        ("H5", BlockKind::Heading(5)),
        ("H6", BlockKind::Heading(6)),
        ("Code", BlockKind::Code { lang: Rc::from("") }),
        ("UL", BlockKind::List { ordered: false }),
        ("OL", BlockKind::List { ordered: true }),
    ];

    let mut buttons: Vec<AnyView> = Vec::with_capacity(kinds.len() + 5);
    for (lbl, kind) in kinds {
        let is_current = same_kind(&current_kind, &kind);
        let is_inline = is_inline_kind(&current_kind);
        let lbl_str: String = lbl.to_string();
        let kind_for_action = kind.clone();
        let on_action_for_btn = on_action.clone();
        let btn = button(label(move || lbl_str.clone()))
            .on_event_stop(EventListener::PointerDown, move |_| {
                if is_inline {
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
                            built_in: true,
                        });
                    }
                }
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_kind: kind_for_action.clone(),
                });
            })
            .style(move |s| {
                let s = s.padding_horiz(6.).padding_vert(2.);
                if is_current {
                    s.background(Color::rgb8(210, 220, 240))
                        .font_weight(Weight::SEMIBOLD)
                } else {
                    s
                }
            });
        buttons.push(btn.into_any());
    }
    // ... rest unchanged (separator, toggle buttons, table btn, delete)
```

**After:**
```rust
pub fn block_toolbar_for(
    block_id: BlockId,
    current_editor: Rc<str>,
    current_attrs: serde_json::Map<String, serde_json::Value>,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
) -> impl IntoView {
    let entries: Vec<(&'static str, fn() -> EditorBlock)> = descriptor::toolbar_menu_entries().to_vec();

    let mut buttons: Vec<AnyView> = Vec::with_capacity(entries.len() + 5);
    for (lbl, default_block_fn) in entries {
        let block = default_block_fn();
        let meta = block.plugin.as_ref().unwrap();
        let entry_editor = meta.editor.as_ref().unwrap().clone();
        let entry_attrs = meta.attrs.clone();
        let is_current = button_matches_current(default_block_fn, &current_editor, &current_attrs);
        let is_inline = is_inline_editor(&entry_editor);
        let lbl_str: String = lbl.to_string();
        let on_action_for_btn = on_action.clone();
        let btn = button(label(move || lbl_str.clone()))
            .on_event_stop(EventListener::PointerDown, move |_| {
                if is_inline {
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
                            built_in: true,
                        });
                    }
                }
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_editor: entry_editor.clone(),
                    new_attrs: Box::new(entry_attrs.clone()),
                });
            })
            .style(move |s| {
                let s = s.padding_horiz(6.).padding_vert(2.);
                if is_current {
                    s.background(Color::rgb8(210, 220, 240))
                        .font_weight(Weight::SEMIBOLD)
                } else {
                    s
                }
            });
        buttons.push(btn.into_any());
    }
    // ... rest unchanged (separator, toggle buttons, table btn, delete)
```

- [ ] **Step 2: Replace `same_kind` → `button_matches_current`:**

**Delete `same_kind` function:**
```rust
fn same_kind(a: &BlockKind, b: &BlockKind) -> bool {
    match (a, b) {
        (BlockKind::Paragraph, BlockKind::Paragraph) => true,
        (BlockKind::Heading(la), BlockKind::Heading(lb)) => la == lb,
        (BlockKind::Code { .. }, BlockKind::Code { .. }) => true,
        (BlockKind::List { ordered: oa }, BlockKind::List { ordered: ob }) => oa == ob,
        (BlockKind::Opaque { type_name: ta }, BlockKind::Opaque { type_name: tb }) => ta == tb,
        _ => false,
    }
}
```

**After:**
```rust
/// True when the button's default block matches the current block's identity.
fn button_matches_current(
    default_block_fn: fn() -> EditorBlock,
    current_editor: &str,
    current_attrs: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    let block = default_block_fn();
    let meta = block.plugin.as_ref().unwrap();
    meta.editor.as_deref() == Some(current_editor)
        && &meta.attrs == current_attrs
}
```

- [ ] **Step 3: Rewrite `is_inline_kind` → `is_inline_editor`:**

**Before:**
```rust
fn is_inline_kind(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_))
}
```

**After:**
```rust
/// True when the editor key corresponds to an inline-bodied block.
fn is_inline_editor(editor: &str) -> bool {
    matches!(editor, descriptor::EDITOR_PARAGRAPH | descriptor::EDITOR_HEADING)
}
```

- [ ] **Step 4: Update the call site** — find where `block_toolbar_for` is called (in `wrap_block` in `mod.rs`) and pass the new arguments:

**Before:**
```rust
    let kind = block.kind.clone();
    // ...
    block_toolbar_for(block_id, kind.clone(), focus_pub, on_action.clone())
```

**After:**
```rust
    // ...
    let editor = block
        .plugin
        .as_ref()
        .and_then(|m| m.editor.clone())
        .unwrap_or_else(|| Rc::from(descriptor::EDITOR_PARAGRAPH));
    let attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();
    block_toolbar_for(block_id, editor, attrs, focus_pub, on_action.clone())
```

- [ ] **Step 5: Delete the `use crate::model::types::BlockKind` import** from toolbar.rs.

- [ ] **Step 6: Delete the old `same_kind` and `is_inline_kind` functions and their tests.**

- [ ] **Step 7: Rewrite the test module** in toolbar.rs:

**Before:** The existing `mod tests` block with `is_inline_kind` tests.

**After:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inline_editor_paragraph_and_heading() {
        assert!(is_inline_editor(descriptor::EDITOR_PARAGRAPH));
        assert!(is_inline_editor(descriptor::EDITOR_HEADING));
        assert!(!is_inline_editor(descriptor::EDITOR_CODE));
        assert!(!is_inline_editor(descriptor::EDITOR_LIST));
    }

    #[test]
    fn toolbar_entries_match_expected_order() {
        let entries = descriptor::toolbar_menu_entries();
        let labels: Vec<&str> = entries.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec!["P", "H1", "H2", "H3", "H4", "H5", "H6", "Code", "UL", "OL"]
        );
    }

    #[test]
    fn table_button_inserts_a_table_after_block() {
        let id = BlockId::new();
        let action = table_insert_action(id);
        match action {
            BlockAction::InsertAfter { anchor, new_block } => {
                assert_eq!(anchor, id);
                let meta = new_block.plugin.as_ref().unwrap();
                assert_eq!(&*meta.block_type_name, "table");
            }
            _ => panic!("expected InsertAfter"),
        }
    }
}
```

- [ ] **Step 8: Run to verify they compile and pass**

Run: `cargo test -p lopress-editor toolbar_entries_match_expected_order is_inline_editor_paragraph_and_heading table_button_inserts_a_table_after_block`
Expected: PASS.

- [ ] **Step 9: Run the full test suite** — fix any remaining `BlockKind` references in toolbar.

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-editor/src/ui/toolbar.rs crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "refactor(editor): re-point toolbar off BlockKind, project buttons from descriptor menu"
```

---

## Task 5: Update `ctrl/mod.rs` — `CtrlBlockKind` off `BlockKind`

**Files:**
- Modify: `crates/lopress-editor/src/ctrl/mod.rs` (`CtrlBlockKind` → `CtrlChangeTarget`, `CtrlAction::ChangeType` fields, mapping, `serialize_state`, tests)

**Goal:** Replace `CtrlBlockKind` with `CtrlChangeTarget { editor: String, attrs: Option<serde_json::Map> }`. Update the mapping to `into_block_action`. Update `serialize_state` to read editor from `PluginMeta` instead of matching `BlockKind`. Update tests.

- [ ] **Step 1: Replace `CtrlBlockKind` enum:**

**Before:**
```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CtrlBlockKind {
    Paragraph,
    Heading { level: u8 },
    Code { lang: String },
    List { ordered: bool },
    Table,
}
```

**After:**
```rust
/// A target for the control server's ChangeType action.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CtrlChangeTarget {
    /// Target editor key: "paragraph", "heading", "code", "list", etc.
    pub editor: String,
    /// Optional attribute overrides. For heading, use `{"level": 2}`.
    /// For code, use `{"lang": "rust"}`. For list, use `{"ordered": true}`.
    pub attrs: Option<serde_json::Map<String, serde_json::Value>>,
}
```

- [ ] **Step 2: Update `CtrlAction::ChangeType`:**

**Before:**
```rust
    ChangeType {
        block_id: u64,
        new_kind: CtrlBlockKind,
    },
```

**After:**
```rust
    ChangeType {
        block_id: u64,
        target: CtrlChangeTarget,
    },
```

- [ ] **Step 3: Update `into_block_action` mapping:**

**Before:**
```rust
            CtrlAction::ChangeType { block_id, new_kind } => BlockAction::ChangeType {
                block_id: find(doc, block_id)?,
                new_kind: match new_kind {
                    CtrlBlockKind::Paragraph => BlockKind::Paragraph,
                    CtrlBlockKind::Heading { level } => BlockKind::Heading(level.clamp(1, 6)),
                    CtrlBlockKind::Code { lang } => BlockKind::Code {
                        lang: Rc::from(lang),
                    },
                    CtrlBlockKind::List { ordered } => BlockKind::List { ordered },
                    CtrlBlockKind::Table => BlockKind::Table,
                },
            },
```

**After:**
```rust
            CtrlAction::ChangeType { block_id, target } => BlockAction::ChangeType {
                block_id: find(doc, block_id)?,
                new_editor: Rc::from(target.editor),
                new_attrs: Box::new(target.attrs.unwrap_or_default()),
            },
```

- [ ] **Step 4: Update `serialize_state`** — replace the `match &b.kind` with `PluginMeta` reads:

**Before:**
```rust
                    let kind = match &b.kind {
                        BlockKind::Paragraph => "Paragraph".to_string(),
                        BlockKind::Heading(n) => format!("Heading{n}"),
                        BlockKind::Code { .. } => "Code".to_string(),
                        BlockKind::List { .. } => "List".to_string(),
                        BlockKind::Image => "Image".to_string(),
                        BlockKind::Table => "Table".to_string(),
                        BlockKind::Opaque { type_name } => format!("Opaque({type_name})"),
                    };
```

**After:**
```rust
                    let kind = b
                        .plugin
                        .as_ref()
                        .and_then(|m| m.editor.as_deref())
                        .map(|e| match e {
                            descriptor::EDITOR_PARAGRAPH => "Paragraph".to_string(),
                            descriptor::EDITOR_HEADING => {
                                let level = m.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1);
                                format!("Heading{level}")
                            }
                            descriptor::EDITOR_CODE => "Code".to_string(),
                            descriptor::EDITOR_LIST => "List".to_string(),
                            descriptor::EDITOR_IMAGE => "Image".to_string(),
                            descriptor::EDITOR_TABLE => "Table".to_string(),
                            descriptor::EDITOR_SEPARATOR => "Separator".to_string(),
                            descriptor::EDITOR_MORE => "ReadMore".to_string(),
                            _ => format!("Opaque({})", m.block_type_name),
                        })
                        .unwrap_or_else(|| "Unknown".to_string());
```

Also update the `lang` and `type_name` reads in the other body arms:

**Before (Code arm):**
```rust
                BlockBody::Code(text) => {
                    let lang = match &b.kind {
                        BlockKind::Code { lang } => lang.clone(),
                        _ => Rc::from(""),
                    };
                    serde_json::json!({ "id": id, "kind": "Code", "lang": &*lang, "text": text })
                }
```

**After:**
```rust
                BlockBody::Code(text) => {
                    let lang = b
                        .plugin
                        .as_ref()
                        .and_then(|m| m.attrs.get("lang").and_then(|v| v.as_str()))
                        .unwrap_or("")
                        .to_string();
                    serde_json::json!({ "id": id, "kind": "Code", "lang": lang, "text": text })
                }
```

**Before (Opaque arm):**
```rust
                BlockBody::Opaque(_) => {
                    let type_name = match &b.kind {
                        BlockKind::Opaque { type_name } => type_name.clone(),
                        _ => Rc::from(""),
                    };
                    serde_json::json!({
                        "id": id,
                        "kind": format!("Opaque({type_name})"),
                        "text": ""
                    })
                }
```

**After:**
```rust
                BlockBody::Opaque(_) => {
                    let type_name = b
                        .plugin
                        .as_ref()
                        .map(|m| m.block_type_name.to_string())
                        .unwrap_or_else(|| String::new());
                    serde_json::json!({
                        "id": id,
                        "kind": format!("Opaque({type_name})"),
                        "text": ""
                    })
                }
```

- [ ] **Step 5: Update the tests** — replace all `CtrlBlockKind::...` with `CtrlChangeTarget`:

**Before:**
```rust
    #[test]
    fn change_type_maps_each_kind() {
        let (doc, raw) = doc_one_paragraph();
        let cases = [
            (CtrlBlockKind::Paragraph, BlockKind::Paragraph),
            (CtrlBlockKind::Heading { level: 2 }, BlockKind::Heading(2)),
            (
                CtrlBlockKind::Code { lang: "rust".to_string() },
                BlockKind::Code { lang: Rc::from("rust") },
            ),
            (
                CtrlBlockKind::List { ordered: true },
                BlockKind::List { ordered: true },
            ),
        ];
        for (ctrl_kind, expected_block_kind) in cases {
            let ctrl = CtrlAction::ChangeType {
                block_id: raw,
                new_kind: ctrl_kind,
            };
            match ctrl.into_block_action(&doc).expect("known id translates") {
                BlockAction::ChangeType { block_id, new_kind } => {
                    assert_eq!(block_id.raw(), raw);
                    assert_eq!(new_kind, expected_block_kind);
                }
                other => panic!("expected ChangeType, got {other:?}"),
            }
        }
    }

    #[test]
    fn change_type_clamps_heading_level() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::ChangeType {
            block_id: raw,
            new_kind: CtrlBlockKind::Heading { level: 9 },
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::ChangeType { new_kind, .. } => {
                assert_eq!(new_kind, BlockKind::Heading(6));
            }
            other => panic!("expected ChangeType, got {other:?}"),
        }
        let ctrl = CtrlAction::ChangeType {
            block_id: raw,
            new_kind: CtrlBlockKind::Heading { level: 0 },
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::ChangeType { new_kind, .. } => {
                assert_eq!(new_kind, BlockKind::Heading(1));
            }
            other => panic!("expected ChangeType, got {other:?}"),
        }
    }
```

**After:**
```rust
    #[test]
    fn change_type_maps_each_kind() {
        let (doc, raw) = doc_one_paragraph();
        let cases = [
            (
                CtrlChangeTarget { editor: "paragraph".into(), attrs: None },
                "paragraph",
                serde_json::Map::new(),
            ),
            (
                CtrlChangeTarget {
                    editor: "heading".into(),
                    attrs: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("level".into(), 2.into());
                        m
                    }),
                },
                "heading",
                {
                    let mut m = serde_json::Map::new();
                    m.insert("level".into(), 2.into());
                    m
                },
            ),
            (
                CtrlChangeTarget {
                    editor: "code".into(),
                    attrs: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("lang".into(), "rust".into());
                        m
                    }),
                },
                "code",
                {
                    let mut m = serde_json::Map::new();
                    m.insert("lang".into(), "rust".into());
                    m
                },
            ),
            (
                CtrlChangeTarget {
                    editor: "list".into(),
                    attrs: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("ordered".into(), true.into());
                        m
                    }),
                },
                "list",
                {
                    let mut m = serde_json::Map::new();
                    m.insert("ordered".into(), true.into());
                    m
                },
            ),
        ];
        for (target, expected_editor, expected_attrs) in cases {
            let ctrl = CtrlAction::ChangeType {
                block_id: raw,
                target: target.clone(),
            };
            match ctrl.into_block_action(&doc).expect("known id translates") {
                BlockAction::ChangeType { block_id, new_editor, new_attrs } => {
                    assert_eq!(block_id.raw(), raw);
                    assert_eq!(&*new_editor, expected_editor);
                    assert_eq!(*new_attrs, expected_attrs);
                }
                other => panic!("expected ChangeType, got {other:?}"),
            }
        }
    }

    #[test]
    fn change_type_heading_level_passes_through_attrs() {
        let (doc, raw) = doc_one_paragraph();
        let ctrl = CtrlAction::ChangeType {
            block_id: raw,
            target: CtrlChangeTarget {
                editor: "heading".into(),
                attrs: Some({
                    let mut m = serde_json::Map::new();
                    m.insert("level".into(), 9.into());
                    m
                }),
            },
        };
        match ctrl.into_block_action(&doc).expect("known id translates") {
            BlockAction::ChangeType { new_attrs, .. } => {
                // The clamping happens in apply_change_type via the descriptor's
                // default_block, not in the ctrl mapping. The attrs pass through
                // as-is; the descriptor's heading default_block uses level=1.
                // The actual clamping is done by the heading widget.
                assert_eq!(new_attrs.get("level").and_then(|v| v.as_u64()), Some(9));
            }
            other => panic!("expected ChangeType, got {other:?}"),
        }
    }
```

Also update the `block_id_accessor_returns_each_variant_id` test:

**Before:**
```rust
        assert_eq!(
            CtrlAction::ChangeType {
                block_id: 5,
                new_kind: CtrlBlockKind::Paragraph
            }
            .block_id(),
            5
        );
```

**After:**
```rust
        assert_eq!(
            CtrlAction::ChangeType {
                block_id: 5,
                target: CtrlChangeTarget {
                    editor: "paragraph".into(),
                    attrs: None,
                }
            }
            .block_id(),
            5
        );
```

- [ ] **Step 6: Delete `use crate::model::types::BlockKind` import** from ctrl/mod.rs (or keep it if other parts still use it — check the file; after this task, `BlockKind` is no longer imported here).

**Before:**
```rust
use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun};
```

**After:**
```rust
use crate::model::descriptor;
use crate::model::types::{BlockBody, BlockId, EditorBlock, EditorDoc, InlineRun};
```

- [ ] **Step 7: Run to verify they compile and pass**

Run: `cargo test -p lopress-editor --lib` (ctrl tests are in the lib)
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/ctrl/mod.rs
git commit -m "refactor(editor): re-point ctrl server off BlockKind, use CtrlChangeTarget { editor, attrs }"
```

---

## Task 6: Update remaining UI references to `BlockKind`

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/inline_editor.rs` (`should_commit` check)
- Modify: `crates/lopress-editor/src/ui/editing/pane_key.rs` (`KindTag` → `PaneTag`, `kind_tag` → `pane_tag`)
- Modify: `crates/lopress-editor/src/ui/inspector.rs` (title/H1 mismatch check)

**Goal:** Update all remaining `BlockKind` references in UI code to read from `PluginMeta.editor` and `PluginMeta.attrs`.

- [ ] **Step 1: Update `inline_editor.rs`** — the `should_commit` check:

**Before:**
```rust
        let should_commit = current_doc.with_untracked(|maybe| {
            maybe.as_ref().and_then(|doc| {
                doc.blocks
                    .iter()
                    .find(|b| b.id == block_id)
                    .map(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::Heading(_)))
            })
        });
```

**After:**
```rust
        let should_commit = current_doc.with_untracked(|maybe| {
            maybe.as_ref().and_then(|doc| {
                doc.blocks
                    .iter()
                    .find(|b| b.id == block_id)
                    .and_then(|b| b.plugin.as_ref())
                    .map(|m| {
                        m.editor.as_deref() == Some(descriptor::EDITOR_PARAGRAPH)
                            || m.editor.as_deref() == Some(descriptor::EDITOR_HEADING)
                    })
            })
        });
```

Also add `use crate::model::descriptor;` import.

**Before (import):**
```rust
use crate::model::types::{BlockId, BlockKind, EditorDoc, InlineRun};
```

**After (import):**
```rust
use crate::model::descriptor;
use crate::model::types::{BlockId, EditorDoc, InlineRun};
```

- [ ] **Step 2: Update `pane_key.rs`** — `KindTag` → `PaneTag`, `kind_tag` → `pane_tag`:

**Before:**
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum KindTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Image,
    Table,
    Opaque,
}

pub fn kind_tag(k: &BlockKind) -> KindTag {
    match k {
        BlockKind::Paragraph => KindTag::Paragraph,
        BlockKind::Heading(level) => KindTag::Heading(*level),
        BlockKind::Code { .. } => KindTag::Code,
        BlockKind::List { ordered } => KindTag::List { ordered: *ordered },
        BlockKind::Image => KindTag::Image,
        BlockKind::Table => KindTag::Table,
        BlockKind::Opaque { .. } => KindTag::Opaque,
    }
}
```

**After:**
```rust
/// Compact equality tag for editor-pane rebuild keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Image,
    Table,
    Opaque,
}

/// Derive a pane tag from a block's PluginMeta.
pub fn pane_tag(block: &crate::model::types::EditorBlock) -> PaneTag {
    let meta = block.plugin.as_ref().unwrap_or_else(|| {
        // Fallback for blocks without PluginMeta (shouldn't happen after Stage B,
        // but the type system must compile). Treat as opaque.
        &crate::model::types::PluginMeta {
            block_type_name: Rc::from("unknown"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: false,
            editor: None,
            native: None,
        }
    });
    match meta.editor.as_deref() {
        Some(descriptor::EDITOR_PARAGRAPH) => PaneTag::Paragraph,
        Some(descriptor::EDITOR_HEADING) => {
            let level = meta.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
            PaneTag::Heading(level.clamp(1, 6))
        }
        Some(descriptor::EDITOR_CODE) => PaneTag::Code,
        Some(descriptor::EDITOR_LIST) => {
            let ordered = meta.attrs.get("ordered").and_then(|v| v.as_bool()).unwrap_or(false);
            PaneTag::List { ordered }
        }
        Some(descriptor::EDITOR_IMAGE) => PaneTag::Image,
        Some(descriptor::EDITOR_TABLE) => PaneTag::Table,
        _ => PaneTag::Opaque,
    }
}
```

- [ ] **Step 3: Update the call site** in `editing_view` (wherever `kind_tag` is called):

**Before:**
```rust
d.blocks.iter().map(|b| (b.id, kind_tag(&b.kind), b.plugin.is_some()))
```

**After:**
```rust
d.blocks.iter().map(|b| (b.id, pane_tag(b), b.plugin.is_some()))
```

- [ ] **Step 4: Update `inspector.rs`** — the title/H1 mismatch check:

**Before:**
```rust
    let h1_text = create_memo(move |_| {
        current_doc.with(|maybe| {
            let d = maybe.as_ref()?;
            let h1 = d
                .blocks
                .iter()
                .find(|b| b.kind == crate::model::types::BlockKind::Heading(1))?;
            match &h1.body {
                crate::model::types::BlockBody::Inline(runs) => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                }
                _ => None,
            }
        })
    });
```

**After:**
```rust
    let h1_text = create_memo(move |_| {
        current_doc.with(|maybe| {
            let d = maybe.as_ref()?;
            let h1 = d
                .blocks
                .iter()
                .find(|b| {
                    b.plugin.as_ref().is_some_and(|m| {
                        m.editor.as_deref() == Some(descriptor::EDITOR_HEADING)
                            && m.attrs.get("level").and_then(|v| v.as_u64()) == Some(1)
                    })
                })?;
            match &h1.body {
                crate::model::types::BlockBody::Inline(runs) => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                }
                _ => None,
            }
        })
    });
```

Add `use crate::model::descriptor;` import.

**Before (import):**
```rust
use crate::model::types::EditorDoc;
```

**After (import):**
```rust
use crate::model::descriptor;
use crate::model::types::EditorDoc;
```

- [ ] **Step 5: Delete `use crate::model::types::BlockKind` imports** from files that no longer need them.

- [ ] **Step 6: Run to verify they compile**

Run: `cargo check -p lopress-editor`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/inline_editor.rs crates/lopress-editor/src/ui/editing/pane_key.rs crates/lopress-editor/src/ui/inspector.rs
git commit -m "refactor(editor): update remaining UI references off BlockKind"
```

---

## Task 7: Make `PluginMeta` non-optional on `EditorBlock` + update `opaque` constructor

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs` (`EditorBlock.plugin: Option<PluginMeta>` → `PluginMeta`, `EditorBlock::opaque` stamps identity meta)

**Goal:** `EditorBlock.plugin` is no longer `Option<PluginMeta>`. Every constructor stamps it. `EditorBlock::opaque` creates an identity meta with the unknown type name.

- [ ] **Step 1: Write the failing test** — append to the `mod tests` block in `types.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod opaque_identity_meta_tests {
    use super::*;

    #[test]
    fn opaque_block_carries_identity_meta() {
        let b = EditorBlock::opaque("lopress:video".to_string(), serde_json::json!({ "src": "a.mp4" }));
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "lopress:video");
        assert!(meta.editor.is_none());
        assert!(meta.native.is_none());
        assert!(!meta.builtin);
        assert!(meta.attr_decls.is_empty());
        assert!(meta.attrs.is_empty());
    }

    #[test]
    fn opaque_identity_meta_round_trips_to_core() {
        let b = EditorBlock::opaque("lopress:video".to_string(), serde_json::json!({ "src": "a.mp4" }));
        let core = crate::model::to_core::block_to_core(&b);
        assert_eq!(core.r#type, "lopress:video");
        assert!(matches!(core.text, None));
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor opaque_block_carries_identity_meta opaque_identity_meta_round_trips_to_core`
Expected: FAIL (compilation errors — `b.plugin` is now `&PluginMeta`, not `Option<&PluginMeta>`).

- [ ] **Step 3: Update `EditorBlock` struct:**

**Before:**
```rust
pub struct EditorBlock {
    pub id: BlockId,
    pub kind: BlockKind,
    pub body: BlockBody,
    pub plugin: Option<PluginMeta>,
}
```

**After:**
```rust
pub struct EditorBlock {
    pub id: BlockId,
    pub body: BlockBody,
    pub plugin: PluginMeta,
}
```

- [ ] **Step 4: Update `EditorBlock::opaque`** — stamp identity meta:

**Before:**
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

**After:**
```rust
    /// An opaque block: unknown or removed plugin type.
    ///
    /// Carries a `PluginMeta` with `editor: None` and `block_type_name` set
    /// to the unknown type, so `to_core` routes it through the opaque arm
    /// and round-trips verbatim. The body stashes the original JSON.
    pub fn opaque(type_name: String, value: Value) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Opaque(value),
            plugin: PluginMeta {
                block_type_name: Rc::from(type_name),
                attrs: serde_json::Map::new(),
                attr_decls: Rc::from([]),
                builtin: false,
                editor: None,
                native: None,
            },
        }
    }
```

- [ ] **Step 5: Update all other `EditorBlock` constructors** — remove `kind:` fields and change `plugin: Some(...)` to `plugin: ...`:

**`paragraph`:**
```rust
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Inline(runs),
            plugin: PluginMeta::paragraph(),
        }
    }
```

**`heading`:**
```rust
    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        let level = level.clamp(1, 6);
        Self {
            id: BlockId::new(),
            body: BlockBody::Inline(runs),
            plugin: PluginMeta::heading(level),
        }
    }
```

**`code`:**
```rust
    pub fn code(lang: String, text: String) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Code(text),
            plugin: PluginMeta::code(&lang),
        }
    }
```

**`list`:**
```rust
    pub fn list(ordered: bool, items: Vec<ListItem>) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::List(items),
            plugin: PluginMeta::list(ordered),
        }
    }
```

**`read_more`:**
```rust
    pub fn read_more() -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Inline(vec![]),
            plugin: PluginMeta::read_more(),
        }
    }
```

**`separator`:**
```rust
    pub fn separator() -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Inline(vec![]),
            plugin: PluginMeta::separator(),
        }
    }
```

**`table`:**
```rust
    pub fn table(data: TableData) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Table(data),
            plugin: PluginMeta::table(),
        }
    }
```

**`table_default`:** — same pattern, no `kind:` field.

**`image`:** — same pattern, no `kind:` field.

**`from_plugin_item`:** — update to not set `kind:`:
```rust
    pub fn from_plugin_item(item: &crate::model::inserter::PluginInserterItem) -> Self {
        Self {
            id: BlockId::new(),
            body: BlockBody::Inline(Vec::new()),
            plugin: PluginMeta {
                block_type_name: item.type_name.clone(),
                attrs: item.default_attrs.clone(),
                attr_decls: item.attr_decls.clone(),
                builtin: false,
                editor: None,
                native: None,
            },
        }
    }
```

- [ ] **Step 6: Run to verify they compile**

Run: `cargo check -p lopress-editor`
Expected: Many errors in other files that still reference `b.kind`, `b.plugin.as_ref()`, `block.kind`, etc. These will be fixed in Task 8.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs
git commit -m "refactor(editor): make PluginMeta non-optional on EditorBlock, opaque stamps identity meta"
```

---

## Task 8: Delete `BlockKind` + `EditorBlock.kind` + consistency test

**Files:**
- Delete: `BlockKind` enum from `types.rs`
- Modify: `crates/lopress-editor/src/model/descriptor.rs` (delete `blockkind_variants_align_with_descriptor_bodies` test)
- Modify: All remaining files that import or use `BlockKind`

**Goal:** The final deletion. Remove `BlockKind`, `EditorBlock.kind`, and the descriptor consistency test. Update all remaining references.

- [ ] **Step 1: Delete the `BlockKind` enum from `types.rs`:**

Delete the entire `pub enum BlockKind { ... }` definition.

- [ ] **Step 2: Delete the consistency test from `descriptor.rs`:**

Delete the `blockkind_variants_align_with_descriptor_bodies` test and its comment.

- [ ] **Step 3: Update `from_core.rs`** — the `plugin_block_from_core` function no longer returns `(kind, body)`:

**Before:**
```rust
    let editor = decl.editor.as_deref().unwrap_or("paragraph");
    let inner = b.children.first();
    let (kind, body) = match editor {
        "heading" => {
            let level = inner
                .and_then(|c| c.attrs.get("level").and_then(serde_json::Value::as_u64))
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            (
                BlockKind::Heading(level.clamp(1, 6)),
                BlockBody::Inline(parse_inline(text)),
            )
        }
        "code" => {
            let lang = inner
                .and_then(|c| c.attrs.get("lang").and_then(serde_json::Value::as_str))
                .unwrap_or("");
            let text = inner.and_then(|c| c.text.clone()).unwrap_or_default();
            (
                BlockKind::Code {
                    lang: Rc::from(lang),
                },
                BlockBody::Code(text),
            )
        }
        "list" => {
            let ordered = inner
                .and_then(|c| c.attrs.get("ordered").and_then(serde_json::Value::as_bool))
                .unwrap_or(false);
            let items = inner.map(list_items_from_block).unwrap_or_default();
            (BlockKind::List { ordered }, BlockBody::List(items))
        }
        _ => {
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            (BlockKind::Paragraph, BlockBody::Inline(parse_inline(text)))
        }
    };

    EditorBlock {
        id: BlockId::new(),
        kind,
        body,
        plugin: Some(plugin),
    }
```

**After:**
```rust
    let editor = decl.editor.as_deref().unwrap_or("paragraph");
    let inner = b.children.first();
    let body = match editor {
        "heading" => {
            let level = inner
                .and_then(|c| c.attrs.get("level").and_then(serde_json::Value::as_u64))
                .and_then(|n| u8::try_from(n).ok())
                .unwrap_or(1);
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            BlockBody::Inline(parse_inline(text))
        }
        "code" => {
            let lang = inner
                .and_then(|c| c.attrs.get("lang").and_then(serde_json::Value::as_str))
                .unwrap_or("");
            let text = inner.and_then(|c| c.text.clone()).unwrap_or_default();
            BlockBody::Code(text)
        }
        "list" => {
            let items = inner.map(list_items_from_block).unwrap_or_default();
            BlockBody::List(items)
        }
        _ => {
            let text = inner.and_then(|c| c.text.as_deref()).unwrap_or("");
            BlockBody::Inline(parse_inline(text))
        }
    };

    EditorBlock {
        id: BlockId::new(),
        body,
        plugin: Some(plugin),
    }
```

Also update `native_image_from_core` and `native_list_from_core` similarly — remove `kind:` fields.

- [ ] **Step 4: Update `to_core.rs`** — `block_to_core` no longer matches on `(&b.kind, &b.body)`:

**Before:**
```rust
    match (&b.kind, &b.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|_| Block {
                r#type: type_name.to_string(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            })
        }
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    }
```

**After:**
```rust
    // Fallback: blocks without PluginMeta (shouldn't happen after Stage B).
    // Treat as opaque.
    Block {
        r#type: "paragraph".into(),
        attrs: empty_attrs(),
        children: vec![],
        text: Some(String::new()),
    }
```

Also update `plugin_block_to_core` — remove the `(&b.kind, &b.body)` match:

**Before:**
```rust
    let inner = match (&b.kind, &b.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => Block {
            r#type: "heading".into(),
            attrs: json!({ "level": level }),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        (BlockKind::Code { lang }, BlockBody::Code(text)) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": &**lang }),
            children: vec![],
            text: Some(text.clone()),
        },
        (BlockKind::List { ordered }, BlockBody::List(items)) => Block {
            r#type: "list".into(),
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
        },
        _ => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(String::new()),
        },
    };
```

**After:**
```rust
    let inner = match &b.body {
        BlockBody::Inline(runs) => Block {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(serialize_inline(runs)),
        },
        BlockBody::Code(text) => Block {
            r#type: "code".into(),
            attrs: json!({ "lang": meta.attrs.get("lang").and_then(Value::as_str).unwrap_or("") }),
            children: vec![],
            text: Some(text.clone()),
        },
        BlockBody::List(items) => Block {
            r#type: "list".into(),
            attrs: json!({ "ordered": meta.attrs.get("ordered").and_then(Value::as_bool).unwrap_or(false) }),
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
        },
        BlockBody::Table(data) => Block {
            r#type: "table".into(),
            attrs: json!({ "align": data.align.iter().map(|a| Value::String(a.as_str().to_string())).collect::<Vec<_>>() }),
            children: data
                .rows
                .iter()
                .map(|row| Block {
                    r#type: "table_row".into(),
                    attrs: empty_attrs(),
                    children: row
                        .cells
                        .iter()
                        .map(|cell| Block {
                            r#type: "table_cell".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&cell.runs)),
                        })
                        .collect(),
                    text: None,
                })
                .collect(),
            text: None,
        },
        BlockBody::Opaque(value) => {
            serde_json::from_value::<Block>(value.clone()).unwrap_or_else(|| Block {
                r#type: meta.block_type_name.to_string(),
                attrs: empty_attrs(),
                children: vec![],
                text: None,
            })
        },
    };
```

- [ ] **Step 5: Update `mod.rs` (block_view)** — remove `block.kind` references:

**Before:**
```rust
pub fn block_view(block: &EditorBlock, dnd: DndState, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let kind = block.kind.clone();

    if block.plugin.is_some() {
        let plugin_view = plugin::plugin_block_view(block, env);
        return wrap_block(plugin_view, block_id, kind, dnd, env);
    }

    let body = match (&block.kind, &block.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env)
        }
        (BlockKind::Opaque { .. }, BlockBody::Opaque(_)) => {
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] block {:?}: kind/body mismatch ({:?} + {:?})",
                block_id, block.kind, block.body
            );
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    };

    wrap_block(body, block_id, kind, dnd, env)
}
```

**After:**
```rust
pub fn block_view(block: &EditorBlock, dnd: DndState, env: &BlockEnv) -> AnyView {
    let block_id = block.id;

    if block.plugin.builtin || block.plugin.editor.is_some() {
        let plugin_view = plugin::plugin_block_view(block, env);
        return wrap_block(plugin_view, block_id, dnd, env);
    }

    let body = match &block.body {
        BlockBody::Code(_) => {
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
        BlockBody::Opaque(_) => {
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!("[fallback] block {:?}: non-plugin block with body {:?}", block_id, block.body);
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    };

    wrap_block(body, block_id, dnd, env)
}
```

Also update `wrap_block` to not take `kind: BlockKind`:

**Before:**
```rust
fn wrap_block(
    body: AnyView,
    block_id: BlockId,
    kind: BlockKind,
    dnd: DndState,
    env: &BlockEnv,
) -> AnyView {
```

**After:**
```rust
fn wrap_block(
    body: AnyView,
    block_id: BlockId,
    dnd: DndState,
    env: &BlockEnv,
) -> AnyView {
```

- [ ] **Step 6: Update `plugin.rs` (render_body)** — **PRESERVE THE CONTAINER-PLUGIN RENDER FIX**.

The current `render_body` has `(BlockKind::Paragraph, BlockBody::Inline)` and `(BlockKind::Heading, BlockBody::Inline)` arms that render container plugins' bodies (those carry `editor: None`). When re-keying off `BlockKind`, the case **"plugin block with `editor: None` + Inline body → paragraph editor (and Heading level → heading editor)"** MUST be preserved by dispatching on body shape when there's no editor key.

**Before:**
```rust
fn render_body(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    use crate::ui::blocks::editor_registry::editor_for;

    if let Some(key) = block.plugin.as_ref().and_then(|m| m.editor.as_deref()) {
        if let Some(widget) = editor_for(key) {
            return widget(block, env);
        }
    }

    // Fallback: editor keys not yet in the registry.
    let block_id = block.id;
    match (&block.kind, &block.body) {
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code_editor::editable_code_view(text, lang, block_id, env).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
            list::editable_list_view(items, block_id, *ordered, env)
        }
        // Container plugins (e.g. `lopress:callout`) carry `editor: None` and a
        // Paragraph/Heading + Inline body, so they skip the `editor_for` path
        // above and land here. Render their body as an editable paragraph/
        // heading — NOT the fallback warning.
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            paragraph::render_paragraph_editable(runs, block_id, env).into_any()
        }
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            heading::render_heading_editable(*level, runs, block_id, env).into_any()
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] plugin block {:?}: kind/body mismatch ({:?} + {:?})",
                block.id, block.kind, block.body
            );
            crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    }
}
```

**After (re-keyed, preserving container-plugin fix):**
```rust
fn render_body(block: &EditorBlock, env: &BlockEnv) -> AnyView {
    use crate::ui::blocks::editor_registry::editor_for;

    // Registry path: a manifest `editor` key with a registered widget wins.
    if let Some(key) = block.plugin.editor.as_deref() {
        if let Some(widget) = editor_for(key) {
            return widget(block, env);
        }
    }

    // Container plugins (e.g. `lopress:callout`) carry `editor: None` and an
    // Inline body, so they skip the `editor_for` path above and land here.
    // Render their body as an editable paragraph or heading — NOT the fallback
    // warning. Dispatch on body shape when there's no editor key:
    // - Inline body → paragraph editor (or heading if attrs["level"] is present).
    // - Code/List body → use the corresponding editor.
    // - Opaque → fallback.
    let block_id = block.id;
    match &block.body {
        BlockBody::Inline(runs) => {
            // Check if this is a heading (attrs["level"] present) or paragraph.
            let level = block.plugin.attrs.get("level").and_then(|v| v.as_u64()).and_then(|n| u8::try_from(n).ok());
            match level {
                Some(l) => heading::render_heading_editable(l.clamp(1, 6), runs, block_id, env).into_any(),
                None => paragraph::render_paragraph_editable(runs, block_id, env).into_any(),
            }
        }
        BlockBody::Code(_) => code_editor::editable_code_view(
            "", // placeholder — real code path should be via editor_for
            &Rc::from(""),
            block_id,
            env,
        ).into_any(),
        BlockBody::List(items) => {
            let ordered = block.plugin.attrs.get("ordered").and_then(|v| v.as_bool()).unwrap_or(false);
            list::editable_list_view(items, block_id, ordered, env)
        }
        BlockBody::Opaque(_) => {
            crate::ui::blocks::fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    }
}
```

- [ ] **Step 7: Delete `use crate::model::types::BlockKind` imports** from all files that no longer need them.

- [ ] **Step 8: Run the full test suite**

Run: `cargo test -p lopress-editor`
Expected: Failures in test files that still assert on `block.kind`. These are fixed in Task 9.

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/model/descriptor.rs crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "refactor(editor): delete BlockKind enum, EditorBlock.kind, and descriptor consistency test"
```

---

## Task 9: Update test files off `BlockKind`

**Files:**
- Modify: `crates/lopress-editor/tests/actions_tests.rs` (all `new_kind: BlockKind::...` → `new_editor` + `attrs`)
- Modify: `crates/lopress-editor/tests/from_to_core_tests.rs` (all `matches!(b.kind, BlockKind::...)` → read from `plugin`)
- Modify: `crates/lopress-editor/tests/list_plugin_meta_tests.rs`
- Modify: `crates/lopress-editor/tests/model_types_tests.rs`
- Modify: `crates/lopress-editor/tests/plugin_block_tests.rs`
- Modify: `crates/lopress-editor/tests/slash_menu_tests.rs`
- Modify: `crates/lopress-editor/tests/undo_tests.rs`

**Goal:** Update all test assertions to read block identity from `PluginMeta.editor` + `PluginMeta.attrs` instead of `BlockKind`.

- [ ] **Step 1: Update `actions_tests.rs`** — replace every `new_kind: BlockKind::...` with `new_editor` + `attrs`:

Example transformation:
```rust
// Before:
BlockAction::ChangeType { block_id: id, new_kind: BlockKind::Heading(2) }

// After:
BlockAction::ChangeType {
    block_id: id,
    new_editor: Rc::from("heading"),
    new_attrs: Box::new({
        let mut m = serde_json::Map::new();
        m.insert("level".into(), 2.into());
        m
    }),
}
```

- [ ] **Step 2: Replace `matches!(b.kind, BlockKind::Heading(l))` assertions** with plugin reads:

```rust
// Before:
assert!(matches!(block.kind, BlockKind::Heading(2)));

// After:
assert_eq!(block.plugin.editor.as_deref(), Some("heading"));
assert_eq!(block.plugin.attrs.get("level").and_then(|v| v.as_u64()), Some(2));
```

- [ ] **Step 3: Update `from_to_core_tests.rs`** — same pattern.

- [ ] **Step 4: Update `list_plugin_meta_tests.rs`** — replace `matches!(block.kind, BlockKind::List { ordered: true })` with plugin reads.

- [ ] **Step 5: Update `model_types_tests.rs`** — replace `BlockKind::Paragraph` assertions.

- [ ] **Step 6: Update `plugin_block_tests.rs`** — replace `matches!(first.kind, BlockKind::Code { .. })` with plugin reads.

- [ ] **Step 7: Update `slash_menu_tests.rs`** — replace `SlashChoice::Kind(BlockKind::...)` with `SlashChoice::ChangeType { ... }`.

- [ ] **Step 8: Update `undo_tests.rs`** — replace `BlockKind::Paragraph` / `BlockKind::Heading(2)` assertions.

- [ ] **Step 9: Run the full test suite**

Run: `cargo test --workspace`
Expected: PASS (all tests updated).

- [ ] **Step 10: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS (formatting, clippy, tests all green).

- [ ] **Step 11: Commit any fmt-only changes**

```bash
git status --short
# Only if there are fmt changes to source files, stage those paths by name:
git add crates/lopress-editor/src crates/lopress-editor/tests
git commit -m "chore: fmt after BlockKind retirement"
```

---

## Task 10: Final gate + round-trip verification

**Files:** (no file changes — verification only)

- [ ] **Step 1: Run the full gate**

Run: `bash scripts/check.sh`
Expected: PASS (formatting, clippy `-D warnings`, all workspace tests).

- [ ] **Step 2: Verify round-trip specifically**

Run: `cargo test -p lopress-editor from_to_core`
Expected: PASS (all round-trip tests, including the new paragraph/heading native-path tests).

- [ ] **Step 3: Verify the safety net — all tests green**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 4: Final commit**

```bash
git add -A  # Only at this final step, after everything compiles and tests pass
git commit -m "refactor(editor): finalize Stage B — BlockKind retired, PluginMeta non-optional"
```

---

## Summary of tasks

| # | Task | Files |
|---|------|-------|
| 1 | Reshape `ChangeType` to `{ new_editor, attrs }` + update `apply_change_type` | `actions.rs` |
| 2 | Re-key `coerce_body_to_editor` / `body_matches_editor` + `apply_split` | `actions.rs` |
| 3 | Enrich descriptor `menu` + re-point slash menu | `descriptor.rs`, `slash_menu.rs` |
| 4 | Re-point toolbar off `BlockKind` | `toolbar.rs`, `mod.rs` |
| 5 | Update `ctrl/mod.rs` — `CtrlBlockKind` → `CtrlChangeTarget` | `ctrl/mod.rs` |
| 6 | Update remaining UI references | `inline_editor.rs`, `pane_key.rs`, `inspector.rs` |
| 7 | Make `PluginMeta` non-optional + update `opaque` constructor | `types.rs` |
| 8 | Delete `BlockKind` + `EditorBlock.kind` + consistency test | `types.rs`, `descriptor.rs`, `from_core.rs`, `to_core.rs`, `mod.rs`, `plugin.rs` |
| 9 | Update test files off `BlockKind` | `actions_tests.rs`, `from_to_core_tests.rs`, `list_plugin_meta_tests.rs`, `model_types_tests.rs`, `plugin_block_tests.rs`, `slash_menu_tests.rs`, `undo_tests.rs` |
| 10 | Final gate + round-trip verification | (no file changes) |

---

## Resolved decisions

- **`ChangeType` shape:** `ChangeType { block_id, new_editor: Rc<str>, new_attrs: Box<serde_json::Map<String, Value>> }`. Boxed to keep `BlockAction` within the 40-byte guard. `apply_change_type` swaps `PluginMeta` to the canonical meta for `new_editor` (via `descriptor_for` + `default_block`) merged with `attrs`, and coerces the body to that editor's `body_shape` preserving inline formatting. Code is the only lossy endpoint (plain text). The inverse snapshots the OLD editor + OLD attrs + OLD body, making `ChangeType` fully reversible.

- **ChangeType → heading default level:** `2`. The descriptor's heading `default_block` produces `EditorBlock::heading(1, vec![])`, but when the toolbar emits a `ChangeType` for a specific heading level (H1-H6), the attrs carry that level. The slash menu's "Heading 2" entry carries `attrs: {"level": 2}`.

- **Opaque identity meta:** An unknown/removed block gets `PluginMeta { block_type_name: <unknown type>, editor: None, native: None, attr_decls: [], builtin: false, attrs: {} }`. This round-trips through `to_core`'s opaque arm byte-identically (verified by the new test in Task 7).

- **`PluginMeta` non-optional:** `EditorBlock.plugin: PluginMeta`. Every constructor stamps it. The `block.plugin.as_ref()` / `if let Some(meta)` sites (dozens) collapse to direct field access `block.plugin`.

- **Menu enrichment:** `BlockDescriptor.menu: &'static [MenuEntry]` with `slash_label` / `toolbar_label` / `category` / `default_block`. Per-type: paragraph → 1 entry (slash "Paragraph", toolbar "P"); heading → 6 entries (H1-H3 both, H4-H6 toolbar-only); code → 1 (both); list → 2 (UL/OL, both); image/separator/table → 1 each (`toolbar_label: None`); more → 0 (not in menus). Projection tests pin the exact label sequences.

- **Action carrier post-BlockKind:** `SlashChoice::ChangeType { new_editor, attrs }` and `BlockAction::ChangeType { new_editor, new_attrs }`. The slash menu's `default_block` is the single source of truth — both the slash menu and toolbar derive their action data from it.

- **Container-plugin render fix preserved:** `render_body` in `plugin.rs` dispatches on body shape when `editor: None` (container plugins): Inline body → paragraph editor (or heading if `attrs["level"]` present), Code/List → corresponding editor, Opaque → fallback. This prevents the fallback-warning regression.

---

## Workspace audit confirmation

Audited the entire workspace for `BlockKind` references (`grep -rn "BlockKind" crates/ --include="*.rs"`). Found references in:

- **Production code (12 files):** `actions.rs`, `ctrl/mod.rs`, `from_core.rs`, `to_core.rs`, `types.rs`, `editor_registry.rs`, `inline_editor.rs`, `mod.rs` (blocks), `plugin.rs`, `pane_key.rs`, `inspector.rs`, `slash_menu.rs`, `toolbar.rs`
- **Test code (7 files):** `actions_tests.rs`, `from_to_core_tests.rs`, `list_plugin_meta_tests.rs`, `model_types_tests.rs`, `plugin_block_tests.rs`, `slash_menu_tests.rs`, `undo_tests.rs`
- **Descriptor consistency test:** `descriptor.rs` (the `blockkind_variants_align_with_descriptor_bodies` test)

The `lopress-core` parser's `CodeBlockKind` (from `pulldown-cmark`) is a different type and is NOT affected.

---

## Post-BlockKind action carrier resolution

**Decision:** `SlashChoice::Kind(BlockKind)` → `SlashChoice::ChangeType { new_editor: Rc<str>, attrs: serde_json::Map<String, Value> }`.

The slash menu's `default_block` closure is the single source of truth. Each `MenuEntry` carries its own `default_block: fn() -> EditorBlock`. When the user picks a slash menu item, we call `default_block()`, extract the `PluginMeta` from the result, and derive `new_editor` (`meta.editor`) + `attrs` (`meta.attrs`). The toolbar does the same — each button's `default_block` produces the exact editor + attrs for the `ChangeType` action.

This means `ChangeType` is fully self-contained: it carries the target editor key and the attrs to merge, and `apply_change_type` looks up the descriptor to validate and fill in any missing defaults. No separate `BlockKind` enum needed anywhere.

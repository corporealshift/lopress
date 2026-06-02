# Image Block Authoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Insert and edit images in the editor — a slash-menu "Image" entry that imports a file into `src/images/` via a native dialog and inserts a native `image` block with an editable alt + caption, displayed with an inline preview.

**Architecture:** A new `image` base plugin claims the native core `image` type (like `list`/`code`), giving the block a `PluginMeta` + a registered editor widget. Captions ride the markdown image *title* slot (`![alt](src "caption")`), which needs a small `lopress-core` parser/serializer change. Insertion reuses the read-more plan's `SlashChoice` slash-menu seam, adding an `Image` variant whose selection opens an `rfd` file dialog, copies the file into `src/images/`, and inserts the block.

**Tech Stack:** Rust, Floem (editor widget + image view), `rfd` (file dialog, already a dep), the workspace's strict clippy lints (`AGENTS.md`).

**Spec:** `docs/superpowers/specs/2026-06-01-image-block-design.md` (editor sections §3–§5; the build/render side is the separate `2026-06-01-responsive-image-rendering.md`).

> **DEPENDENCY:** This plan assumes the read-more plan's slash-menu generalization has landed — specifically `pub enum SlashChoice { Kind(BlockKind), ReadMore }` in `crates/lopress-editor/src/ui/slash_menu.rs` and the `slash_menu(items, on_select, on_close)` signature taking `SlashChoice`. If read-more is **not** yet implemented, create `SlashChoice` first per that plan's Task 8 (the `Kind`/`ReadMore` variants), then proceed. This plan adds an `Image` variant.

> **SOFT SPOT — Floem image view (Task 7):** there is **no existing usage of a Floem image view anywhere in this codebase** to copy. Do NOT fabricate a constructor. Before writing the preview, verify the exact API in the pinned Floem version: check `floem`'s version in `Cargo.lock`, then its docs/source for the image/img view (candidates: `floem::views::img(move || bytes)`, an `img_dynamic`, or a path-based image view). Implement the preview against the real API; if no usable image view exists in the pinned version, fall back to a bordered placeholder showing the filename and proceed (note it in the commit).

> **Gate:** run `bash scripts/check.sh` before declaring done.

---

## Task 1: Caption round-trip in lopress-core (markdown title slot)

**Files:**
- Modify: `crates/lopress-core/src/parser.rs` (`consume_inline` image arm)
- Modify: `crates/lopress-core/src/serializer.rs` (`image` arm)
- Test: `crates/lopress-core/tests/roundtrip.rs` and the in-file parser/serializer tests

- [ ] **Step 1: Write the failing tests**

In `crates/lopress-core/src/parser.rs` tests:

```rust
#[test]
fn parses_image_caption_from_title() {
    let d = parse("![alt](foo.jpg \"My caption\")\n").unwrap();
    assert_eq!(types(&d.blocks), vec!["image"]);
    assert_eq!(
        d.blocks[0].attrs,
        json!({ "src": "foo.jpg", "alt": "alt", "caption": "My caption" })
    );
}

#[test]
fn parses_image_without_title_has_no_caption() {
    let d = parse("![alt](foo.jpg)\n").unwrap();
    assert_eq!(d.blocks[0].attrs, json!({ "src": "foo.jpg", "alt": "alt" }));
}
```

In `crates/lopress-core/tests/roundtrip.rs`:

```rust
#[test]
fn image_with_caption_round_trips() {
    let src = "![alt](foo.jpg \"My caption\")\n";
    let doc = lopress_core::parse(src).unwrap();
    assert_eq!(lopress_core::serialize(&doc), src);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-core image_caption image_with_caption_round_trips parses_image_caption_from_title`
Expected: FAIL — the title is discarded on parse and never serialized.

- [ ] **Step 3: Capture the title on parse**

In `crates/lopress-core/src/parser.rs`, the `Event::Start(Tag::Image { dest_url, title: _, id: _, .. })` arm in `consume_inline` (around line 368): bind `title` and add it to attrs when non-empty:

```rust
Event::Start(Tag::Image {
    dest_url,
    title,
    id: _,
    ..
}) => {
    let src = dest_url.to_string();
    let caption = title.to_string();
    let mut alt = String::new();
    for inner in parser.by_ref() {
        match inner {
            Event::Text(t) => alt.push_str(&t),
            Event::End(TagEnd::Image) => break,
            _ => {}
        }
    }
    let mut attrs = serde_json::Map::new();
    attrs.insert("src".into(), serde_json::Value::String(src));
    attrs.insert("alt".into(), serde_json::Value::String(alt));
    if !caption.is_empty() {
        attrs.insert("caption".into(), serde_json::Value::String(caption));
    }
    only_image = Some(Block {
        r#type: "image".into(),
        attrs: serde_json::Value::Object(attrs),
        children: vec![],
        text: None,
    });
}
```

(Keeps the no-caption case byte-identical to today — `attrs` has only `src`/`alt`, matching the existing `parses_image_block_from_standalone_markdown_image` test. `serde_json` is already imported via the `json!` macro usage in this file.)

- [ ] **Step 4: Emit the title on serialize**

In `crates/lopress-core/src/serializer.rs`, the `"image" =>` arm (around line 128):

```rust
"image" => {
    let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
    let alt = b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or("");
    let caption = b.attrs.get("caption").and_then(|v| v.as_str()).unwrap_or("");
    if caption.is_empty() {
        let _ = writeln!(out, "![{alt}]({src})");
    } else {
        // Markdown image title is double-quoted; escape embedded quotes.
        let cap = caption.replace('"', "\\\"");
        let _ = writeln!(out, "![{alt}]({src} \"{cap}\")");
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p lopress-core image_caption image_with_caption_round_trips parses_image_caption_from_title parses_image_without_title_has_no_caption parses_image_block_from_standalone_markdown_image`
Expected: PASS (including the pre-existing no-caption test, unchanged).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/src/parser.rs crates/lopress-core/src/serializer.rs crates/lopress-core/tests/roundtrip.rs
git commit -m "feat(core): round-trip image captions via the markdown title slot"
```

---

## Task 2: The `image` base plugin

**Files:**
- Create: `base_plugins/image/manifest.toml`
- Modify: `crates/lopress-plugin/src/registry.rs` (`load_base_plugins`)
- Test: `crates/lopress-plugin/src/registry.rs`

- [ ] **Step 1: Create the manifest**

`base_plugins/image/manifest.toml`:

```toml
# Built-in "base" plugin: the image block, claiming the native core `image`
# type. Embedded at compile time via include_str! — see load_base_plugins.
name    = "lopress-image"
version = "0.1.0"

[[blocks]]
name    = "image"
editor  = "image"
native  = "image"
builtin = true

[blocks.attrs]
src     = { type = "string", required = true, ui = "hidden" }
alt     = { type = "string", ui = "text" }
caption = { type = "string", ui = "text" }
```

- [ ] **Step 2: Write the failing test**

In `crates/lopress-plugin/src/registry.rs` tests:

```rust
#[test]
fn base_plugins_include_image() {
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    let (_p, decl) = reg.native_block("image").expect("image native block");
    assert_eq!(decl.editor.as_deref(), Some("image"));
    assert_eq!(decl.native.as_deref(), Some("image"));
    assert!(decl.builtin);
}
```

(Use `native_block` since `image` claims the native type; mirror the existing list/code native-block test if one exists.)

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p lopress-plugin base_plugins_include_image`
Expected: FAIL — `image` not registered.

- [ ] **Step 4: Seed the base plugin**

In `load_base_plugins` (`crates/lopress-plugin/src/registry.rs`), add the `image` embed + insert alongside `list`/`code`/`more`, mirroring the existing pattern:

```rust
let image_src = include_str!("../../../base_plugins/image/manifest.toml");
let image = parse_manifest_str(image_src)?;
self.insert(LoadedPlugin {
    root: std::path::PathBuf::from("<embedded:image>"),
    manifest: image,
})?;
```

- [ ] **Step 5: Run it to verify it passes**

Run: `cargo test -p lopress-plugin base_plugins_include_image`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add base_plugins/image/manifest.toml crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): add the image base plugin (native image claim)"
```

---

## Task 3: `BlockKind::Image` + `EditorBlock::image` / `PluginMeta::image`

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs`
- Test: same file

- [ ] **Step 1: Write the failing test**

In `crates/lopress-editor/src/model/types.rs` tests:

```rust
#[cfg(test)]
mod image_ctor_tests {
    use super::*;

    #[test]
    fn image_block_carries_attrs_in_meta() {
        let b = EditorBlock::image("/images/p.jpg", "alt text", "");
        assert!(matches!(b.kind, BlockKind::Image));
        let meta = b.plugin.as_ref().unwrap();
        assert_eq!(&*meta.block_type_name, "image");
        assert_eq!(meta.editor.as_deref(), Some("image"));
        assert_eq!(meta.native.as_deref(), Some("image"));
        assert_eq!(meta.attrs.get("src").and_then(|v| v.as_str()), Some("/images/p.jpg"));
        assert_eq!(meta.attrs.get("alt").and_then(|v| v.as_str()), Some("alt text"));
        assert!(!meta.attrs.contains_key("caption"), "empty caption omitted");
        assert!(matches!(b.body, BlockBody::Opaque(serde_json::Value::Null)));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor image_block_carries_attrs_in_meta`
Expected: FAIL — `BlockKind::Image` / `EditorBlock::image` don't exist.

- [ ] **Step 3: Add the variant + constructors**

In `crates/lopress-editor/src/model/types.rs`, add to `BlockKind`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Code { lang: Rc<str> },
    List { ordered: bool },
    Image,
    Opaque { type_name: Rc<str> },
}
```

Add to `impl PluginMeta`:

```rust
/// The canonical `PluginMeta` for an image block. Native `image` claim,
/// built-in (chrome suppressed), edited via the `"image"` widget. `attrs`
/// carries `src` (+ optional `alt`/`caption`).
pub fn image(src: &str, alt: &str, caption: &str) -> Self {
    let mut attrs = serde_json::Map::new();
    attrs.insert("src".to_string(), Value::String(src.to_string()));
    if !alt.is_empty() {
        attrs.insert("alt".to_string(), Value::String(alt.to_string()));
    }
    if !caption.is_empty() {
        attrs.insert("caption".to_string(), Value::String(caption.to_string()));
    }
    Self {
        block_type_name: Rc::from("image"),
        attrs,
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("image")),
        native: Some(Rc::from("image")),
    }
}
```

Add to `impl EditorBlock`:

```rust
/// An image block. State (src/alt/caption) lives in `PluginMeta.attrs`; the
/// body is an empty Opaque placeholder (images have no editable text/children).
pub fn image(src: &str, alt: &str, caption: &str) -> Self {
    Self {
        id: BlockId::new(),
        kind: BlockKind::Image,
        body: BlockBody::Opaque(Value::Null),
        plugin: Some(PluginMeta::image(src, alt, caption)),
    }
}
```

- [ ] **Step 4: Fix non-exhaustive `BlockKind` matches**

Run: `cargo build -p lopress-editor`
Expected: compile errors at any `match` on `BlockKind` without a catch-all (e.g. possibly in `ui/toolbar.rs` block-type cycler, `pane_key.rs`, or `inspector.rs`). For each, add an `Image` arm that does the sensible thing:
- toolbar block-type cycler: skip/ignore `Image` (it isn't a cyclable text kind) — add `BlockKind::Image => { /* not cyclable */ }` or fold into the existing default.
- any discriminant/tag use (pane_key): no action needed if it already maps via `std::mem::discriminant`.
Fix until it compiles. (Matches in `actions.rs`, `to_core.rs`, `coerce_body_to_kind` already have catch-all `_` arms — confirm and leave them.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p lopress-editor image_block_carries_attrs_in_meta`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/ui/toolbar.rs crates/lopress-editor/src/ui/blocks/mod.rs
git commit -m "feat(editor): add BlockKind::Image and image block constructors"
```

(Stage whichever files Step 4 actually touched.)

---

## Task 4: `from_core` / `to_core` for the image block

**Files:**
- Modify: `crates/lopress-editor/src/model/from_core.rs` (`native_block_from_core`)
- Modify: `crates/lopress-editor/src/model/to_core.rs` (verify native image serialization)
- Test: `crates/lopress-editor/tests/from_to_core_tests.rs`

- [ ] **Step 1: Write the failing round-trip test**

In `crates/lopress-editor/tests/from_to_core_tests.rs`:

```rust
#[test]
fn image_block_round_trips_with_caption() {
    let mut reg = lopress_plugin::PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    let src = "![the alt](/images/p.jpg \"A caption\")\n";
    let core = lopress_core::parse(src).unwrap();
    let edoc = lopress_editor::model::from_core::doc_from_core(&core, &reg);
    // The image becomes a BlockKind::Image with attrs in PluginMeta.
    assert_eq!(edoc.blocks.len(), 1);
    let back = lopress_editor::model::to_core::doc_to_core(&edoc);
    assert_eq!(lopress_core::serialize(&back), src);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor image_block_round_trips_with_caption`
Expected: FAIL — `native_block_from_core` has no `image` arm, so the image becomes `Opaque` (and may not round-trip the caption attr through the editor model cleanly).

- [ ] **Step 3: Add the `from_core` image arm**

In `crates/lopress-editor/src/model/from_core.rs`, `native_block_from_core`'s match on `decl.editor.as_deref()`:

```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        Some("code") => native_code_from_core(b, decl),
        Some("image") => native_image_from_core(b, decl),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

Add the helper:

```rust
/// Build an image `EditorBlock` from a core `image` block. `src`/`alt`/`caption`
/// come from the core block's attrs and are stamped into `PluginMeta.attrs`;
/// the body is an empty Opaque placeholder.
fn native_image_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    EditorBlock {
        id: BlockId::new(),
        kind: BlockKind::Image,
        body: BlockBody::Opaque(serde_json::Value::Null),
        plugin: Some(PluginMeta {
            block_type_name: Rc::from(decl.name.as_str()),
            attrs: block_attrs_as_object(&b.attrs),
            attr_decls: Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>()),
            builtin: decl.builtin,
            editor: decl.editor.as_deref().map(Rc::from),
            native: decl.native.as_deref().map(Rc::from),
        }),
    }
}
```

(`block_attrs_as_object`, `BlockKind`, `BlockBody`, `PluginMeta`, `Rc` are already imported/defined in this file.)

- [ ] **Step 4: Verify `to_core` serialization**

In `crates/lopress-editor/src/model/to_core.rs`, the image block flows through `native_block_to_core(b, meta, "image")` (since `meta.native == Some("image")`). Its body is `Opaque(Null)`, which hits the `_ =>` arm:

```rust
_ => Block {
    r#type: core_type.to_string(),
    attrs: Value::Object(meta.attrs.clone()),
    children: vec![],
    text: None,
},
```

This emits `Block { type: "image", attrs: {src, alt, caption?} }` — exactly what the serializer's `image` arm consumes. **No change needed** if this arm already exists (it does). Confirm by reading the function; if the `_` arm differs, ensure image attrs pass through unchanged.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p lopress-editor image_block_round_trips_with_caption`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "feat(editor): classify and round-trip image blocks"
```

---

## Task 5: `Session::import_image` helper

**Files:**
- Modify: `crates/lopress-gui-host/src/session.rs`
- Test: `crates/lopress-gui-host/tests/session_integration.rs`

- [ ] **Step 1: Write the failing test**

In `crates/lopress-gui-host/tests/session_integration.rs` (use the existing temp-workspace harness; write a small source image file in a temp dir, open a session, call `import_image`):

```rust
// let session = Session::open(ws_root).unwrap();
// let web = session.import_image(&some_png_path).unwrap();
// assert!(web.starts_with("/images/"));
// assert!(ws_root.join("src/images").join(<filename>).exists());
// // Importing a colliding *different* file disambiguates with -1 suffix.
```

- [ ] **Step 2: Add the method**

In `impl Session` (`crates/lopress-gui-host/src/session.rs`):

```rust
/// Copy `src` into the workspace's `src/images/` and return its web path
/// (`/images/<filename>`). On a filename collision with different bytes, a
/// numeric suffix is appended; identical bytes reuse the existing file.
///
/// # Errors
/// Returns `SaveError` on I/O failure.
pub fn import_image(&self, src: &Path) -> Result<String, SaveError> {
    let images_dir = self.workspace.images_dir();
    std::fs::create_dir_all(&images_dir).map_err(SaveError::Io)?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("bin");
    let bytes = std::fs::read(src).map_err(SaveError::Io)?;

    // Find a non-colliding name; reuse if identical bytes already present.
    let mut filename = format!("{stem}.{ext}");
    let mut n: u32 = 1;
    loop {
        let candidate = images_dir.join(&filename);
        if !candidate.exists() {
            break;
        }
        if std::fs::read(&candidate).map(|b| b == bytes).unwrap_or(false) {
            return Ok(format!("/images/{filename}"));
        }
        filename = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    std::fs::write(images_dir.join(&filename), &bytes).map_err(SaveError::Io)?;
    Ok(format!("/images/{filename}"))
}
```

(Confirm the `SaveError::Io(std::io::Error)` variant — read `crates/lopress-gui-host/src/error.rs`; adapt the constructor if the variant differs. `Path` is already imported in session.rs.)

- [ ] **Step 3: Run the test**

Run: `cargo test -p lopress-gui-host import_image`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-gui-host/src/session.rs crates/lopress-gui-host/tests/session_integration.rs
git commit -m "feat(gui-host): import_image copies a file into src/images"
```

---

## Task 6: Slash-menu `Image` entry + editor-pane wiring

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (`SlashChoice::Image`)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (route `Image` to an injected callback)
- Test: `crates/lopress-editor/tests/slash_menu_tests.rs`

- [ ] **Step 1: Write the failing test**

In `crates/lopress-editor/tests/slash_menu_tests.rs`:

```rust
#[test]
fn slash_items_include_image() {
    let items = lopress_editor::ui::slash_menu::slash_menu_items();
    assert!(items.iter().any(|(label, choice)| *label == "Image"
        && matches!(choice, lopress_editor::ui::slash_menu::SlashChoice::Image)));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor slash_items_include_image`
Expected: FAIL — no `Image` variant.

- [ ] **Step 3: Add the variant + menu entry**

In `crates/lopress-editor/src/ui/slash_menu.rs`, extend `SlashChoice` (introduced by the read-more plan) and the items list:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
}

pub fn slash_menu_items() -> Vec<(&'static str, SlashChoice)> {
    vec![
        ("Paragraph", SlashChoice::Kind(BlockKind::Paragraph)),
        ("Heading 1", SlashChoice::Kind(BlockKind::Heading(1))),
        ("Heading 2", SlashChoice::Kind(BlockKind::Heading(2))),
        ("Heading 3", SlashChoice::Kind(BlockKind::Heading(3))),
        ("Code block", SlashChoice::Kind(BlockKind::Code { lang: Rc::from("") })),
        ("Unordered list", SlashChoice::Kind(BlockKind::List { ordered: false })),
        ("Ordered list", SlashChoice::Kind(BlockKind::List { ordered: true })),
        ("Image", SlashChoice::Image),
        ("Read more", SlashChoice::ReadMore),
    ]
}
```

- [ ] **Step 4: Route `Image` through an injected callback in editor_pane**

The import needs the session (file dialog + copy), which `editor_pane` doesn't hold. Add a callback parameter `on_insert_image: Rc<dyn Fn(BlockId)>` to `editor_pane` (the `BlockId` is the slash-menu anchor). In the slash overlay's `on_select`, route `SlashChoice::Image` to it:

```rust
let on_select = move |choice: crate::ui::slash_menu::SlashChoice| {
    match choice {
        crate::ui::slash_menu::SlashChoice::Kind(new_kind) => {
            on_action_for_select(BlockAction::ChangeType { block_id, new_kind });
        }
        crate::ui::slash_menu::SlashChoice::ReadMore => {
            on_action_for_select(BlockAction::InsertAfter {
                anchor: block_id,
                new_block: Box::new(crate::model::types::EditorBlock::read_more()),
            });
        }
        crate::ui::slash_menu::SlashChoice::Image => {
            (on_insert_image_for_select)(block_id);
        }
    }
};
```

Thread `on_insert_image` into `editor_pane`'s signature and clone it (`on_insert_image_for_select`) into the overlay closure, mirroring how `on_action` is cloned. Update the `editor_pane(...)` call in `crates/lopress-editor/src/ui/mod.rs` to pass the new callback (built in Task 8).

- [ ] **Step 5: Run the test**

Run: `cargo test -p lopress-editor slash_items_include_image`
Expected: PASS (the editor crate must still compile — Task 8 supplies the callback at the call site; if doing Task 6 before Task 8, temporarily pass `Rc::new(|_| {})` so it compiles, then replace in Task 8).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/tests/slash_menu_tests.rs
git commit -m "feat(editor): add the Image slash-menu entry routed to an import callback"
```

---

## Task 7: The `image` editor widget (preview + alt/caption)

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/image.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (`pub mod image;`)
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` (register `"image"`)
- Test: `crates/lopress-editor/src/ui/blocks/editor_registry.rs`

> **Read the SOFT SPOT note at the top of this plan first** — verify the Floem image-view API before writing the preview.

- [ ] **Step 1: Write the failing test**

In `editor_registry.rs` tests:

```rust
#[test]
fn editor_for_resolves_image() {
    assert!(editor_for("image").is_some());
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p lopress-editor editor_for_resolves_image`
Expected: FAIL.

- [ ] **Step 3: Create the widget**

`crates/lopress-editor/src/ui/blocks/image.rs`. It reads `src`/`alt`/`caption` from `ctx.block.plugin.attrs`, renders a preview (per the verified Floem image API) plus editable alt + caption fields that emit `BlockAction::EditAttrs`. Model the attr-edit emission on `ui/blocks/plugin.rs::attr_text` (text_input bound to an `RwSignal<String>`, committing on `FocusLost` by cloning the full attrs map, updating one key, and emitting `EditAttrs`). Skeleton:

```rust
//! The image block's editor widget: an inline preview plus editable alt and
//! caption fields. State lives in `PluginMeta.attrs`; edits emit EditAttrs.

use crate::actions::BlockAction;
use crate::ui::blocks::editor_registry::EditorContext;
use floem::reactive::{RwSignal, SignalGet};
use floem::views::{label, text_input, v_stack, Decorators};
use floem::{AnyView, IntoView};
use serde_json::Value;

pub fn image_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let meta = match ctx.block.plugin.as_ref() {
        Some(m) => m,
        None => return label(|| "(image: missing meta)".to_string()).into_any(),
    };
    let attrs = meta.attrs.clone();
    let src = attrs.get("src").and_then(Value::as_str).unwrap_or("").to_string();
    let alt = attrs.get("alt").and_then(Value::as_str).unwrap_or("").to_string();
    let caption = attrs.get("caption").and_then(Value::as_str).unwrap_or("").to_string();

    // PREVIEW: build per the verified Floem image-view API (see SOFT SPOT note).
    // The src is a web path `/images/<file>`; resolve it against the workspace
    // images dir on disk to load bytes for the preview. If no usable image view
    // exists in the pinned Floem, render a bordered placeholder with the filename.
    let preview: AnyView = build_image_preview(&src);

    // alt + caption fields: commit on FocusLost via EditAttrs, exactly like
    // attr_text in ui/blocks/plugin.rs.
    let alt_field = attr_field("alt", alt, attrs.clone(), block_id, ctx.on_action.clone());
    let caption_field = attr_field("caption", caption, attrs, block_id, ctx.on_action.clone());

    v_stack((
        preview,
        labeled("Alt", alt_field),
        labeled("Caption", caption_field),
    ))
    .style(|s| s.gap(4.).width_full())
    .into_any()
}
```

Implement `attr_field` by copying `attr_text`'s commit logic from `plugin.rs` (clone the attrs map, insert the edited key, emit `BlockAction::EditAttrs { block_id, new_attrs: Box::new(map) }` on `FocusLost`). Implement `labeled` as a simple `h_stack` of a label + the field. Implement `build_image_preview` against the verified Floem API.

- [ ] **Step 4: Register the module + key**

In `crates/lopress-editor/src/ui/blocks/mod.rs`: `pub mod image;`.
In `editor_registry.rs`: add `use crate::ui::blocks::image;` and the arm:

```rust
"image" => Some(image::image_widget),
```

- [ ] **Step 5: Run the test + build**

Run: `cargo test -p lopress-editor editor_for_resolves_image && cargo build -p lopress-editor`
Expected: PASS + compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/image.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "feat(editor): image block widget with preview and alt/caption fields"
```

---

## Task 8: Build the import callback and wire it into the editing view

**Files:**
- Modify: `crates/lopress-editor/src/ui/mod.rs`

- [ ] **Step 1: Build `on_insert_image`**

In `crates/lopress-editor/src/ui/mod.rs`, where `editing` (the `Rc<RefCell<EditingState>>` holding `session`) and `on_action` are in scope, build the callback and pass it to `editor_pane`:

```rust
let editing_for_image = Rc::clone(&editing);
let on_action_for_image = on_action.clone();
let on_insert_image: Rc<dyn Fn(BlockId)> = Rc::new(move |anchor: BlockId| {
    // Native file dialog (rfd) — same crate used by the workspace picker.
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
        .pick_file()
    else {
        return; // cancelled
    };
    let web = {
        let st = editing_for_image.borrow();
        st.session.import_image(&path)
    };
    match web {
        Ok(src) => {
            on_action_for_image(BlockAction::InsertAfter {
                anchor,
                new_block: Box::new(crate::model::types::EditorBlock::image(&src, "", "")),
            });
        }
        Err(e) => eprintln!("image import failed: {e}"),
    }
});
```

Pass `on_insert_image` into the `editor_pane::editor_pane(...)` call (added in Task 6). Note `editor_pane` is built inside a `dyn_container` rebuild closure — clone `on_insert_image` into that closure like `on_undo`/`on_redo` are cloned.

- [ ] **Step 2: Compile-check**

Run: `cargo build -p lopress-editor`
Expected: success. Confirm `rfd` is a dependency of `lopress-editor` (it is — used by `welcome.rs`).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/src/ui/mod.rs crates/lopress-editor/src/ui/editor_pane.rs
git commit -m "feat(editor): wire image import (file dialog + copy + insert)"
```

---

## Task 9: Full gate + end-to-end verification

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace` pass. Fix clippy per `AGENTS.md`. In particular, the new `BlockKind::Image` may surface non-exhaustive-match warnings elsewhere — resolve them (Task 3 Step 4).

- [ ] **Step 2: End-to-end (control interface)**

Via the `127.0.0.1:7878` control server, throwaway workspace under `$TEMP`:
- launch the editor (repo-root `cargo run`, visible window; poll `/ping`),
- `/open` an absolute path to a post,
- trigger image insertion via the slash menu (the native file dialog is real-mouse — hand back that portion per the control workflow; assert the rest), or pre-place an `![alt](/images/x.jpg "cap")` in the source and confirm it loads as an image block (preview renders, alt/caption populated),
- edit alt text, save, and confirm the saved markdown is `![new alt](/images/x.jpg "cap")` and the built page (with the responsive-rendering plan in place) shows a `<picture>` + `<figcaption>`.

Record verbatim commands + outputs; no PASS without them.

- [ ] **Step 3: Commit any gate fixes**

```bash
git add -A
git commit -m "chore: gate pass for image block authoring"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage:** caption round-trip (Task 1), image base plugin (Task 2), model + constructors (Task 3), classify/round-trip (Task 4), import helper (Task 5), slash entry + routing (Task 6), editor widget (Task 7), import wiring (Task 8), gate + e2e (Task 9).
- **Cross-plan dependencies:** requires read-more's `SlashChoice` seam (Task 6) and pairs with the responsive-rendering plan for the built-site `<picture>` (the e2e step assumes it). Both are independent code-wise; only the slash seam is a hard prerequisite.
- **Type consistency:** `BlockKind::Image` + `EditorBlock::image`/`PluginMeta::image` (Task 3) used by `from_core` (Task 4) and the import wiring (Task 8); `import_image` returns `/images/<file>` which `EditorBlock::image` stores as `src`; the `image` editor key (Task 7) matches the manifest (Task 2) and `PluginMeta.editor`.
- **Soft spot:** the Floem image-view API (Task 7) is the one unverified external API — explicitly flagged as verify-first with a placeholder fallback, not fabricated.
- **Caption escaping:** the serializer escapes `"` in captions (Task 1); a round-trip test covers a plain caption — extend with a quote-containing caption if desired.

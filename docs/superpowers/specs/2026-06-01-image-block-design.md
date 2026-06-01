# Image Block — Insert, Import & Responsive Rendering

**Date:** 2026-06-01
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (plugin capability model), `docs/architecture.md` (§5.4 lopress-assets, §8 build pipeline)

---

## 1. Background

lopress already has the pieces for images but they are not connected:

- The parser recognizes a core `image` block from standalone `![alt](src)` markdown
  (`Block { type: "image", attrs: { src, alt } }`).
- The static renderer (`crates/lopress-build/src/render.rs`) emits a **bare**
  `<img src alt>` for it.
- `lopress-assets` generates responsive WebP variants for everything in `src/images/`,
  named `{stem}.{width}w.{webp|ext}` (plus the original copied through as `{stem}.{ext}`),
  but this runs **after** rendering, so the renderer can't know which variants exist and
  emits no `srcset`.
- There is **no editor UI** to insert an image; the slash menu offers only
  paragraph/heading/code/list.

This spec makes "insert an image" a first-class editor action backed by a built-in
plugin block (like `code` and `list`), imports the chosen file into the workspace, and
renders a **responsive `<picture>`** on the built site using the existing WebP pipeline.

---

## 2. Scope

- A new base plugin block `image` claiming the native core `image` type, with attrs
  `src`, `alt`, and a new optional `caption`.
- A slash-menu "Image" entry that opens a native file dialog (`rfd`, already a
  dependency), copies the chosen file into `src/images/`, and inserts an image block
  pointing at `/images/<filename>`.
- An in-editor image block widget: inline preview + editable `alt` and `caption` fields.
- A build change: run image processing **before** page rendering, build an `ImageIndex`,
  thread it into `render_body`, and emit a responsive `<picture>` (WebP `srcset` +
  original `<img>` fallback), wrapped in `<figure>`/`<figcaption>` when captioned.

### Non-goals

- No image alignment/float/size controls (a future enhancement; render is full-width).
- No alt-text accessibility linting or required-alt enforcement.
- No external-URL image fetching/caching — an external `src` renders as a plain `<img>`
  (see §6).
- No gallery/multi-image block.
- No editing of the image bytes (crop/rotate) in the editor.

---

## 3. The `image` Base Plugin

A new embedded base plugin, mirroring `base_plugins/code`:

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

`native = "image"` is an **exclusive claim** on the core `image` type (the registry
already enforces single-claim per native type). `editor = "image"` selects the new
editor widget. `builtin = true` suppresses the generic plugin chrome so the image widget
renders its own UI. `load_base_plugins()` (`crates/lopress-editor/src/state.rs`) is
extended to seed this base plugin alongside `list` and `code`.

The core block remains `Block { type: "image", attrs: { src, alt, caption? }, … }`, so
existing `![alt](src)` markdown still parses into the same block and now carries an
optional `caption` attr.

---

## 4. Editor Model & Classification

### `BlockKind::Image`

A new variant `BlockKind::Image` is added to
`crates/lopress-editor/src/model/types.rs`. Image attributes (`src`, `alt`, `caption`)
live in `PluginMeta.attrs` — `BlockKind::Image` carries no inline payload. The body is
the empty placeholder `BlockBody::Opaque(Value::Null)` (an image block has no editable
text/children; all state is in attrs). `apply()` action arms that match on `BlockKind`
handle `Image` like other attr-only blocks (delete/move work; split/merge are no-ops on
it).

### `from_core`

`image` is a native-claiming type, so `block_from_core`'s `other` arm hits
`registry.native_block("image")` → `native_block_from_core`. A new arm
`Some("image") => native_image_from_core` builds:

```rust
EditorBlock {
    kind: BlockKind::Image,
    body: BlockBody::Opaque(Value::Null),
    plugin: Some(PluginMeta {
        block_type_name: "image",
        attrs: { src, alt, caption } from b.attrs,
        attr_decls: from decl,
        builtin: true,
        editor: Some("image"),
        native: Some("image"),
    }),
}
```

Attrs are read from the core block's `attrs` (`src`, `alt`, optional `caption`).

### `to_core`

A new branch in `block_to_core` (`crates/lopress-editor/src/model/to_core.rs`): a
native-claiming `image` block serializes to a core `Block { type: "image", attrs:
{ src, alt, caption? } }` from `PluginMeta.attrs` (omitting `caption` when empty). This
joins the existing native-serialization branch added for `list`/`code`.

---

## 5. Editor: Insertion, Import & Widget

### Slash-menu insertion of a plugin block

The slash menu (`crates/lopress-editor/src/ui/slash_menu.rs`) today offers only built-in
`BlockKind` conversions via `ChangeType`. An "Image" entry inserts a plugin block
instead. `slash_menu_items()` is extended from `(&str, BlockKind)` to a small item enum
that represents **either** a `BlockKind` conversion (existing) **or** a plugin-block
insertion identified by block name (`"image"`). A shared constructor
`new_plugin_block(registry, block_name) -> Option<EditorBlock>` builds the `EditorBlock`
(stamping `PluginMeta` with default attrs and the empty body), reusing the assembly that
`native_image_from_core` performs.

> **Shared infrastructure note.** This plugin-block insertion path and `new_plugin_block`
> are also required by the Read-More Marker spec
> (`2026-06-01-read-more-marker-design.md`). Whichever feature lands first builds this
> seam; the second reuses it. It is described self-containedly in both specs.

### File import

When "Image" is chosen, before inserting the block:

1. Open a native file-open dialog via `rfd` (already used by the workspace picker in
   `crates/lopress-editor/src/ui/welcome.rs`), filtered to common image extensions
   (`png`, `jpg`, `jpeg`, `gif`, `webp`).
2. If the user cancels, insert nothing (no-op).
3. Otherwise copy the chosen file into the workspace's `src/images/` directory
   (`Workspace::images_dir()`), creating the directory if needed. On a filename
   collision with a **different** file, disambiguate by appending a numeric suffix
   (`name-1.png`); if the same file is already present (same bytes), reuse it.
4. Insert an image block with `src = "/images/<final-filename>"`, empty `alt`, no
   `caption`.

The copy uses the session's workspace root. The import helper lives in
`lopress-gui-host` (it has the workspace/session) or is given the images dir by the
editor; the editor invokes it and receives the final `/images/<filename>` path. A failed
copy surfaces an inline error (reusing the editor's existing error-surface mechanism) and
inserts no block.

> **Note on saving:** copying the file into `src/images/` writes to disk immediately, so
> the imported asset exists before the document is saved. The image block referencing it
> is part of the normal save/undo flow. If the user undoes the insertion, the copied file
> is left in `src/images/` (harmless; the build only processes referenced + unreferenced
> images alike, and an orphan image just produces unused variants). Cleaning orphaned
> imports is out of scope.

### Image widget

A new editor widget registered under key `"image"` in
`crates/lopress-editor/src/ui/blocks/editor_registry.rs`. It renders:

- an **inline preview** of the image, loaded from the workspace by resolving
  `/images/<file>` against `Workspace::images_dir()` (Floem's image view);
- an editable **alt** text field;
- an editable **caption** text field;

Editing `alt`/`caption` emits `BlockAction::EditAttrs` with the updated attrs map (the
same mechanism the generic attr form uses). The `src` is fixed at insertion and not
editable inline in this spec (re-importing means inserting a new image). The widget reads
all values from `PluginMeta.attrs`.

---

## 6. Build: Responsive Rendering

### Reorder: process images before rendering

In `crates/lopress-build/src/build.rs`, the image pipeline currently runs **after**
`render_all`. It is moved **before** page rendering. As each source image is processed,
its `ImageResult` is collected into an in-memory index:

```rust
/// Maps an image source stem (filename without extension) to the variants
/// available under www/images/, so the renderer can emit a correct srcset.
pub struct ImageIndex {
    // stem -> { original: "stem.ext", webp_variants: [(width, "stem.800w.webp"), …] }
}
```

The index is keyed by stem because `variant_filename` names variants
`{stem}.{width}w.{ext}` and the original is `{stem}.{ext}`. The image pipeline keeps its
own per-file cache (`.lopress-image-cache.json`), so reordering does not re-encode
unchanged images; the index is rebuilt cheaply each build from the (possibly cached)
`process_image` results. The image cache is saved as before.

### Thread the index into rendering

`render_body` (and `render_excerpt` if present) gains an `&ImageIndex` parameter, threaded
from `render_all` → `render_one_post` / `render_one_page`. Custom-block template rendering
(`render_custom`) is unaffected; only the `image` arm consults the index.

### The `image` render arm

`write_block`'s `"image"` arm changes from a bare `<img>` to:

- Resolve the stem from `src`. The block `src` is `/images/<file>`; strip the
  `/images/` prefix and the extension to get the stem and look it up in the `ImageIndex`.
- **If found in the index** (a processed workspace image): emit a `<picture>`:

  ```html
  <figure>
    <picture>
      <source type="image/webp"
              srcset="/images/stem.400w.webp 400w, /images/stem.800w.webp 800w, …"
              sizes="(max-width: 800px) 100vw, 800px">
      <img src="/images/stem.ext" alt="{escaped alt}" loading="lazy">
    </picture>
    <figcaption>{escaped caption}</figcaption>   {# only when caption present #}
  </figure>
  ```

  The WebP `srcset` lists only the widths actually generated (variants wider than the
  source are skipped by `process_image`, so they won't appear in the index). The `<img>`
  fallback points at the copied original. `sizes` defaults to
  `(max-width: 800px) 100vw, 800px`. `<figcaption>` is emitted only when `caption` is
  non-empty. `alt`/`caption` are HTML-escaped via the existing `escape` helper.

- **If not found in the index** (an external URL, or a `src` that didn't go through the
  pipeline): fall back to a plain `<figure><img src alt loading="lazy"><figcaption?></figure>`
  — i.e. today's behavior plus the optional caption. No `srcset`.

`caption` is read from `b.attrs.get("caption")`.

---

## 7. Testing

### Core / round-trip

- A core `image` block with a `caption` attr round-trips through parse → serialize.
- Editor round-trip (`from_to_core_tests.rs`): an image block survives `from_core` →
  `to_core` with `src`/`alt`/`caption` preserved (base plugins loaded). An image with no
  caption omits the `caption` attr on the way back to core.

### Render

- With a populated `ImageIndex`, the `image` arm emits a `<picture>` whose `srcset` lists
  exactly the generated WebP widths and whose `<img>` fallback points at the original.
- A captioned image wraps in `<figure>`/`<figcaption>`; an uncaptioned one omits the
  `<figcaption>`.
- An image `src` not in the index falls back to a plain `<img>` (no `srcset`).
- `alt`/`caption` are escaped.

### Build integration

- A build over a fixture workspace with an image in `src/images/` produces the WebP
  variants **and** an index entry, and the rendered post HTML contains the matching
  `srcset`. (Extend `crates/lopress-build/tests/build_integration.rs`; a
  `fixtures/with-images` workspace already exists.)

### Editor / import

- `new_plugin_block(registry, "image")` builds a `BlockKind::Image` block with default
  attrs and `PluginMeta { editor: "image", native: "image", builtin: true }`.
- The import helper copies a file into `src/images/`, disambiguates a colliding
  different-bytes filename, and returns the `/images/<file>` path.
- Manifest parse of `base_plugins/image/manifest.toml`.

### End-to-end (control interface)

Via the `127.0.0.1:7878` control server: open a post, trigger image insertion (the file
dialog itself is real-mouse and may be handed back per the control workflow; the
post-selection state can be asserted), confirm the block renders a preview, edit the alt
text, save, and confirm the saved markdown carries the image with its `alt`/`caption` and
the built page emits a responsive `<picture>`.

---

## 8. Implementation Order

1. `base_plugins/image/manifest.toml`; seed in `load_base_plugins()`.
2. `BlockKind::Image` in `model/types.rs`; handle it in `apply()` action arms.
3. `from_core`: `native_image_from_core`; `to_core`: native `image` serialization branch.
4. Slash-menu plugin-block insertion + `new_plugin_block` (shared seam — §5).
5. Image import helper (copy into `src/images/`, disambiguate, return `/images/<file>`).
6. `editor_registry`: the `"image"` widget (preview + alt/caption fields).
7. `ImageIndex` + build reorder (process images before render); thread `&ImageIndex`
   through `render_all` → `render_one_*` → `render_body`.
8. `write_block` `image` arm: responsive `<picture>` + `<figure>`/`<figcaption>` +
   external-`src` fallback.
9. Tests (round-trip, render, build integration, import, manifest parse, e2e).

---

## 9. Decisions

### Image as a native base plugin claiming the core `image` type

Chosen for consistency with the block-types-as-plugins direction and so existing
`![alt](src)` markdown keeps parsing into the same block. Rejected a comment-container
`lopress:image` block — it would create a second, non-portable image representation
alongside the native one.

### Import-by-file-dialog into `src/images/`, `src = /images/<file>`

The author picks a file from anywhere; lopress owns the copy into the workspace so the
asset is version-controlled with the site and flows through the WebP pipeline. Rejected
referencing arbitrary absolute paths (not portable) and paste-a-URL-only (no pipeline,
no local asset). External URLs still render (plain `<img>`) for flexibility.

### Responsive `<picture>` via a build-time `ImageIndex`, requiring a build reorder

The renderer cannot emit a correct `srcset` without knowing which variants exist, and
variants depend on source dimensions (wider-than-source widths are skipped). Building an
index from `process_image` results and consulting it during render is the minimal correct
approach. Rejected emitting `srcset` for all configured widths blindly (would reference
nonexistent files for small images) and keeping a bare `<img>` (defeats the existing WebP
pipeline the user explicitly wants).

### `caption` as a new optional attr; full-width render only

Caption is the one piece of presentational metadata worth carrying now. Alignment/size
controls are deferred (YAGNI) — the block renders full-width inside `<figure>`.

### `BlockKind::Image` with an `Opaque(Null)` placeholder body

An image block has no editable text/children; all state is attrs. Reusing the existing
`Opaque` body avoids inventing an image-specific body variant, and the editor widget reads
everything from `PluginMeta.attrs`. The kind variant exists so `apply()` and dispatch can
recognize images explicitly.

---

## 10. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no placeholders.

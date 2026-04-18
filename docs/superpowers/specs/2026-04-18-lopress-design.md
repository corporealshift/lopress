# Lopress — Design

**Status:** draft
**Date:** 2026-04-18
**Scope:** v1 of lopress, a personal desktop blog-authoring tool with built-in static site generation.

## 1. Overview

Lopress is a desktop GUI authoring tool for personal blogs. It combines a Gutenberg-style block editor (built in Rust with egui) with a built-in static site generator. There is no database — the authoring state and the generated site are both the contents of a directory on disk.

You point lopress at a workspace directory. When the editor saves a file — or when `src/` changes from outside — lopress rebuilds `www/` into a static site of HTML, CSS, images, and optional JavaScript.

The tool is designed for the author's personal use first, with clean architectural boundaries so it could be open-sourced later without a rewrite.

### Goals

- Pleasant block-based editing, comparable in feel to WordPress's Gutenberg.
- Output is a plain static site — HTML, CSS, images, optional JS. No server required to host.
- Source of truth is markdown files on disk. Nothing lives in a database.
- Extensible via plugins that contribute block types, themes, and asset JS/CSS.
- Portable workspaces: a workspace directory is fully self-describing; cloning it gets a working site.

### Non-goals (v1)

- Multi-site management inside a single running instance.
- In-place WYSIWYG editing using the live theme's CSS (we use a separate webview preview pane instead).
- WASM plugin runtime (planned as a phase-2 escape hatch).
- Collaborative editing, authentication, comments.
- Mobile or web-based editor.

## 2. Workspace layout

```
<workspace>/
  lopress.toml       # site-level config
  src/
    posts/           # .md files, one per blog post
    pages/           # .md files for standalone pages (about, projects)
    images/          # source images, referenced from markdown
  plugins/           # site-local plugins (themes, block types)
  www/               # generated output — reproducible, gitignore-able
```

`lopress.toml` holds site-level settings:

```toml
[site]
title    = "Kyle's blog"
base_url = "https://example.com"
theme    = "default"

[site.nav]
items = [
  { label = "Home",     href = "/" },
  { label = "About",    href = "/about/" },
  { label = "Projects", href = "/projects/" },
]

[plugins]
enabled = ["lopress-video", "my-plugin"]

[build]
image_variants = [400, 800, 1600]  # px widths
```

## 3. Content format

### 3.1 Markdown with front-matter

Every post and page is a single markdown file. Markdown is the source of truth.

```markdown
---
title: My first post
slug: my-first-post        # optional; defaults to filename
date: 2026-04-18
tags: [rust, gui]
draft: false
description: Optional, used for OG meta and feed summary.
---

# My first post

Regular markdown for standard content.
```

Posts live in `src/posts/`. Pages live in `src/pages/`. The homepage is the reverse-chronological post index, rendered through the theme's `index.html` template. (A future version may allow a custom homepage via a dedicated page type; v1 keeps it simple.)

### 3.2 Block delimiter format

Custom blocks use HTML-comment delimiters — the Gutenberg wire format adapted for markdown:

```markdown
<!-- lopress:video {"src":"https://...","poster":"thumb.jpg"} -->
<!-- /lopress:video -->

<!-- lopress:callout {"kind":"warning"} -->
This callout contains **regular markdown** inside.
<!-- /lopress:callout -->

<!-- lopress:columns {"count":2} -->
  <!-- lopress:column -->
  Left column.
  <!-- /lopress:column -->
  <!-- lopress:column -->
  Right column.
  <!-- /lopress:column -->
<!-- /lopress:columns -->
```

Properties:

- Portable: opens cleanly in any markdown tool; custom blocks appear as invisible HTML comments.
- Grep-able: normal text search works.
- Nestable: delimiters nest naturally.
- Attribute-rich: JSON in the opening comment carries arbitrary structured config.
- Self-closing blocks (`<!-- lopress:video ... --><!-- /lopress:video -->` with no inner content) for atoms like video/canvas.

### 3.3 In-memory representation

```
Document {
  front_matter: FrontMatter,
  blocks: [Block],
}

Block {
  type:     String,        // "paragraph", "heading", "lopress:video", ...
  attrs:    JsonValue,
  children: [Block],       // inner block tree for containers
  text:     Option<String>,// raw inner text for text-like blocks
}
```

Standard markdown constructs (paragraph, heading, list, code fence, quote, image, link) map to built-in block types. Custom blocks from plugins map to `lopress:<name>` types.

**Parse and serialize are round-trip stable.** Parsing a file, then serializing the resulting tree, produces a byte-identical string back (modulo insignificant whitespace normalization defined by the serializer).

## 4. Architecture

Single Rust binary; internal crates in a cargo workspace:

```
lopress/
  crates/
    lopress-core/      # block tree, markdown parse/serialize, types
    lopress-build/     # static site generator: tree -> HTML files
    lopress-plugin/    # plugin manifest, loader, registry
    lopress-theme/     # template engine, theme resolution
    lopress-assets/    # image pipeline
    lopress-watch/     # fs watcher, debounced rebuild scheduler
    lopress-editor/    # egui GUI: editor, inspector, menus
    lopress-preview/   # webview host for the preview pane
  src/main.rs          # wires it all together, CLI entry
```

### 4.1 Dependency direction

- `lopress-core` has no dependency on GUI, filesystem, or rendering. Pure library.
- `lopress-build` depends on `-core`, `-theme`, `-plugin`, `-assets`. Usable as a CLI without the GUI.
- `lopress-editor` depends on everything else; nothing depends on it.
- `lopress-plugin` defines the trait surface between host and plugin — a small, stable API.

### 4.2 Data flow on save

```
Editor holds block tree in memory
  -> debounce (500ms) -> serialize to markdown -> write src/posts/foo.md
  -> fs watcher fires
  -> build scheduler debounces (200ms)
  -> lopress-build reads changed files, runs theme + plugins + assets
  -> writes www/
  -> preview webview reloads www/posts/foo/index.html
```

External edits (vim, git checkout, `cp`) take the same path starting from the fs watcher.

### 4.3 Build incrementality

The build cache at `www/.lopress-cache.json` records per-file source hashes and per-asset variant hashes. A rebuild regenerates only pages and image variants whose source or configuration changed. Full rebuilds are triggered only by:

- Missing cache.
- Theme change.
- `Rebuild All` menu command.

## 5. Plugins

### 5.1 Plugin directory layout

```
plugins/my-plugin/
  plugin.toml          # manifest — all block definitions live here
  blocks/
    video.html         # template (one per block declared in plugin.toml)
    # (phase-2) video.wasm, video.js — optional escape-hatch files
  assets/
    video.js           # bundled into www/assets/<plugin>/
    video.css          # linked from generated HTML when the block is used
  templates/           # (only if plugin.toml sets theme = true)
    layout.html
    post.html
    index.html
    page.html
    tag.html
  theme.css            # (only for theme plugins)
```

### 5.2 Manifest

```toml
name    = "my-plugin"
version = "0.1.0"
theme   = false                   # set true if plugin supplies templates

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
# optional phase-2 escape hatches:
# renderer = "blocks/video.wasm"
# editor   = "blocks/video.js"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
poster   = { type = "string", required = false, ui = "image-picker" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }
```

**Attribute UI kinds** (v1): `text`, `textarea`, `number`, `bool` / `checkbox`, `select`, `image-picker`, `color`. More kinds are added as blocks need them; the editor's declarative-block widget dispatches on this.

### 5.3 Editor side

The editor reads `blocks.attrs` and renders a sidebar form in the inspector pane. Declarative blocks require no plugin-specific editor code. Block inner content (for container blocks) is edited with the normal block-tree editor.

### 5.4 Build side

The build engine passes `(attrs, inner_html)` to the template engine, which renders the final HTML. Plugin `assets/` is copied to `www/assets/<plugin-name>/`; the build tracks which blocks are used on which pages and emits `<link>`/`<script>` tags in the page `<head>` only when the corresponding block appears.

### 5.5 Themes as plugins

Themes are plugins with `theme = true` and a `templates/` directory. Exactly one theme is active per site, declared in `lopress.toml`. A built-in default theme ships inside the lopress binary as a fallback when no theme plugin is active.

Theme templates use [Tera](https://keats.github.io/tera/) (Jinja-style syntax):

- `layout.html` — base layout with `<head>` and body shell. Other templates extend this.
- `post.html` — single post page.
- `page.html` — single page.
- `index.html` — homepage (post list).
- `tag.html` — single tag archive.

Template context provided by the build: site config, current page/post object, navigation, list of posts (for index/tag templates), rendered body HTML, computed meta (OG tags, canonical URL).

**Name collisions.** A built-in default theme named `default` ships with the binary. If a plugin with `name = "default"` exists in `<workspace>/plugins/` and has `theme = true`, it overrides the built-in theme. Any other name refers exclusively to a local plugin.

### 5.6 Phase-2 escape hatches

Not implemented in v1; reserved in the manifest:

- **WASM renderer** — `renderer = "blocks/video.wasm"` at the block level. The wasm module exports `render(attrs_json, inner_html) -> html`. Used for blocks whose output can't be expressed as a template (computation, complex transforms).
- **Custom editor UI** — `editor = "blocks/video.js"` at the block level. For blocks whose settings don't fit the declarative form schema. The editor would load the JS in an embedded webview panel inside the inspector.

Both are opt-in per block; declarative plugins remain untouched when these are added.

## 6. Build output

### 6.1 Output layout

```
www/
  index.html
  about/index.html
  projects/index.html
  posts/
    my-first-post/index.html
    rust-is-nice/index.html
  tags/
    rust/index.html
    gui/index.html
  images/
    foo.jpg
    foo.400w.webp
    foo.800w.webp
    foo.1600w.webp
  assets/
    my-plugin/video.js
    my-plugin/video.css
    theme.css
  feed.xml
  sitemap.xml
  robots.txt
  404.html
  .lopress-cache.json
```

### 6.2 URL shape

Flat URLs, no date prefix:

- Posts: `example.com/posts/<slug>/`
- Pages: `example.com/<slug>/`
- Tags: `example.com/tags/<tag>/`
- Feed: `example.com/feed.xml`
- Sitemap: `example.com/sitemap.xml`

Slug defaults to the filename (without extension); overridable by front-matter `slug:` field.

### 6.3 Image pipeline

The image block (and plain markdown `![]()`) reference a source path resolved relative to the containing `.md` file, with a fallback lookup in `src/images/` when the relative path doesn't resolve. The build:

1. Hashes the source.
2. Produces responsive variants at the widths configured in `[build].image_variants` (default `[400, 800, 1600]`).
3. Converts each variant to WebP (original format also preserved).
4. Emits a `<picture>` element with a `srcset` covering the variants.
5. Caches variants by `(source_hash, target_width, format)` in the build cache.

Unchanged images are never re-encoded. The image block exists to expose richer attribute controls (caption, alignment, focal point) that plain markdown `![]()` cannot express, but both use the same pipeline.

### 6.4 Meta generation

Every page emits OpenGraph and Twitter-card meta tags derived from:

- `title` — post/page title.
- `description` — front-matter `description` if present, else first paragraph as fallback.
- `og:image` — a site-level default from `lopress.toml`; per-post override via `image:` front-matter field.
- `canonical` — `{base_url}{path}`.

### 6.5 Drafts

Posts with `draft: true` in front-matter are skipped entirely by the build: no HTML, not in feed, not in sitemap, not in tag archives.

### 6.6 Feed, sitemap, robots, 404

- `feed.xml` — RSS/Atom feed of non-draft posts in reverse chronological order.
- `sitemap.xml` — posts, pages, and tag archive URLs with lastmod from file mtime.
- `robots.txt` — permissive by default; site owners can override content via `[robots]` in `lopress.toml`.
- `404.html` — rendered through the active theme's `layout.html` with a dedicated 404 body.

## 7. Editor UX

### 7.1 Window layout

Two-pane canvas with a right-hand inspector:

```
+--- Menu bar -----------------------------------------------+
| File  Site  Edit  View  Plugins  Help                      |
+------------------------------------------------------------+
| [Post ▾ My first post]   Saved · Draft ☐       Build: OK   |
+---------------------+-------------------+------------------+
|                     |                   |                  |
| Block editor        | Live preview      | Inspector        |
| (egui)              | (webview)         | (egui)           |
|                     |                   |                  |
| ¶ paragraph         |                   |  -- BLOCK --     |
| H2 heading          | [rendered post    |  (attrs of the   |
| 🖼 image             |  via active theme]|   selected block)|
| ¶ paragraph         |                   |                  |
|                     |                   |  -- POST --      |
| + Add block         |                   |  title, slug,    |
|                     |                   |  tags, draft,    |
|                     |                   |  date, excerpt   |
+---------------------+-------------------+------------------+
```

### 7.2 Menu bar

- **File**
  - *New Site…* — wizard that creates a workspace directory (`lopress.toml`, `src/posts/`, `src/pages/`, `src/images/`, `plugins/`, a hello-world post, initializes git).
  - *Open Site…* — directory picker.
  - *Open Recent ▸*
  - *Close Site*
  - *Save* (Ctrl/Cmd-S) — force-flush the debounced save.
  - *Quit*
- **Site**
  - *New Post* — creates `src/posts/<slug>.md` with minimal front-matter and opens it.
  - *New Page* — creates `src/pages/<slug>.md` and opens it.
  - *Rebuild All* — clears the cache and builds from scratch.
  - *Open www in Browser* — opens `www/index.html` in the default browser.
  - *Site Settings…* — editor for `lopress.toml` fields (title, base_url, theme, nav, enabled plugins).
- **Edit**
  - *Undo* / *Redo* (Ctrl/Cmd-Z, Shift-Ctrl/Cmd-Z) — operates on the block tree.
  - *Cut* / *Copy* / *Paste* — context-aware: block-level when a block is selected, text-level inside a text block.
  - *Find in Post* (Ctrl/Cmd-F).
- **View**
  - *Toggle Inspector*
  - *Toggle Preview*
  - *Refresh Preview*
- **Plugins**
  - *Manage Plugins…* — lists plugins found in `<workspace>/plugins/` with an enable/disable toggle (writes to `lopress.toml`).
  - *Reveal Plugins Folder*
  - *Reload Plugins* — re-scans manifests without restarting.
- **Help**
  - *About*
  - *Documentation*

### 7.3 Post switcher

Title bar dropdown showing all posts and pages; typing narrows the list. Ctrl/Cmd-P opens it from anywhere. "New post" and "New page" also appear at the top.

### 7.4 Block editor (left pane)

Vertical list of block widgets. Each block widget implements a small `BlockEditor` trait with a default implementation for text-like blocks (paragraph, heading, list item, quote). Declarative blocks from plugins use a built-in "declarative block" widget that:

- Renders the block's inner content with the normal block-tree editor (for containers).
- Renders the block's attributes as a form in the inspector (not inline) so the editor surface stays uncluttered.

Keyboard behavior:

- `/` at the start of a line opens the **block inserter**: fuzzy search over registered block types.
- `Enter` on a paragraph splits into two paragraphs.
- `Backspace` at the start of an empty block removes it and merges into the previous.
- `Alt+Up/Down` moves the selected block.
- Drag handle on the left of each block for reordering.

### 7.5 Preview (middle pane)

Hosts a webview pointed at `file://<workspace>/www/posts/<current-slug>/index.html` (or the appropriate page/index URL). After each successful build, the webview reloads. Preserving scroll position across reloads is a nice-to-have (webviews reload to the top by default; achievable with a small injected script) but is not a v1 must-have.

### 7.6 Inspector (right pane)

Two collapsible sections:

1. **Block** — attributes of the selected block, rendered from the block's `attrs` schema. Changes flow back into the in-memory block tree live.
2. **Post** — front-matter fields: title, slug, date, tags (chip input), draft toggle, description.

### 7.7 Image picker

When a block attribute has `ui = "image-picker"`, the inspector shows a thumbnail grid sourced from `src/images/` plus a drop target that copies a new image into `src/images/` and references it.

### 7.8 Save model

The editor holds the authoritative in-memory block tree for the open post. Edits debounce at 500ms; when the debounce fires, lopress serializes the tree to markdown and writes the file. The fs watcher sees the write and triggers a build.

External edits to the currently-open file are detected via a file-modtime check on next interaction — we prompt to reload (discarding in-memory edits) rather than attempting a three-way merge.

### 7.9 Workspace and plugin model

One site open at a time. Plugins live inside the workspace at `<workspace>/plugins/` — per-site, committed to the site's repo. Workspace is fully self-describing: cloning the workspace gets a working site.

## 8. Error handling

Principle: errors in one post or one plugin never block the rest of the site from building. Partial success is the norm.

- **Malformed markdown / unterminated block delimiters** — parser returns a diagnostic (line, column, reason). Build skips the page and surfaces the error in the title-bar build-status indicator, clickable to jump to the file and line. The editor always produces valid markdown, so the parser's forgiving mode handles external edits gracefully.
- **Template render errors** — build fails for that page with the engine's error; other pages continue. Build status lists failed pages.
- **Plugin manifest errors** — plugin fails to load with a specific reason; *Manage Plugins* shows the error next to the plugin entry. Blocks from a failed plugin render as a visible placeholder in the editor (`"unknown block: lopress:video"`) and as an HTML comment in the output (so content is not lost).
- **Image pipeline errors** — variant falls back to the original; build status reports the warning.
- **Write failures on save** — modal error in the editor; edits remain in-memory until the cause is resolved.

## 9. Testing strategy

- **`lopress-core`** — unit tests. Golden-file tests for parse/serialize round-trips. Proptest fuzz target: generate random block trees, serialize, reparse, assert equality.
- **`lopress-build`** — integration tests. Fixtures under `tests/fixtures/` are small complete workspaces; each test runs `build(fixture) -> www/` and diffs against `tests/expected/<fixture>/`. Covers: bare site, site with custom theme, site with custom block plugin, site with draft posts, site with images (variant generation, cache hits).
- **`lopress-plugin`** — unit tests on manifest parsing and plugin registry, with fixture plugins under `crates/lopress-plugin/tests/plugins/`.
- **`lopress-assets`** — unit tests on the image pipeline: cache hits by hash, variant dimensions, format conversion.
- **`lopress-watch`** — integration tests with `tempdir` + synthetic writes; verifies debounce coalesces rapid writes and triggers the build exactly once per window.
- **`lopress-editor`** — minimal automated tests. egui does not lend itself to UI automation; keep editor-internal logic (block tree operations, undo/redo) in pure functions under `lopress-core` to maximize test coverage, and rely on manual testing for GUI behavior. Later: consider `egui_kittest` or similar snapshot tooling.
- **CI** — `cargo test` on Linux, macOS, Windows. `cargo fmt --check`. `cargo clippy -- -D warnings`.

Non-goals (v1): end-to-end GUI automation, visual regression of rendered pages, performance benchmarks.

## 10. Open questions (to decide during implementation)

These were not resolved in the design phase and should be addressed during implementation planning:

- Webview crate choice (`wry` is the leading candidate; depends on deployment-story tradeoffs on Linux).
- Markdown parser choice (`pulldown-cmark` for CommonMark conformance, plus a custom pre-pass for the HTML-comment block delimiters).
- Default theme visual design — out of scope here; handled in a separate design pass.
- Undo/redo granularity — character-level within text blocks vs block-level ops only.

# Lopress Architecture Specification

> **Date:** 2026-05-28 (updated 2026-07-03 for the descriptor table / everything-is-a-plugin refactor)
> **Author:** Auto-generated from codebase audit
> **Purpose:** Reference document describing modules, data flows, plugin architecture, and how the system works end-to-end.
> **Staleness note:** this document is a snapshot. When a detail matters (enum variants, function signatures, JSON shapes), confirm it against the source file it names before relying on it.

---

## Table of Contents

1. [High-Level Overview](#1-high-level-overview)
2. [Workspace Layout](#2-workspace-layout)
3. [Crate Inventory](#3-crate-inventory)
4. [Data Flow: The Three-Layer Pipeline](#4-data-flow-the-three-layer-pipeline)
5. [Module Deep-Dive](#5-module-deep-dive)
   - 5.1 [lopress-core](#51-lopress-core)
   - 5.2 [lopress-plugin](#52-lopress-plugin)
   - 5.3 [lopress-theme](#53-lopress-theme)
   - 5.4 [lopress-assets](#54-lopress-assets)
   - 5.5 [lopress-build](#55-lopress-build)
   - 5.6 [lopress-watch](#56-lopress-watch)
   - 5.7 [lopress-serve](#57-lopress-serve)
   - 5.8 [lopress-gui-host](#58-lopress-gui-host)
   - 5.9 [lopress-editor](#59-lopress-editor)
6. [Plugin Architecture](#6-plugin-architecture)
   - 6.1 [Manifest Format](#61-manifest-format)
   - 6.2 [Block Declaration Types](#62-block-declaration-types)
   - 6.3 [Registry Lifecycle](#63-registry-lifecycle)
   - 6.4 [Native vs Comment-Container Blocks](#64-native-vs-comment-container-blocks)
   - 6.5 [Plugin Loading Order](#65-plugin-loading-order)
7. [Editor Architecture](#7-editor-architecture)
   - 7.1 [Block Model](#71-block-model)
   - 7.2 [Action System](#72-action-system)
   - 7.3 [Undo/Redo](#73-undoredo)
   - 7.4 [UI Components](#74-ui-components)
   - 7.5 [Save Pipeline](#75-save-pipeline)
8. [Build Pipeline](#8-build-pipeline)
   - 8.1 [Discovery](#81-discovery)
   - 8.2 [Rendering](#82-rendering)
   - 8.3 [Caching](#83-caching)
   - 8.4 [Aggregates](#84-aggregates)
9. [Theme System](#9-theme-system)
10. [Live Preview](#10-live-preview)
11. [File Index](#11-file-index)

---

## 1. High-Level Overview

Lopress is a **desktop blog-authoring tool** with a Gutenberg-style block editor and a built-in static site generator, written in Rust. It is a single binary that combines:

- **A GUI editor** (built on [Floem](https://floem.dev), an egui-adjacent reactive GUI framework)
- **A static site generator** (using Tera templates + custom block rendering)
- **A live preview server** (HTTP server with SSE-based browser reload)

All three live in the same process. There is no database, no external server required to host output — the source of truth is markdown on disk.

### Key design principles

- **Source-first:** Markdown files on disk are the single source of truth. The editor is a rich interface for creating and editing them.
- **Portability:** Custom blocks use the [Gutenberg HTML-comment delimiter convention](https://developer.wordpress.org/block-editor/developers/filters/filters/#the-html-comment-delimiter), so the markdown remains readable in any other tool.
- **Extensibility:** Plugins contribute block types, themes, and asset JS/CSS via a TOML manifest.
- **Incremental builds:** A file-system watcher triggers debounced rebuilds keyed on source hashes.

---

## 2. Workspace Layout

A lopress workspace is a directory containing:

```
<workspace>/
  lopress.toml          # Site config: title, base_url, theme, plugins
  nav.toml              # Navigation links (label + href); editable in the GUI
  src/
    posts/              # .md files — one per blog post
    pages/              # .md files — standalone pages
    images/             # Source images (auto-resized to WebP)
  plugins/              # Site-local plugins (themes, custom blocks)
  www/                  # Generated static site (gitignore-able)
```

The `lopress.toml` is the workspace root config. `nav.toml` holds the site
navigation as an `items` array of `{ label, href }` entries — it is the only
nav source (a legacy `[site.nav]` block in `lopress.toml` is ignored, with a
build warning) and is edited from the editor's "Site settings" panel.
`src/posts/` and `src/pages/` contain the content. `plugins/` holds user
plugins. `www/` is the output directory.

---

## 3. Crate Inventory

The workspace has 9 member crates (plus the root `lopress` binary package, whose entry point is `src/main.rs`):

| Crate | Role | Dependencies |
|-------|------|-------------|
| `lopress-core` | Markdown parser + serializer + document types | `pulldown-cmark`, `serde_json`, `chrono`, `serde_yaml` |
| `lopress-plugin` | Plugin manifest parsing + registry | `toml`, `serde_json` |
| `lopress-theme` | Theme engine + template context | `tera`, `lopress-core` |
| `lopress-assets` | Image processing (WebP variants) | `image`, `webp` |
| `lopress-build` | Static site generation pipeline | `lopress-core`, `lopress-plugin`, `lopress-theme`, `lopress-assets`, `walkdir`, `tera` |
| `lopress-watch` | File-system watcher | `notify` |
| `lopress-serve` | HTTP preview server + SSE | hand-rolled on `std::net::TcpListener` (no web framework), `lopress-build`, `lopress-watch` |
| `lopress-gui-host` | Session management, doc I/O, workspace scanning | `lopress-build`, `lopress-core`, `lopress-serve`, `lopress-watch`, `lopress-plugin` |
| `lopress-editor` | Block editor UI + action system | `floem`, `lopress-core`, `lopress-plugin`, `lopress-gui-host`, `lapce-xi-rope` |
| `lopress` (binary) | CLI entry point + GUI launch | All above |

**Dependency graph:**

```
lopress (binary)
  ├── lopress-editor (GUI)
  │     ├── lopress-core
  │     ├── lopress-plugin
  │     └── lopress-gui-host
  ├── lopress-gui-host
  │     ├── lopress-build
  │     │     ├── lopress-core
  │     │     ├── lopress-plugin
  │     │     ├── lopress-theme
  │     │     └── lopress-assets
  │     ├── lopress-serve
  │     │     ├── lopress-build
  │     │     └── lopress-watch
  │     ├── lopress-watch
  │     └── lopress-plugin
  ├── lopress-build
  ├── lopress-serve
  └── lopress-watch
```

---

## 4. Data Flow: The Three-Layer Pipeline

Lopress has three distinct data layers, each with its own representation:

### Layer 1: On-Disk Markdown

Raw `.md` files with YAML front-matter. Custom blocks use HTML comments:

```markdown
---
title: My Post
date: 2026-05-28
---

A paragraph with **bold** text.

<!-- lopress:video {"src":"talk.mp4"} -->
<!-- /lopress:video -->
```

### Layer 2: Core Document (`lopress_core::Document`)

The parsed representation — a flat tree of `Block` nodes. This is the **interchange format** between parser and serializer.

```rust
struct Document {
    front_matter: FrontMatter,
    blocks: Vec<Block>,
}

struct Block {
    r#type: String,       // "paragraph", "heading", "lopress:video", etc.
    attrs: Value,          // JSON attributes: {"level": 2}, {"src": "..."}
    children: Vec<Block>,  // Nested blocks (containers)
    text: Option<String>,  // Inline text (paragraphs, headings, code)
}
```

### Layer 3: Editor Document (`lopress_editor::EditorDoc`)

The editor's working model — a richer representation optimized for live editing.

```rust
struct EditorDoc {
    front_matter: lopress_core::FrontMatter,
    blocks: Vec<EditorBlock>,
}

struct EditorBlock {
    id: BlockId,        // Stable identity (not persisted)
    body: BlockBody,    // Enum: Inline(Vec<InlineRun>), Code(String), List(Vec<ListItem>), Table(TableData), Opaque(Value)
    plugin: PluginMeta, // Always present — every block (including paragraph/heading) is a plugin block
}

struct InlineRun {
    text: String,
    bold: bool, italic: bool, code: bool, link: Option<String>,
}
```

There is no `BlockKind` enum anymore ("everything is a plugin"): a block's type
identity is its `PluginMeta` (`block_type_name`, `editor` key, `native` claim),
and the data facts for each built-in type live in the **block descriptor table**
(`model/descriptor.rs`) keyed by the `editor` string (`"paragraph"`, `"heading"`,
`"code"`, `"list"`, `"image"`, `"table"`, `"separator"`, `"more"`).

### Conversion Pipeline

```
.on-disk markdown
       │
       ▼  (lopress_core::parse)
Core Document (lopress_core::Document)
       │
       ▼  (lopress_editor::model::from_core::doc_from_core)
Editor Document (lopress_editor::EditorDoc)
       │  ← user edits via GUI
       ▼  (lopress_editor::model::to_core::doc_to_core)
Core Document (lopress_core::Document)
       │
       ▼  (lopress_core::serialize)
.on-disk markdown
```

The `from_core` / `to_core` pair forms a **loss-free round-trip** for the supported subset and a **verbatim round-trip** for `Opaque` blocks (unknown block types whose original JSON is stashed in the body).

---

## 5. Module Deep-Dive

### 5.1 lopress-core

**Location:** `crates/lopress-core/`

**Responsibility:** Markdown parsing, document types, and serialization.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `parser.rs` | Parses markdown into `Document`. Uses `pulldown-cmark` for standard markdown. Custom blocks are handled by a delimiter scanner that finds `<!-- lopress:NAME ... -->` / `<!-- /lopress:NAME -->` pairs and treats the content between them as nested markdown. |
| `serializer.rs` | Renders `Document` back to markdown. Handles all block types including the comment-container format for custom blocks. |
| `types.rs` | `Document`, `Block`, `FrontMatter` definitions. |
| `frontmatter.rs` | YAML front-matter splitting. |
| `delimiter.rs` | Scans for `lopress:` comment delimiters. |
| `perf.rs` | Performance timing spans. |
| `error.rs` | Parse error types. |

**Parsing strategy:**

1. Split front-matter (YAML between `---` markers).
2. Scan for custom block delimiters.
3. If delimiters found, build a tree: flush plain markdown before each delimiter, recursively parse nested content between open/close pairs.
4. If no delimiters, parse as plain markdown via `pulldown-cmark`.

**Block types recognized by the parser:**

| Type | Source | Structure |
|------|--------|-----------|
| `paragraph` | Standard markdown paragraph | `text` |
| `heading` | `#` through `######` | `text` + `attrs.level` |
| `quote` | `>` blockquote | `children` |
| `code` | ``` fenced code block | `text` + `attrs.lang` |
| `list` | `-` or `1.` lists | `children` (list_item → paragraph) |
| `image` | `![alt](src)` standalone | `attrs.src`, `attrs.alt` |
| `list_item` | List item content | `children` (paragraph) |
| `lopress:*` | HTML-comment delimiters | `children` + `attrs` |

**List parsing detail:** Both tight lists (no blank line between items) and loose lists (blank lines) are normalized so every `list_item` has exactly one `paragraph` child. This uniform shape simplifies the editor's list widget.

### 5.2 lopress-plugin

**Location:** `crates/lopress-plugin/`

**Responsibility:** Plugin manifest parsing, loading, and registry management.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `manifest.rs` | `PluginManifest`, `BlockDecl`, `AttrDecl`, `AttrType` types + TOML parsing. |
| `registry.rs` | `PluginRegistry` — indexes blocks by name and native type, manages lookup. |
| `loader.rs` | Loads plugins from a directory. |
| `error.rs` | Plugin error types. |

**Block declaration fields:**

```rust
struct BlockDecl {
    name: String,           // e.g. "list", "code", "lopress:video"
    template: Option<String>,  // HTML template path (None for base plugins)
    attrs: BTreeMap<String, AttrDecl>,
    renderer: Option<String>,    // Capability #2 — renderer type
    editor: Option<String>,      // Editor key for the editor widget
    builtin: bool,               // True for base plugins
    native: Option<String>,      // Capability #1 — core Block type claim
    css: Vec<String>,            // Capability #3 — asset CSS files
    js: Vec<String>,           // Capability #3 — asset JS files
}
```

**Attribute declaration:**

```rust
enum AttrType { String, Number, Bool, Array, Object }

struct AttrDecl {
    kind: AttrType,
    required: bool,
    default: Option<Value>,
    ui: Option<String>,    // UI hint: "text", "checkbox", "hidden", etc.
    options: Vec<String>,  // For select-style attributes
}
```

### 5.3 lopress-theme

**Location:** `crates/lopress-theme/`

**Responsibility:** Theme engine using Tera templates.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `engine.rs` | `ThemeEngine` — loads theme templates, renders pages. |
| `resolver.rs` | Resolves theme name to plugin. |
| `builtin.rs` | Built-in default theme (embedded at compile time). |
| `context.rs` | `SiteCtx`, `PageCtx`, `PageKind`, `PostSummary`, `RenderContext` — template context types. |
| `error.rs` | Theme error types. |

**Template context:**

```rust
struct SiteCtx {
    title: String,
    base_url: String,
    nav: Vec<NavItem>,
    posts: Vec<PostSummary>,
}

struct PageCtx {
    kind: PageKind,     // Index, Post, Page, Tag
    title: String,
    slug: String,
    url: String,
    canonical: String,
    description: Option<String>,
    og_image: Option<String>,
    date: Option<NaiveDate>,
    tags: Vec<String>,
    body_html: String,
    posts: Vec<PostSummary>,
    tag: Option<String>,
}
```

**Built-in theme provides templates:**
- `layout.html` — base layout
- `post.html` — individual post
- `index.html` — home page with post list
- `page.html` — standalone page
- `tag.html` — tag archive

### 5.4 lopress-assets

**Location:** `crates/lopress-assets/`

**Responsibility:** Image processing — generates responsive WebP variants.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `image.rs` | Image resizing to WebP variants. |
| `cache.rs` | Variant cache keyed on source hash. |
| `error.rs` | Image error types. |
| `lib.rs` | Public API. |

**Default variant widths:** 400px, 800px, 1600px. Emits `<picture>` with `srcset`.

### 5.5 lopress-build

**Location:** `crates/lopress-build/`

**Responsibility:** Static site generation pipeline.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `site.rs` | `Workspace`, `SiteConfig` — workspace loading and config. |
| `pages.rs` | `discover()`, `render_all()`, `render_one_post()`, `render_one_page()` — page rendering. |
| `render.rs` | `render_body()` — converts `Document` blocks to HTML. |
| `cache.rs` | `BuildCache` — hash-keyed cache for incremental builds. |
| `feed.rs` | RSS/Atom feed generation. |
| `sitemap.rs` | Sitemap.xml generation. |
| `robots.rs` | robots.txt generation. |
| `not_found.rs` | 404.html generation. |

**Build process:**

1. Load `lopress.toml` → `Workspace` + `SiteConfig`.
2. Discover all `.md` files in `src/posts/` and `src/pages/`.
3. Parse each file → `Document`.
4. Render each document's body to HTML via `render_body()` (which dispatches to Tera templates for custom blocks).
5. Render each page through the theme engine (`post.html`, `page.html`, etc.).
6. Write output to `www/`.
7. Generate aggregates: index, feed, sitemap, robots, 404.
8. Update cache for next incremental build.

**Caching:** Each page entry stores a source hash, output paths, tags, draft status, title, and date. On rebuild, pages with unchanged hashes are skipped.

### 5.6 lopress-watch

**Location:** `crates/lopress-watch/`

**Responsibility:** File-system watcher using `notify`.

Spawns a background thread that watches the workspace directory. On file changes, it triggers a debounced rebuild via the `Session::rebuild()` mechanism.

### 5.7 lopress-serve

**Location:** `crates/lopress-serve/`

**Responsibility:** HTTP preview server with Server-Sent Events (SSE) for browser reload.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `server.rs` | HTTP server setup. |
| `http.rs` | Request handling. |
| `sse.rs` | SSE broadcast mechanism. |
| `router.rs` | Route definitions. |
| `inject.rs` | HTML injection for reload script. |
| `mime.rs` | MIME type handling. |

**Server behavior:**
- Serves `www/` directory.
- On rebuild, broadcasts a reload message via SSE to connected browsers.
- Attempts port 8080, falls back to ephemeral port.
- There is no standalone `serve` CLI subcommand (the binary's CLI is only `lopress new` or the GUI); the preview server is started by the GUI session when a workspace opens.

### 5.8 lopress-gui-host

**Location:** `crates/lopress-gui-host/`

**Responsibility:** Session management — bridges the GUI editor with the build/watch/serve infrastructure.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `session.rs` | `Session` — the central runtime object. Manages workspace, watcher, server, build status. |
| `document.rs` | `LoadedDocument` — the document as loaded from disk. |
| `error.rs` | Load/Open/Save error types. |

**Session lifecycle:**

1. `Session::open(workspace_root)` — loads workspace, scans files, spawns background thread for initial build + serve start, spawns watcher thread.
2. `session.load_document(path)` — reads and parses a `.md` file.
3. `session.save(doc)` — serializes and atomically writes a document.
4. `session.rebuild()` — triggers background rebuild + SSE broadcast.
5. `session.plugin_registry()` — returns the plugin registry (base plugins + user plugins).
6. `session.rescan()` — re-scans posts/pages directories.

**Atomic write:** Uses a temp file + rename pattern to prevent partial writes.

### 5.9 lopress-editor

**Location:** `crates/lopress-editor/`

**Responsibility:** The block editor GUI and its action/undo system. This is the most complex crate.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `lib.rs` | App entry point — loads settings, creates Floem application. |
| `state.rs` | `AppContext`, `AppState` (Welcome/Editing), `EditingState`. |
| `actions.rs` | `BlockAction` enum + `apply()` chokepoint. |
| `undo.rs` | `UndoStack` — stores action/inverse pairs. |
| `model/types.rs` | `EditorDoc`, `EditorBlock`, `BlockBody`, `InlineRun`, `TableData`, `PluginMeta`. |
| `model/descriptor.rs` | `BlockDescriptor` table — single source of truth per built-in block type (editor key, native claim, body shape, menu entries, default block). |
| `model/inserter.rs` | Slash-menu inserter entries projected from the descriptor table + plugin registry. |
| `model/from_core.rs` | `lopress_core::Document` → `EditorDoc`. |
| `model/to_core.rs` | `EditorDoc` → `lopress_core::Document`. |
| `model/sync.rs` | `InlineRun` ↔ `Rope` conversion utilities. |
| `model/inline.rs` | Inline text parsing/serialization. |
| `model/style_span.rs` | `StyleSpan` — inline formatting spans. |
| `ui/mod.rs` | Root view — switches Welcome/Editing, builds three-column layout. |
| `ui/blocks/mod.rs` | Per-block rendering dispatch. |
| `ui/blocks/inline_editor.rs` | Editable paragraph/heading widget (uses Floem's native editor). |
| `ui/blocks/code_editor.rs` | Editable code block widget. |
| `ui/blocks/list.rs` | Editable list widget. |
| `ui/blocks/plugin.rs` | Plugin block view (header strip + attr form + body editor). |
| `ui/blocks/heading.rs` | Editable heading widget. |
| `ui/blocks/paragraph.rs` | Editable paragraph widget. |
| `ui/blocks/opaque.rs` | Read-only opaque block display. |
| `ui/blocks/image.rs` | Image block widget. |
| `ui/blocks/table.rs` | Table block widget. |
| `ui/blocks/separator.rs` | Separator block widget. |
| `ui/blocks/read_more.rs` | Read-more marker widget. |
| `ui/blocks/fallback.rs` | Fail-safe rendering for unrenderable blocks. |
| `ui/blocks/env.rs` | `BlockEnv` — shared context threaded to block widgets. |
| `ui/blocks/editor_registry.rs` | Data-driven editor dispatch from manifest `editor` key. |
| `ui/blocks/style_span.rs` | `InlineRunStyling` — Floem `Styling` trait impl. |
| `ui/editor_pane.rs` | Block tree canvas. |
| `ui/link_bar.rs` | Pane-level link editing bar (survives block-pane rebuilds). |
| `ui/nav_editor.rs` | Site-settings nav editor (`nav.toml`). |
| `ui/toolbar.rs` | Block-type toolbar (P/H1/H2/Code/UL/OL + style toggles). |
| `ui/slash_menu.rs` | Slash command popup (`/` in empty paragraph). |
| `ui/inspector.rs` | Right-hand inspector panel. |
| `ui/sidebar.rs` | Left-hand sidebar (post/page list). |
| `ui/footer.rs` | Bottom footer (build status, save status, serve status). |
| `ui/welcome.rs` | Welcome screen (workspace picker). |
| `ui/editing/` | Save pipeline, action sink, undo/redo, pane key, focus, new doc, ctrl wire. |
| `ui/dnd.rs` | Drag-and-drop support. |
| `ctrl/mod.rs` | Debug control server (HTTP API for testing). |
| `settings.rs` | User settings (window geometry, recents). |
| `recents.rs` | Recent workspace list. |

---

## 6. Plugin Architecture

### 6.1 Manifest Format

A plugin is a directory under `<workspace>/plugins/<name>/` with a `plugin.toml`:

```toml
name = "my-plugin"
version = "0.1.0"
theme = true  # or false (default)

[[blocks]]
name = "lopress:video"
template = "blocks/video.html"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }

[blocks.attrs.poster]
type = "string"
ui = "file-picker"
```

### 6.2 Block Declaration Types

Plugins declare blocks with three capabilities:

**Capability #1 — Native (`native` field):**
The block IS a native markdown construct. When serialized, it uses the native markdown format (e.g., a list block serializes as actual `- item` markdown, not as HTML comments). This is how `list` and `code` blocks work — they're declared as plugins but render as standard markdown.

**Capability #2 — Renderer (`renderer` field):**
A WASM renderer for blocks whose output can't be expressed as a template. (Planned, not yet implemented.)

**Capability #3 — Assets (top-level `[assets]` table, plus per-block `css`/`js`):**
CSS and JS files the build injects into every rendered page — `<link>` before `</head>`, `<script defer>` before `</body>`. Declared paths (relative to the plugin root) map to their copied web path under `/assets/<plugin-name>/`. Only enabled plugins inject; order within a plugin is preserved. Implemented in `lopress-build/src/assets.rs`.

### 6.3 Registry Lifecycle

```
1. Session::plugin_registry()  (lopress-gui-host/src/session.rs)
   → Calls lopress_plugin::load_dir() on workspace/plugins/
   → Filters by the enabled list in lopress.toml
   → Returns a registry of USER plugins only — load_dir starts from an
     empty PluginRegistry::default() and does NOT seed the base plugins

2. EditingState::new(session)  (lopress-editor/src/state.rs)
   → Starts from an empty PluginRegistry::default()
   → Calls load_base_plugins() → seeds the eight embedded base plugins
     (paragraph, heading, code, list, image, table, separator, more —
     manifests in base_plugins/*/manifest.toml, embedded via include_str!;
     non-removable, always present)
   → Layers the user plugins from session.plugin_registry() on top via
     insert(); a user plugin whose block name collides with a base block
     is rejected by insert and the error is ignored (`let _ = …`)
   → The resulting merged registry is held by EditingState and used by
     from_core for block classification and by the plugin block view for
     attr-form rendering
```

The base-plugin seeding and the user-plugin merge both happen in
`EditingState::new`, **not** in `Session::plugin_registry()` — the session
only contributes the user plugins.

**Registry indexing:**
- `block_index`: `String → (plugin_index, block_index)` — lookup by block name.
- `native_index`: `String → (plugin_index, block_index)` — lookup by core Block type.
- `theme_index`: `String → plugin_index` — lookup by theme name.

**Duplicate handling:** `PluginRegistry::insert` returns an error on both
duplicate block names and duplicate native claims. Base plugins are inserted
first, so when a user plugin collides the `insert` error is ignored at the
call site (`EditingState::new`) and the user block is silently skipped — base
plugins cannot be shadowed.

### 6.4 Native vs Comment-Container Blocks

| Aspect | Native Block | Comment-Container Block |
|--------|-------------|------------------------|
| Serialization | Uses native markdown format | Uses `<!-- lopress:name -->` markers |
| Example | `list` serializes as `- item` | `video` serializes as HTML comments |
| `native` field | Present | Absent (or None) |
| Markdown portability | Fully portable | Prose is portable; blocks are invisible HTML comments |
| Editor rendering | Has `PluginMeta` → plugin view | Has `PluginMeta` → plugin view |

**Serialization decision tree in `to_core`** (every block has a `PluginMeta`):

```
Block's PluginMeta has a native claim?
  ├─ Yes → native_block_to_core() (renders as native markdown — paragraph,
  │        heading, code, list, image, table, separator)
  └─ No  → plugin_block_to_core() (wraps in a <!-- lopress:… --> comment container)
```

Dispatch keys off the descriptor **editor key**, not the body shape — two types
can share a body shape (image and separator differ, yet image is Opaque-bodied
and separator Inline-bodied); asserting on `plugin.editor` identity is the
reliable check in round-trip tests.

### 6.5 Plugin Loading Order

1. **Base plugins** (embedded at compile time via `include_str!` from `base_plugins/*/manifest.toml`): `paragraph`, `heading`, `code`, `list`, `image`, `table`, `separator`, `more`
2. **User plugins** (loaded from `plugins/` directory at runtime)

User plugins that declare a block name already owned by a base plugin are silently skipped (the `insert` method returns an error, which is ignored).

---

## 7. Editor Architecture

### 7.1 Block Model

The editor uses a richer block model than the core document:

```
EditorDoc
  └── Vec<EditorBlock>
        ├── id: BlockId          (u64, monotonic counter)
        ├── body: BlockBody      (enum: Inline, Code, List, Table, Opaque)
        └── plugin: PluginMeta   (always present — every block is a plugin block)
```

**Type identity vs data shape:**
- A block's **type** is its `PluginMeta` — `block_type_name`, the `editor` key
  that selects its widget, and the optional `native` core-type claim.
- `BlockBody` describes the **data shape** (how content is stored).
- The mapping between the two is declared once in the **block descriptor table**
  (`model/descriptor.rs`): one `BlockDescriptor` per built-in type carrying its
  `editor` key, `native` claim, `BodyShape`, `builtin` flag, slash-menu/toolbar
  `MenuEntry` list, and a `default_block` constructor. Menus, `ChangeType`
  conversion, and `from_core`/`to_core` dispatch all read this table instead of
  hardcoding per-type match arms. The widget fn-pointers live separately in
  `ui/blocks/editor_registry.rs`, keyed by the same `editor` string.

**`InlineRun`** — the atomic unit of inline text in the editor:
```rust
struct InlineRun {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    link: Option<String>,
}
```
Runs are stored as a `Vec<InlineRun>` for paragraph/heading bodies. Adjacent runs with identical styles are coalesced (canonicalized) to minimize the number of runs.

**`PluginMeta`** — metadata stamped on plugin blocks:
```rust
struct PluginMeta {
    block_type_name: Rc<str>,       // e.g. "lopress:video"
    attrs: serde_json::Map<String, Value>,  // parsed attributes
    attr_decls: Rc<[AttrDecl]>,     // attribute declarations from manifest
    builtin: bool,                  // true for base plugins
    editor: Option<Rc<str>>,        // editor key (e.g. "list", "code")
    native: Option<Rc<str>>,        // native core type claim
}
```

### 7.2 Action System

All block-tree mutations go through a single `apply(doc, action)` chokepoint:

```rust
enum BlockAction {
    Split { block_id, byte_offset, new_block_id },
    MergeWithPrev { block_id },
    Delete { block_id },
    Move { block_id, to_index },
    ChangeType { block_id, new_editor, new_attrs },  // editor key + seed attrs
    OpenSlashMenu { block_id },                      // UI-only, no-op in apply
    EditAttrs { block_id, new_attrs },               // Box<serde_json::Map>
    EditBlockBody { block_id, new_body, built_in },  // Box<BlockBody> + provenance flag
    InsertAfter { anchor, new_block },               // Box<EditorBlock>
    EditFrontMatter { new_front_matter },            // Box<FrontMatter>
    TableInsertRow { block_id, at },
    TableDeleteRow { block_id, row },
    TableInsertColumn { block_id, at },
    TableDeleteColumn { block_id, col },
    TableSetAlign { block_id, col, align },
}
```

**`apply()` returns `Option<(canonical_action, inverse_action)>`:**
- `Some((action, inverse))` — the action is recordable on the undo stack.
- `None` — the action is UI-only or a no-op (e.g., `OpenSlashMenu`, `MergeWithPrev` on first block).

**Key invariants:**
- `EditBlockBody` compares bodies in **canonical form** (coalesced runs, no empty runs) to avoid spurious undo entries.
- `Split` mints a `new_block_id` that is returned in the canonical action, so redo reuses the same ID.
- `ChangeType` body conversions are **lossy on undo** — the original body is not snapshot. This is a known limitation documented in the code.
- `BlockAction` variants are boxed to keep the enum size small (≤40 bytes).

### 7.3 Undo/Redo

The `UndoStack` stores `(canonical_action, inverse_action)` pairs. Each edit pushes one pair. Undo pops the inverse and reapplies it. Redo pops the canonical and reapplies it.

**Stack operations:**
- `push_after_apply(canonical, inverse)` — called after `apply()` succeeds.
- `pop_undo()` — returns the inverse action for reapplication.
- `pop_redo()` — returns the canonical action for reapplication.
- `undo_depth()` — number of undo entries available.

**New edits after an undo clear the redo stack** (standard undo behavior).

### 7.4 UI Components

The editor UI is a three-column layout built with Floem:

```
┌─────────────────────────────────────────────────────────┐
│  Sidebar          │  Editor Pane           │  Inspector  │
│  (left)           │  (center)              │  (right)    │
│                   │                        │             │
│  - Workspace name │  - Block tree          │  - Block    │
│  - Posts list     │    (paragraphs,        │    attrs    │
│  - Pages list     │     code blocks,       │  - Front-   │
│                   │     lists, etc.)       │    matter   │
│                   │                        │             │
│                   │  ┌──────────────────┐  │             │
│                   │  │ Block 1 (para)   │  │             │
│                   │  │ Block 2 (code)   │  │             │
│                   │  │ Block 3 (list)   │  │             │
│                   │  └──────────────────┘  │             │
│                   │                        │             │
├─────────────────────────────────────────────────────────┤
│  Footer: Build status │ Save status │ Serve status      │
└─────────────────────────────────────────────────────────┘
```

**Per-block rendering:**

```rust
fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target, focus_pub, dnd,
    current_doc, on_undo, on_redo,
) -> AnyView
```

**Dispatch logic:** every block carries a `PluginMeta`, so `block_view`
(`ui/blocks/mod.rs`) always renders via `plugin::plugin_block_view`, which
resolves the block's `editor` key through `ui/blocks/editor_registry.rs` to the
concrete widget (paragraph, heading, code, list, image, table, separator,
read-more, or the generic attr-form for site plugins). Built-in blocks
(`builtin: true` in the descriptor) suppress the plugin chrome (header strip +
attr form). Unrenderable blocks fall back to `ui/blocks/fallback.rs` instead of
disappearing. All blocks then funnel through `wrap_block` for shared chrome
(drag handle, hover/focus styling, floating toolbar).

**Toolbar:** Anchored above the focused block. Shows block-type cycler (P/H1/H2/Code/UL/OL) and style toggles (B/I/C/link).

**Slash menu:** Triggered by typing `/` in an empty paragraph. Shows block-type options.

### 7.5 Save Pipeline

```
User edits
    │
    ▼
mark_dirty() → dirty_sig = true, dirty_counter++
    │
    ▼
debounce_action(500ms)
    │
    ▼
save_doc(&doc)
    │
    ├── doc_to_core(doc) → lopress_core::Document
    ├── session.save(loaded_doc) → serialize + atomic write
    ├── dirty_sig = false
    ├── save_error_sig = None
    └── session.rebuild() → background rebuild + SSE broadcast
```

**Key optimization:** The save closure performs the `RefCell::borrow()` of editing state inside `with_untracked`, passing `&doc` directly instead of cloning the full `EditorDoc`. This eliminates one full-text allocation per save.

---

## 8. Build Pipeline

### 8.1 Discovery

```rust
fn discover(dir: &Path, kind: &str) → (Vec<DiscoveredPost>, Vec<PageFailure>)
```

Walks `src/posts/` and `src/pages/`, parses each `.md` file, computes slug from front-matter or filename. Returns discovered posts/pages and any parse failures.

### 8.2 Rendering

```rust
fn render_all(
    workspace, registry, theme, tera_shared,
    posts, pages, cache, force_full,
) → RenderStats
```

For each post/page:
1. Check cache: if source hash unchanged and outputs exist, skip.
2. If draft: clean up stale outputs, record cache entry, skip body rendering.
3. If not draft: render body HTML, render through theme template, write to `www/`.
4. Update cache entry.
5. Prune orphaned cache entries.

### 8.3 Caching

```rust
struct PageEntry {
    source_hash: String,    // SHA-256 of source file
    outputs: Vec<String>,   // Relative paths of generated files
    tags: Vec<String>,
    is_draft: bool,
    title: Option<String>,
    date: Option<String>,
}
```

Cache is stored at `www/.lopress-cache.json`. On rebuild, entries are compared by source hash. If unchanged, the page is skipped entirely.

**Aggregate regeneration:** Index, feed, sitemap, and tag pages are regenerated whenever any post's aggregate-visible metadata changes (draft flip, slug/title/date/tag edit, new/removed post).

### 8.4 Aggregates

| Aggregate | Output | Depends on |
|-----------|--------|------------|
| Index | `www/index.html` | All non-draft posts |
| Feed | `www/feed.xml` | All non-draft posts |
| Sitemap | `www/sitemap.xml` | All non-draft posts + pages |
| Robots | `www/robots.txt` | `lopress.toml` robots config |
| 404 | `www/404.html` | Static template |
| Tags | `www/tags/<tag>/index.html` | Posts with each tag |

---

## 9. Theme System

**Resolution:** Theme name from `lopress.toml` → look up in plugin registry → if not found, use built-in default theme.

**Built-in default theme** is embedded at compile time via `include_str!`. It provides:
- `layout.html` — base HTML layout
- `post.html` — post template
- `index.html` — home page template
- `page.html` — page template
- `tag.html` — tag archive template
- Default CSS

**Custom themes** are plugins with `theme = true`. They provide their own template files and optionally a stylesheet.

**Template context injection:**
- `SiteCtx` — site-wide data (title, base_url, nav, posts).
- `PageCtx` — page-specific data (title, slug, url, body_html, etc.).
- Plugin templates receive `attrs` and `inner_html`.

---

## 10. Live Preview

```
User saves document
    │
    ▼
session.save() → atomic write to disk
    │
    ▼
session.rebuild() → background thread
    │
    ├── lopress_build::build()
    │     ├── discover posts/pages
    │     ├── render bodies
    │     ├── render through theme
    │     └── write to www/
    │
    ▼
server.broadcast_reload() → SSE message
    │
    ▼
Browser receives SSE → reloads page
```

The preview server runs on `127.0.0.1:8080` (or ephemeral port if 8080 is taken). It serves the `www/` directory and injects a reload script into HTML pages that connects via SSE.

---

## 11. File Index

### lopress-core
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/types.rs` | `Document`, `Block`, `FrontMatter` |
| `src/parser.rs` | Markdown → Document |
| `src/serializer.rs` | Document → markdown |
| `src/frontmatter.rs` | YAML front-matter splitting |
| `src/delimiter.rs` | Custom block delimiter scanning |
| `src/error.rs` | ParseError |
| `src/perf.rs` | Performance timing |

### lopress-plugin
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/manifest.rs` | PluginManifest, BlockDecl, AttrDecl, TOML parsing |
| `src/registry.rs` | PluginRegistry with block/native/theme indexes |
| `src/loader.rs` | Directory loading |
| `src/error.rs` | PluginError |

### lopress-theme
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/engine.rs` | ThemeEngine |
| `src/resolver.rs` | Theme name → plugin resolution |
| `src/builtin.rs` | Built-in default theme |
| `src/context.rs` | SiteCtx, PageCtx, PostSummary, etc. |
| `src/error.rs` | ThemeError |

### lopress-assets
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/image.rs` | WebP variant generation |
| `src/cache.rs` | Variant cache |
| `src/error.rs` | ImageError |

### lopress-build
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/site.rs` | Workspace, SiteConfig |
| `src/pages.rs` | discover, render_all, render_one_post/page |
| `src/render.rs` | render_body (blocks → HTML) |
| `src/cache.rs` | BuildCache, PageEntry |
| `src/feed.rs` | RSS/Atom feed |
| `src/sitemap.rs` | Sitemap |
| `src/robots.rs` | robots.txt |
| `src/not_found.rs` | 404.html |
| `src/error.rs` | BuildError, PageFailure |

### lopress-watch
| File | Role |
|------|------|
| `src/lib.rs` | Watcher spawn |

### lopress-serve
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/server.rs` | HTTP server |
| `src/sse.rs` | SSE broadcast |
| `src/router.rs` | Routes |
| `src/http.rs` | Request handling |
| `src/inject.rs` | Reload script injection |
| `src/mime.rs` | MIME types |
| `src/error.rs` | ServeError |

### lopress-gui-host
| File | Role |
|------|------|
| `src/lib.rs` | Module exports |
| `src/session.rs` | Session — central runtime object |
| `src/document.rs` | LoadedDocument |
| `src/error.rs` | LoadError, OpenError, SaveError |

### lopress-editor
| File | Role |
|------|------|
| `src/lib.rs` | App entry, run() |
| `src/state.rs` | AppContext, AppState, EditingState |
| `src/actions.rs` | BlockAction enum, apply() chokepoint |
| `src/undo.rs` | UndoStack |
| `src/model/types.rs` | EditorDoc, EditorBlock, BlockBody, InlineRun, TableData, PluginMeta |
| `src/model/descriptor.rs` | BlockDescriptor table (per-type source of truth) |
| `src/model/inserter.rs` | Slash-menu inserter projection |
| `src/model/from_core.rs` | Core → EditorDoc |
| `src/model/to_core.rs` | EditorDoc → Core |
| `src/model/sync.rs` | Rope ↔ InlineRun conversion |
| `src/model/inline.rs` | Inline text parsing/serialization |
| `src/model/style_span.rs` | StyleSpan, InlineRunStyling |
| `src/ui/mod.rs` | root_view, editing_view |
| `src/ui/blocks/mod.rs` | block_view dispatch |
| `src/ui/blocks/inline_editor.rs` | Editable paragraph/heading |
| `src/ui/blocks/code_editor.rs` | Editable code block |
| `src/ui/blocks/list.rs` | Editable list |
| `src/ui/blocks/plugin.rs` | Plugin block view |
| `src/ui/blocks/heading.rs` | Editable heading |
| `src/ui/blocks/paragraph.rs` | Editable paragraph |
| `src/ui/blocks/opaque.rs` | Read-only opaque block |
| `src/ui/blocks/image.rs` | Image block widget |
| `src/ui/blocks/table.rs` | Table block widget |
| `src/ui/blocks/separator.rs` | Separator block widget |
| `src/ui/blocks/read_more.rs` | Read-more marker widget |
| `src/ui/blocks/fallback.rs` | Fail-safe block rendering |
| `src/ui/blocks/env.rs` | BlockEnv shared widget context |
| `src/ui/blocks/editor_registry.rs` | Data-driven editor dispatch |
| `src/ui/blocks/style_span.rs` | InlineRunStyling |
| `src/ui/editor_pane.rs` | Block tree canvas |
| `src/ui/link_bar.rs` | Pane-level link editing bar |
| `src/ui/nav_editor.rs` | Nav (site settings) editor |
| `src/ui/toolbar.rs` | Block-type toolbar |
| `src/ui/slash_menu.rs` | Slash command popup |
| `src/ui/inspector.rs` | Inspector panel |
| `src/ui/sidebar.rs` | Sidebar (post/page list) |
| `src/ui/footer.rs` | Footer (status indicators) |
| `src/ui/welcome.rs` | Welcome screen |
| `src/ui/dnd.rs` | Drag-and-drop |
| `src/ui/editing/mod.rs` | Editing submodules |
| `src/ui/editing/action_sink.rs` | ActionSink builder |
| `src/ui/editing/undo_redo.rs` | Undo/redo builders |
| `src/ui/editing/save_pipeline.rs` | Debounced save pipeline |
| `src/ui/editing/pane_key.rs` | Pane rebuild key |
| `src/ui/editing/focus.rs` | Focus management |
| `src/ui/editing/new_doc.rs` | New doc action builder |
| `src/ui/editing/ctrl_wire.rs` | Debug ctrl wiring |
| `src/ctrl/mod.rs` | Debug control server |
| `src/ctrl/input.rs` | Key input parsing |
| `src/settings.rs` | User settings |
| `src/recents.rs` | Recent workspaces |

---

*End of specification.*

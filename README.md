# Lopress

A desktop blog-authoring tool with a Gutenberg-style block editor and a built-in static site generator, written in Rust.

Point lopress at a directory. Write posts in a block editor. Save. The directory now contains a static website — HTML, CSS, images, optional JavaScript — ready to deploy anywhere.

**Status: GUI editor MVP.** `lopress` (no args) opens the block editor and `lopress new <dir>` scaffolds a new site. Building and live preview happen automatically inside the editor — it builds the site incrementally on every save and serves a live preview on `http://127.0.0.1:8080`. Standalone `lopress build` / `lopress serve` CLI subcommands are designed but not yet wired into the binary. See [`docs/superpowers/specs/2026-04-18-lopress-design.md`](docs/superpowers/specs/2026-04-18-lopress-design.md) for the full design.

## What lopress is

- A single Rust binary. A GUI (built with [floem](https://github.com/lapce/floem)) and a static site generator live in the same process.
- Source of truth is markdown on disk. No database. No server required to host the output.
- Content is authored as blocks — paragraphs, headings, images, code, lists, and plugin-defined block types — serialized into markdown using the Gutenberg HTML-comment delimiter convention, so the markdown stays portable and readable in any other tool.
- Extensible via plugins that contribute block types, themes, and asset JS/CSS.

## Workspace layout

```
<workspace>/
  lopress.toml       # site config: title, base_url, theme, enabled plugins
  src/
    posts/           # .md files, one per blog post
    pages/           # .md files for standalone pages
    images/          # source images
  plugins/           # site-local plugins (themes, custom blocks)
  www/               # generated static site — gitignore-able
```

Drop the workspace into git. Clone it elsewhere, run lopress, and everything works — plugins and content travel with the repo, so builds are reproducible.

## How the build works

When a file in `src/` changes — whether from lopress itself or from vim or a `git checkout` — a file-system watcher fires a debounced rebuild. Lopress reads only what changed, regenerates the affected pages, refreshes responsive image variants if needed, and writes to `www/`. The background preview server reloads.

A build cache keyed on source hashes (config, theme, plugins, and per-page) keeps rebuilds fast as the site grows.

## Editor

A block-tree editor with a file sidebar and a front-matter inspector:

- **Left — sidebar**: lists the workspace's posts and pages, with buttons to create a new post or page.
- **Center — block canvas**: the keyboard-driven block editor. `/` opens the block inserter (built-in kinds plus any registered plugin blocks), `Enter` splits a paragraph, `Backspace` at the start merges blocks, and blocks reorder by drag-and-drop. Inline formatting uses `Ctrl/Cmd+B` (bold), `Ctrl/Cmd+I` (italic), and `Ctrl/Cmd+K` (link). A small toolbar above the focused block converts its type (P / H1–H6 / Code / lists) and toggles formatting. Plugin block attributes are edited in a form rendered inline within the block.
- **Right — inspector**: the front-matter fields for the current post (title, slug, date, tags, draft, description).
- **Footer**: build status, save state, word count, and the live-preview URL.

The live preview is served from `www/` on `http://127.0.0.1:8080` and rebuilds on save; open it in any browser alongside the editor.

## Content format

Standard markdown for standard content. Plugin blocks use HTML-comment delimiters with their attributes as JSON in the opening marker:

```markdown
# My first post

A paragraph with **regular markdown**.

<!-- lopress:callout {"variant":"warning","title":"Heads up","body":"Watch **this**."} -->
<!-- /lopress:callout -->
```

Opened in any markdown tool, the prose renders normally and plugin blocks appear as inert HTML comments. Inside lopress, the block editor round-trips the same file without drift.

## Plugins

A plugin is a directory under `<workspace>/plugins/<name>/` with a `plugin.toml` manifest. Plugins can provide:

- **Block types** — declare form attributes (text, textarea, number, bool, select, …) and supply either a Tera **HTML** template or a Tera **markdown** template; optionally ship JS/CSS. Registered plugin blocks appear in the editor's `/` inserter.
- **Themes** — a plugin with `theme = true` provides the template set (`layout.html`, `post.html`, `index.html`, `page.html`, `tag.html`) and a stylesheet.

A built-in default theme ships with the binary, so fresh sites work without installing anything. The repository's `plugins/` directory includes example blocks (`callout`, `button`) you can copy into a site.

**See [`docs/plugins.md`](docs/plugins.md) for the full `plugin.toml` manifest reference** — every field, the block flavors, attribute/UI options, template context, and worked examples.

## Output

- URLs: `/posts/<slug>/`, `/<page-slug>/`, `/tags/<tag>/` — flat, no date prefix.
- Auto-generated: `feed.xml`, `sitemap.xml`, `robots.txt`, `404.html`, OpenGraph/Twitter card meta tags.
- Image pipeline: source images in `src/images/` generate responsive WebP variants (default widths 400/800/1600 px, configurable), emitted as `<picture>` with `srcset`. Variants are cached by source hash.

## Building

`cargo build --release` builds for the **current host only**. There is no single command that produces all platform binaries at once. The editor uses wgpu, so the binary needs a GPU-capable window server (it will not run headless without a virtual display).

**macOS / Linux / Windows (native)**
```
cargo build --release
```
On Windows the resulting `target\release\lopress.exe` is self-contained.

**Windows cross-compile from macOS/Linux**

The easiest path is [`cross`](https://github.com/cross-rs/cross), which uses Docker containers:

```
cargo install cross
cross build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/lopress.exe`.

**CI (GitHub Actions)**

The cleanest multi-platform approach is a matrix build:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
runs-on: ${{ matrix.os }}
steps:
  - uses: actions/checkout@v4
  - run: cargo build --release
```

## Usage

Create a new workspace and open the editor:

```
cargo build --release
./target/release/lopress new my-site --title "My Blog" --base-url "https://myblog.example.com"
./target/release/lopress              # open the editor (welcome screen; pick a workspace)
```

Open a workspace in the editor, write and save — the site builds into `my-site/www/` and the live preview is served on `http://127.0.0.1:8080`. The `www/` output is a complete static site; copy it to any static host.

Dedicated `lopress build <site>` and `lopress serve <site>` CLI subcommands are planned but not yet wired into the binary.

## License

TBD.

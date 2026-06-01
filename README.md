# Lopress

A desktop blog-authoring tool with a Gutenberg-style block editor and a built-in static site generator, written in Rust.

Point lopress at a directory. Write posts in a block editor. Save. The directory now contains a static website — HTML, CSS, images, optional JavaScript — ready to deploy anywhere.

**Status: GUI editor MVP complete.** `lopress` (no args) opens the block editor, and `lopress new <dir>` scaffolds a new site. (`lopress build` / `lopress serve` are designed but not yet wired into the current binary.) See [`docs/superpowers/specs/2026-04-18-lopress-design.md`](docs/superpowers/specs/2026-04-18-lopress-design.md) for the full design.

## What lopress is

- A single Rust binary. A GUI (egui) and a static site generator live in the same process.
- Source of truth is markdown on disk. No database. No server required to host the output.
- Content is authored as blocks — paragraphs, headings, images, galleries, embeds, custom block types — serialized into markdown using the Gutenberg HTML-comment delimiter convention, so the markdown remains portable and readable in any other tool.
- Extensible via plugins that contribute block types, themes, and asset JS/CSS.

## Workspace layout

```
<workspace>/
  lopress.toml       # site config: title, base_url, theme, enabled plugins, nav
  src/
    posts/           # .md files, one per blog post
    pages/           # .md files for standalone pages
    images/          # source images
  plugins/           # site-local plugins (themes, custom blocks)
  www/               # generated static site — gitignore-able
```

Drop the workspace into git. Clone it elsewhere, run lopress, and everything works.

## How the build works

When a file in `src/` changes — whether from lopress itself or from vim or a `git checkout` — a file-system watcher fires a debounced rebuild. Lopress reads only what changed, regenerates the affected pages, refreshes responsive image variants if needed, and writes to `www/`. The preview pane in the editor reloads.

A build cache keyed on source hashes means rebuilds stay fast as the site grows.

## Editor

Two-pane canvas with a right-hand inspector, plus a native menu bar:

- **Left**: block-tree editor in egui. Keyboard-driven: `/` opens the block inserter, `Enter` splits paragraphs, `Alt+Up/Down` moves blocks.
- **Middle**: live preview via embedded webview, rendered through the active theme.
- **Right**: inspector showing attributes of the selected block and front-matter fields for the current post.
- **Menu bar**: File (New/Open Site), Site (New Post/Page, Rebuild All, Site Settings), Edit (Undo/Redo, Find), View (toggle panes), Plugins (Manage, Reload), Help.

## Content format

Standard markdown for standard content. Custom blocks use HTML-comment delimiters:

```markdown
# My first post

A paragraph with **regular markdown**.

<!-- lopress:video {"src":"https://example.com/talk.mp4","autoplay":false} -->
<!-- /lopress:video -->

<!-- lopress:callout {"kind":"warning"} -->
Callouts can contain *nested* markdown.
<!-- /lopress:callout -->
```

Opened in any markdown tool, the prose renders normally; custom blocks appear as invisible HTML comments. Inside lopress, the block editor round-trips the same file without drift.

## Plugins

A plugin is a directory under `<workspace>/plugins/<name>/` with a `plugin.toml` manifest. Plugins can provide:

- **Block types** — declare attributes (text, number, bool, select, image-picker, …), supply an HTML template, and optionally ship JS/CSS that loads on pages using the block.
- **Themes** — a plugin with `theme = true` provides the template set (`layout.html`, `post.html`, `index.html`, `page.html`, `tag.html`) and a stylesheet.

A built-in default theme ships with the binary, so fresh sites work without installing anything.

Planned as phase-2 escape hatches (not v1): WASM renderers for blocks whose output can't be expressed as a template, and custom JS-based editor UIs for blocks whose settings don't fit the declarative form model.

## Output

- URLs: `/posts/<slug>/`, `/<page-slug>/`, `/tags/<tag>/` — flat, no date prefix.
- Auto-generated: `feed.xml`, `sitemap.xml`, `robots.txt`, `404.html`, OpenGraph/Twitter card meta tags.
- Image pipeline: source images in `src/images/` generate responsive WebP variants (default widths 400/800/1600 px), emitted as `<picture>` with `srcset`. Variants cached by source hash.

## Building

`cargo build --release` builds for the **current host only**. There is no single command that produces all platform binaries at once.

**macOS / Linux (native)**
```
cargo build --release
```

**Windows (native)**

Install [Rust](https://rustup.rs) on Windows, then:
```
cargo build --release
```
The resulting `target\release\lopress.exe` is self-contained.

**Windows cross-compile from macOS/Linux**

The easiest path is [`cross`](https://github.com/cross-rs/cross), which uses Docker containers:

```
cargo install cross
cross build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/lopress.exe`.

> Note: eframe/egui requires a GPU-capable window server. The resulting binary runs on Windows with a GPU. It will not run in a headless Windows Server environment without additional setup (e.g. a virtual display adapter).

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

CLI-only build workflow (planned — `build` is not yet wired into the binary):

```
./target/release/lopress build my-site
# Open my-site/www/index.html in a browser.
```

The `www/` output is a complete static site — copy it to any static host.

## Live preview

While authoring, run (planned — `serve` is not yet wired into the binary):

```
./target/release/lopress serve my-site
```

This serves `my-site/www/` on `http://127.0.0.1:8080/`, watches the workspace, rebuilds incrementally on every write, and reloads open browser tabs via Server-Sent Events. Flags:

- `--port <n>` — bind port (default 8080).
- `--bind <addr>` — bind address (default `127.0.0.1`; use `0.0.0.0` to reach it from other devices on your LAN).
- `--no-open` — skip opening the default browser on startup.

## License

TBD.

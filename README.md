# Lopress

A desktop blog-authoring tool with a Gutenberg-style block editor and a built-in static site generator, written in Rust.

Point lopress at a directory. Write posts in a block editor. Save. The directory now contains a static website — HTML, CSS, images, optional JavaScript — ready to deploy anywhere.

**Status: pre-implementation.** The design is in [`docs/superpowers/specs/2026-04-18-lopress-design.md`](docs/superpowers/specs/2026-04-18-lopress-design.md). No code yet.

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

## Building (once code exists)

```
cargo build --release
./target/release/lopress
```

## License

TBD.

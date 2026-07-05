# Theme Authoring

A lopress theme is a plugin with `theme = true` that supplies the site's Tera
template set and stylesheet. A built-in default theme ships in the binary, so
fresh sites render without installing anything.

## Directory layout

```
<workspace>/
  plugins/
    my-theme/
      plugin.toml        # name = "my-theme", version, theme = true
      theme.css          # the site stylesheet (plugin root, not templates/)
      templates/
        layout.html
        post.html
        page.html
        index.html
        tag.html
        404.html
```

`plugin.toml`:

```toml
name    = "my-theme"
version = "0.1.0"
theme   = true
```

Every `*.html` file in `templates/` is loaded (by bare filename) into a Tera
engine. The build renders these six templates: `post.html`, `page.html`,
`index.html`, `tag.html`, `404.html`, with `layout.html` as the shared base.
The build's cache also hashes exactly those six plus `theme.css` — changing
any of them invalidates cached pages.

## Selecting the theme

`lopress.toml`:

```toml
[site]
theme = "my-theme"   # the plugin's `name`; default is "default" (built-in)
```

Resolution (`crates/lopress-theme/src/resolver.rs`): a theme plugin with that
name wins; otherwise only `"default"` falls back to the built-in — any other
unknown name is a hard `ThemeError::NotFound`. A plugin literally named
`default` overrides the built-in.

## Tera gotcha: template names are exact strings

Templates are registered under their **bare filenames**. Inheritance must use
the exact registered name:

```html
{% extends "layout.html" %}      ✅
{% extends "./layout.html" %}    ❌ Tera does exact-name lookup — this fails
{% extends "templates/layout.html" %}  ❌ same
```

## Template context

Every template receives two objects, `site` and `page`
(`crates/lopress-theme/src/context.rs`):

**`site`** — site-wide:

| Field | Type | Notes |
|---|---|---|
| `site.title` | string | from `lopress.toml` |
| `site.base_url` | string | no trailing slash |
| `site.nav` | array of `{label, href}` | from `nav.toml` |
| `site.posts` | array of PostSummary | all non-draft posts, for archives |
| `site.favicon` | string or null | web path like `/favicon.png`; null when the site has no favicon |

**`page`** — per-page:

| Field | Type | Notes |
|---|---|---|
| `page.kind` | `"Index"` / `"Post"` / `"Page"` / `"Tag"` | which template context this is |
| `page.title`, `page.slug`, `page.url`, `page.canonical` | string | |
| `page.description`, `page.og_image` | string or null | |
| `page.date` | date or null | posts only |
| `page.tags` | array of string | |
| `page.body_html` | string | the rendered post/page body — emit with `{{ page.body_html \| safe }}` |
| `page.posts` | array of PostSummary | populated on index/tag pages |
| `page.tag` | string or null | tag pages only |

**PostSummary** (entries of `site.posts` / `page.posts`): `title`, `slug`,
`url`, `date`, `tags`, `description`, `excerpt_html` (the pre-`lopress:more`
excerpt, or null — emit with `| safe`).

Because the template names end in `.html`, Tera auto-escapes interpolations;
use `| safe` only for trusted HTML (`body_html`, `excerpt_html`).

## Stylesheet

`theme.css` at the plugin root is copied to `www/assets/theme.css` on every
build. Link it from `layout.html` as `/assets/theme.css`. Custom-block markup
(e.g. `.callout`, `.pullquote` from site plugins) needs its styling here too —
block plugins' own `css` manifest fields are not yet injected by the build.

## Starting point

Copy the built-in default theme rather than starting blank — it is embedded
from `crates/lopress-theme/assets/default-theme/` (the same `templates/` +
`theme.css` layout) and already handles nav, post lists, excerpts/read-more,
tags, and meta/OG tags correctly.

## Verifying a theme

Scaffold a scratch site (`cargo run --quiet -- new <dir>`), drop the theme
into `<dir>/plugins/`, set `[site] theme`, and open the site in the editor
(`cargo run`) — the session builds `www/` and serves a live preview on
127.0.0.1:8080. Inspect the generated `www/` HTML directly. A missing
template or bad `extends` fails the build with a Tera error naming the
template.

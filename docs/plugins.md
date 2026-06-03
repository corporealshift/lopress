# Plugin Manifest Reference (`plugin.toml`)

A lopress plugin is a directory under a site's `plugins/` folder containing a
`plugin.toml` manifest:

```
<workspace>/
  lopress.toml
  plugins/
    callout/
      plugin.toml
      blocks/
        callout.md
```

Plugins are discovered per-site: the build and the editor scan
`<workspace>/plugins/*/plugin.toml` (the binary's built-in "base" plugins are
embedded separately and always present). A plugin can contribute **block types**,
a **theme**, and **assets** (CSS/JS).

This document describes every field the manifest understands. All fields not marked
**required** are optional; unknown fields are ignored.

---

## Top-level fields

```toml
name    = "callout"     # required — unique plugin name
version = "0.1.0"       # required — semver string (informational)
theme   = false         # optional — see "Theme plugins" below (default false)

[[blocks]]              # zero or more block declarations
# ...
```

| Field | Type | Default | Purpose |
|---|---|---|---|
| `name` | string | — *(required)* | Unique plugin identifier. Collisions across plugins are rejected at load. |
| `version` | string | — *(required)* | Plugin version. Informational today. |
| `theme` | bool | `false` | When `true`, the plugin supplies the site theme (template set + stylesheet) instead of blocks. |
| `blocks` | array of tables | `[]` | Block-type declarations. Each is a `[[blocks]]` table. |

---

## Block declarations — `[[blocks]]`

Each `[[blocks]]` entry declares one block type. There are three *flavors*, chosen
by which fields you set:

| Flavor | Set | Output |
|---|---|---|
| **HTML-template** | `template` | Attrs interpolate into a Tera **HTML** template at build time. |
| **Markdown-template-form** | `markdown_template` | Attrs interpolate into a Tera **markdown** template, which then runs through the normal markdown→HTML pipeline. |
| **Base / native** | `builtin` / `native` / `editor` | Shipped in the core binary; edited by a dedicated editor widget. Not authored as a site plugin. |

`template` and `markdown_template` are **mutually exclusive** — setting both is a
load error.

### Fields

| Field | Type | Default | Purpose |
|---|---|---|---|
| `name` | string | — *(required)* | The block's type identifier. Comment-container blocks use the `lopress:` prefix (e.g. `"lopress:callout"`) — this is the name that appears in the `<!-- lopress:callout … -->` delimiter on disk. |
| `template` | string | none | Path (relative to the plugin dir) to a Tera **HTML** template, e.g. `"blocks/button.html"`. Makes this an HTML-template block. |
| `markdown_template` | string | none | Path to a Tera **markdown** template, e.g. `"blocks/callout.md"`. Makes this a markdown-template-form block. Mutually exclusive with `template`. |
| `attrs` | table | `{}` | The block's form fields. See [Attributes](#attributes--blocksattrs). |
| `title` | string | derived | Label shown in the editor's block inserter (slash menu). When absent, derived from `name` (strip `lopress:`, title-case → `"lopress:author-bio"` becomes `"Author bio"`). |
| `description` | string | none | Secondary description line for the inserter entry. |
| `category` | string | `"Blocks"` | Inserter grouping bucket, e.g. `"Text"`, `"Media"`, `"Design"`. |
| `css` | array of strings | `[]` | CSS files this block contributes to the page `<head>`. **Parsed and exposed today; build-side injection is not yet wired** — treat as forward-looking. |
| `js` | array of strings | `[]` | JS files this block contributes. Same status as `css`. |
| `editor` | string | none | *(Base plugins only.)* Selects the built-in editor widget for the block (`"list"`, `"code"`, `"image"`, `"more"`). Leave unset for site plugins — they get the generic attr-form editor. |
| `builtin` | bool | `false` | *(Base plugins only.)* Marks a block shipped inside the core binary; the editor suppresses its chrome (header strip + attr form). **Do not set this in a site plugin.** |
| `native` | string | none | *(Base plugins only.)* Claims a native `lopress_core` block type (e.g. `"list"`), so the block serializes as bare markdown instead of a comment container. Exclusive — one plugin per core type. **Advanced; not for typical site plugins.** |

> **Inserter visibility:** a block appears in the editor's slash-menu inserter when it
> is a comment-container block — i.e. it has `template` *or* `markdown_template`, and is
> **not** `builtin` and does **not** claim a `native` type. Base/native blocks have their
> own dedicated slash entries.

---

## Attributes — `[blocks.attrs]`

Attributes are the block's form fields. Each is a key (the attr name) mapping to an
inline table:

```toml
[blocks.attrs]
variant = { type = "string", ui = "select", options = ["note", "tip", "warning", "danger"], default = "note", label = "Style" }
title   = { type = "string", ui = "text",   label = "Title" }
body    = { type = "string", ui = "textarea", label = "Body", help = "Supports markdown" }
```

| Field | Type | Default | Purpose |
|---|---|---|---|
| `type` | enum | — *(required)* | Value type: `"string"`, `"number"`, `"bool"`, `"array"`, `"object"`. |
| `ui` | string | `"text"` | Which form widget to render — see [UI widgets](#ui-widgets). |
| `default` | any | none | Default value. Pre-fills the field; a freshly inserted block is seeded with it. |
| `required` | bool | `false` | Marks the field as required in the form. |
| `options` | array of strings | `[]` | The choices for `ui = "select"`. Ignored otherwise. |
| `label` | string | attr name | Human-friendly caption shown in the form. Falls back to the attr key. |
| `help` | string | none | Hint text shown beneath the label. |

### UI widgets

| `ui` value | Widget | Works with `type` |
|---|---|---|
| `"text"` *(default)* | Single-line text input | `string` |
| `"textarea"` | Multi-line text input (markdown-friendly) | `string` |
| `"checkbox"` | Toggle | `bool` (also any `type` if `ui = "checkbox"`) |
| `"select"` | One-of toggle buttons; needs `options` | `string` |
| `"number"` | Numeric input | `number` |
| `"hidden"` | *(Used by base plugins for internal attrs; the generic site-plugin form does not currently suppress these — they render as text.)* | — |

> The form currently pairs declarations to stored values **by position**, so a block's
> stored attrs should include every declared attr. The inserter handles this
> automatically (it seeds all declared attrs with their `default` or a type-appropriate
> empty); hand-authored blocks should likewise carry all fields.

---

## Template context

What your template can reference depends on the flavor:

**HTML-template** (`template`) — Tera context has:
- `attrs` — the form values, e.g. `{{ attrs.url }}`, `{{ attrs.variant }}`.
- `inner_html` — the block's nested-markdown body, already rendered to HTML. Emit it
  raw with the `safe` filter: `{{ inner_html | safe }}`.

Because the template name ends in `.html`, Tera **auto-escapes** interpolated values
(so `/go` becomes `&#x2F;go` — valid; browsers decode it). Use `| safe` only for
trusted HTML like `inner_html`.

**Markdown-template-form** (`markdown_template`) — Tera context exposes each attr at
the **top level** (bare names), e.g. `{{ name }}`, `{% if spoiler %}…{% endif %}`. The
rendered markdown then flows through the normal markdown→HTML pipeline, so field values
that contain markdown render as markdown.

---

## Worked examples

### Markdown-template-form block (`plugins/callout/`)

`plugin.toml`:
```toml
name    = "callout"
version = "0.1.0"

[[blocks]]
name             = "lopress:callout"
markdown_template = "blocks/callout.md"
title            = "Callout"
category         = "Text"
description      = "Highlighted note, tip, warning, or danger admonition"

[blocks.attrs]
variant = { type = "string", ui = "select", options = ["note", "tip", "warning", "danger"], default = "note", label = "Style" }
title   = { type = "string", ui = "text",     label = "Title" }
body    = { type = "string", ui = "textarea",  label = "Body", help = "Supports markdown" }
```

`blocks/callout.md`:
```markdown
<div class="callout callout-{{ variant }}">

{% if title %}**{{ title }}**

{% endif %}
{{ body }}

</div>
```

### HTML-template block (`plugins/button/`)

`plugin.toml`:
```toml
name    = "button"
version = "0.1.0"

[[blocks]]
name        = "lopress:button"
template    = "blocks/button.html"
title       = "Button"
category    = "Design"
description = "A call-to-action link styled as a button"

[blocks.attrs]
text    = { type = "string", ui = "text",   required = true, label = "Label" }
url     = { type = "string", ui = "text",   required = true, label = "URL" }
variant = { type = "string", ui = "select", options = ["primary", "secondary"], default = "primary", label = "Style" }
new_tab = { type = "bool",   ui = "checkbox", default = false, label = "Open in new tab" }
```

`blocks/button.html`:
```html
<p class="button-wrap"><a class="btn btn-{{ attrs.variant }}" href="{{ attrs.url }}"{% if attrs.new_tab %} target="_blank" rel="noopener"{% endif %}>{{ attrs.text }}</a></p>
```

---

## Enabling plugins

By default every plugin in `<workspace>/plugins/` is loaded. To restrict to an
allowlist, set `enabled` in `lopress.toml`:

```toml
[plugins]
enabled = ["callout", "button"]   # by plugin `name`; empty/absent = load all
```

---

## Theme plugins

A plugin with `theme = true` supplies the site's theme instead of blocks: the Tera
template set (`layout.html`, `post.html`, `index.html`, `page.html`, `tag.html`) and a
stylesheet. A default theme ships with the binary, so fresh sites render without
installing anything. (Theme authoring is documented separately.)

---

## On-disk format

Comment-container blocks (HTML-template and markdown-template-form) persist as a
Gutenberg-style HTML-comment delimiter pair, with the attribute values as JSON in the
opening delimiter:

```markdown
<!-- lopress:callout {"variant":"note","title":"Heads up","body":"Be **careful**."} -->
<!-- /lopress:callout -->
```

Opened in any markdown tool the prose renders normally and the block appears as inert
comments; inside lopress the editor round-trips the same file without drift.

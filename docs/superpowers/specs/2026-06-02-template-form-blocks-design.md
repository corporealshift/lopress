# Template-Form Blocks — Declarative Form + Tera Markdown Template

**Date:** 2026-06-02
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (plugin capability model, comment-container blocks), `docs/superpowers/specs/2026-06-01-image-block-design.md` (base plugin block pattern)

---

## 1. Background

lopress plugins already ship **HTML** templates via the `template` field on
`BlockDecl` in `crates/lopress-plugin/src/manifest.rs`. A plugin author declares
form fields (`attrs`) and an HTML template; the editor renders an attr form, and
lopress interpolates the field values into the template at build time, producing
HTML output. This is the *HTML-template comment block* — the block persists as a
comment-container (`<!-- lopress:x {"attrs"} -->…body…<!-- /lopress:x -->`), where
the body between the delimiters is the author's nested markdown and the HTML
`template` wraps it at build time using `attrs` + `inner_html`.

The motivating gap is **declarative form + markdown template**: a plugin author
wants to ship a Tera *markdown* template whose interpolated result flows through
lopress's existing markdown→HTML pipeline. This lets form field values themselves
contain markdown (e.g. a textarea holding `Loves **Rust**` renders bold), keeps
output consistent with the site's prose rendering, and avoids forcing authors to
write HTML. Crucially, it does this **without** embedding a code runtime — no
filesystem access, no network, no site/post data. Just form values interpolated
into a Tera markdown template.

This rides on lopress's existing comment-container block model. The only genuinely
new behavior is "interpolate a *markdown* template with attrs" instead of "render
an *HTML* template with attrs."

---

## 2. Scope

- A **new class of plugin block** called a *template-form block*: a declarative
  form bound to a Tera markdown template.
- Three manifest additions to `crates/lopress-plugin/src/manifest.rs`:
  `BlockDecl.markdown_template`, `ui = "textarea"`, and optional `label`/`help`
  on `AttrDecl`.
- Editor form extension to render a textarea widget and use `label`/`help`.
- One new branch in `crates/lopress-build/src/render.rs`: load and interpolate
  a markdown template with Tera, feed the result through the existing md→HTML
  pipeline, emit the result.
- Persisted form values only; empty body; interpolated fresh at render time.
- **Out of scope:** inline in-pane render preview (webview covers it), any
  dynamic-query feature (e.g. posts-by-tag), any on-disk markdown format change.

---

## 3. Concept & Data Flow

A template-form block **is** a standard comment-container plugin block: it gets
`PluginMeta`, appears in the inserter/slash menu like any plugin block, and
round-trips through the existing plugin path.

The plugin author declares form fields and ships a Tera markdown template. Form
values are the **only** persisted state. On disk the block is a comment-container
with the form values as JSON in the opening delimiter and an **empty body**
between the delimiters:

```
<!-- lopress:author-bio {"name":"Jane","spoiler":true} -->
<!-- /lopress:author-bio -->
```

Render flow (both build output and the live-preview webview):

```
attrs ──▶ Tera(markdown_template) ──▶ markdown string ──▶ existing md→HTML ──▶ page
```

Because only the form values are persisted, the output is regenerated from
template + attrs on every render. Editing a field OR updating the plugin's
template both take effect on the next rebuild. There is no cached or persisted
body to go stale.

---

## 4. Plugin Author Surface (Manifest)

Three additions to the existing manifest machinery in
`crates/lopress-plugin/src/manifest.rs`:

### 4.1 `BlockDecl.markdown_template: Option<String>`

A Tera template path relative to the plugin directory (e.g. `"blocks/author-bio.md"`).
Its presence is what marks a block as a template-form block. It is **mutually
exclusive** with the existing HTML `template` field — declaring both is a load
error.

### 4.2 `ui = "textarea"`

A new value for the `ui` hint on `AttrDecl`, for multi-line text. The existing
`"text"` (one-line) and `"checkbox"` values are unchanged.

### 4.3 Optional `label` and `help` on `AttrDecl`

A human-friendly field caption (`label`) and a hint/description string (`help`).
When absent, the form falls back to the attr key as the label.

### Field-type mapping

| Kind              | `type`   | `ui`       |
|-------------------|----------|------------|
| One-line text     | `"string"` | `"text"` |
| Checkbox          | `"bool"`   | `"checkbox"` |
| Multi-line text   | `"string"` | `"textarea"` |

The existing `required` and `default` attr fields are honored by the form.

---

## 5. Editor Authoring Experience

A template-form block is a normal comment-container plugin block, so it already
receives `PluginMeta` and is rendered by the existing **plugin block view**
(header strip + attr form), in
`crates/lopress-editor/src/ui/blocks/plugin.rs`.

Work in the editor:

- Extend the attr form to render a multi-line **textarea** widget for
  `ui = "textarea"` (today it handles `text`/`checkbox`/etc.).
- Use the new `label`/`help` values in the form when present; otherwise fall back
  to the attr key.
- The block's in-pane editing surface is the **form only** for the MVP. The
  rendered markdown result is shown in the **live-preview webview**, which
  already rebuilds on save — the same way every other block's final rendered
  appearance is shown today.
- An inline rendered preview *inside the editor pane* is a deliberate later
  enhancement and is **out of scope** for this spec.

---

## 6. Rendering (Preview + Build)

One new branch in the build's block-render path
(`crates/lopress-build/src/render.rs`): when a comment block's plugin `BlockDecl`
carries `markdown_template`, load and interpolate that template with Tera, using
the block's attrs as the template context, producing a markdown string; then feed
that markdown string through the existing markdown→HTML pipeline and emit the
result.

- The Tera context is **strictly the form values (attrs)** — no filesystem access,
  no network access, no site/post data.
- Blocks that use the existing HTML `template` field are completely unchanged.
- Preview needs no separate path: the live-preview webview shows the build output,
  so the single build-side render branch covers both build and preview.

---

## 7. Worked Example

`plugins/author-bio/plugin.toml`:

```toml
name = "author-bio"
version = "0.1.0"

[[blocks]]
name = "lopress:author-bio"
markdown_template = "blocks/author-bio.md"

[blocks.attrs]
name    = { type = "string", ui = "text",     required = true, label = "Author name" }
bio     = { type = "string", ui = "textarea",                  label = "Short bio" }
spoiler = { type = "bool",   ui = "checkbox",  default = false, label = "Mark as spoiler" }
```

`plugins/author-bio/blocks/author-bio.md`:

```markdown
**About {{ name }}**

{{ bio }}
{% if spoiler %}
> ⚠️ Contains spoilers.
{% endif %}
```

---

## 8. Testing

### Manifest parsing

- New `markdown_template` field, new `ui = "textarea"` value, and optional
  `label`/`help` on `AttrDecl` parse correctly.
- The `markdown_template`-and-`template`-both-set case is a load error.

### Round-trip

- A template-form block round-trips byte-identically through `from_core`/`to_core`
  (it is a standard comment-container block with attrs and empty body; unknown-to-registry
  blocks still fall back to `Opaque` and round-trip verbatim).

### Build render

- A block with `markdown_template` interpolates its attrs through Tera, renders the
  resulting markdown to HTML, and emits expected output.
- A checkbox attr drives a `{% if %}` conditional.
- A text/textarea field value that itself contains markdown renders as markdown.

### End-to-end (control interface)

Via the `127.0.0.1:7878` control server (the `driving-lopress-editor` capability):
insert a template-form block, fill the form fields, save, and confirm the live
preview shows the interpolated output.

---

## 9. Implementation Order

1. `crates/lopress-plugin`: add `markdown_template: Option<String>` to `BlockDecl`;
   add `ui = "textarea"` value; add optional `label`/`help` to `AttrDecl`; add the
   mutual-exclusivity check (`markdown_template` + `template` → load error).
2. `crates/lopress-editor`: extend the attr form to render a textarea widget for
   `ui = "textarea"`; use `label`/`help` in the form when present.
3. `crates/lopress-build/src/render.rs`: add the new render branch for
   `markdown_template` — load template, interpolate with Tera (attrs as context),
   feed through md→HTML pipeline.
4. Tests: manifest parsing, round-trip, build render, and final e2e via control
   server.

---

## 10. Non-Goals / Scope Boundary

- **No code/script runtime.** Tera template interpolation only; the template context
  is strictly the form values — no filesystem, no network, no site/post data.
- **No on-disk markdown format change.** A template-form block is a standard
  comment-container block; round-trip is handled by the existing plugin path.
- **No inline in-editor render preview** in the MVP (the webview covers it).
- **Not the posts-by-tag / dynamic-query feature.** That is a separate future design.

---

## 11. Decisions

### Persistent re-editable block, not a one-shot snippet expander

Chosen: the block stays in the document as a comment-container the user can reopen
and re-edit via its form. Rejected: a one-shot wizard that expands into ordinary
markdown blocks and disappears (values not retained, not re-editable).

### Persist form values only; no persisted body

Chosen: store only the JSON form values in the comment delimiter, empty body,
interpolate fresh at render time. Single source of truth, plugin template updates
auto-apply, no stale cache. Rejected: persisting the generated markdown between the
delimiters (would be readable/portable in other markdown tools, but introduces a
generated-body cache that can drift and that hand-edits would silently lose). The
accepted tradeoff of "values only" is that the block is opaque (no readable prose)
when the `.md` is opened in a non-lopress tool.

### Tera template engine, not plain substitution

Chosen: reuse the Tera engine lopress already depends on, giving `{{ field }}`
substitution plus `{% if checkbox %}…{% endif %}` conditionals and loops. This is
consistent with the existing HTML-template system and makes checkbox fields actually
useful (toggling whether a line appears). Rejected: plain `{{ field }}`-only
substitution with no logic (a checkbox could then only insert literal text like
"true").

### Markdown template, not direct-to-HTML

Chosen: the plugin ships a Tera *markdown* template; the interpolated result goes
through the existing md→HTML pipeline. Rationale: (a) plugin authors write markdown
instead of HTML, and output stays consistent with how the rest of the site's prose
renders; (b) form field values can themselves contain markdown that renders (a
textarea holding `Loves **Rust**` renders the bold). Rejected: interpolating
straight into an HTML Tera template (the existing `template` field) — that would
shrink the feature to merely adding textarea/label/help to today's attr-form, force
authors to write HTML, and escape field values to plain text unless a per-field
markdown filter were added. It was explicitly acknowledged that the markdown step is
not *structurally* required once the body isn't persisted, but the two authoring
benefits justify keeping it.

### `markdown_template` mutually exclusive with `template`

A block is either an HTML-template comment block (existing) or a markdown-template-form
block (new), never both; declaring both is a load error. The presence of
`markdown_template` is the sole signal that distinguishes the new block class.

### Form-only editing surface for MVP; inline in-pane render preview deferred

Chosen: the editor pane shows the form; the rendered result is shown only in the
live-preview webview (as with all other blocks). Rejected for now: an inline rendered
preview inside the editor pane (a deliberate later enhancement).

---

## 12. Open Questions for Claude

None. All design decisions listed above are resolved. The spec covers every section
with concrete decisions and no placeholders.

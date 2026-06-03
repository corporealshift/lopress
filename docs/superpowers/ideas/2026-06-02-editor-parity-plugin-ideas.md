# Editor Parity — Plugin & Block Ideas

**Date:** 2026-06-02
**Author:** Claude (autonomous exploration, commissioned by Kyle)
**Status:** ideas — exploration, not yet specced

> Goal: map lopress's editor against the WordPress **Gutenberg** block editor (and
> common docs-editor conventions like Notion / MkDocs admonitions), find the gaps, and
> propose a prioritized set of plugins/features that close them. Each idea is tagged
> with the **existing lopress capability it rides on**, so effort estimates are grounded
> in what already works rather than greenfield guesses.

---

## 1. How lopress blocks work today (grounding)

Surveyed from the live tree at `explore/editor-parity` (stacked on `feat/template-form-blocks`):

- **Native built-in blocks** (`crates/lopress-build/src/render.rs` match arms + editor
  `BlockKind`): paragraph, heading, quote, code, list, image, `lopress:more` (read-more
  marker). These have first-class editor widgets and direct HTML renderers.
- **Base plugins** (`base_plugins/{list,code,more,image}/manifest.toml`, embedded via
  `include_str!`): dogfood the plugin system — `builtin = true`, claim a `native` core
  type, ship an `editor` name, no HTML template (the editor renders them).
- **Comment-container plugin blocks** — two flavors, both persisted as
  `<!-- lopress:NAME {json-attrs} -->…body…<!-- /lopress:NAME -->`:
  1. **HTML-template** (`BlockDecl.template`): attrs + `inner_html` → a Tera **HTML**
     template at build time.
  2. **Markdown-template-form** (`BlockDecl.markdown_template`, new on
     `feat/template-form-blocks`): attrs → a Tera **markdown** template → the existing
     md→HTML pipeline. Declarative form (text / textarea / checkbox / label / help),
     values-only persistence.
- **Editor surface for plugin blocks:** `crates/lopress-editor/src/ui/blocks/plugin.rs`
  renders a header strip + an attr form (text / number / select / checkbox / textarea).
- **Rendering of unknown blocks:** unknown `lopress:*` → `Opaque` in the editor (round-trips
  verbatim), `<!-- missing plugin -->` in the build. Fail-safe.

### The foundational gap: no dynamic inserter

`crates/lopress-editor/src/ui/slash_menu.rs` offers a **hardcoded** `SlashChoice` enum:
Paragraph, Headings 1–3, Code, Lists, Image, Read more. **Registered plugin blocks are not
listed anywhere in the UI.** A callout/button/embed/author-bio block can be *rendered* and
*round-tripped*, but a user cannot *insert* one except by hand-editing the markpdown comment
delimiters. **Every block-plugin idea below is gated on fixing this** — it is the single
highest-leverage parity feature. See §3 Tier 0.

---

## 2. Gutenberg block coverage matrix

✅ shipped · 🟡 partial · ❌ missing. "Rides on" = the lopress capability that makes it cheap.

| Gutenberg block | lopress | Rides on | Notes |
|---|---|---|---|
| Paragraph | ✅ | native | |
| Heading | ✅ | native | |
| List (un/ordered) | ✅ | native (base plugin) | nested lists 🟡 |
| Quote | ✅ | native | |
| Code | ✅ | native (base plugin) | |
| Image | ✅ | native (base plugin) | responsive srcset, captions |
| Read more / page break | ✅ | base plugin | excerpt boundary |
| Pullquote | ❌ | md-template-form | trivial |
| Table | ❌ | native (GFM via pulldown-cmark) + editor grid | medium |
| Details / Accordion | ❌ | HTML-template (inner body) | `<details><summary>` |
| Verse / Preformatted | 🟡 | code | low priority |
| Footnotes | ❌ | core + build | medium |
| Button(s) | ❌ | HTML-template | trivial |
| Separator / Divider | ❌ | native (`Event::Rule` is currently dropped) | trivial |
| Spacer | ❌ | HTML-template | trivial |
| Audio | ❌ | HTML-template | trivial |
| Video (self-hosted) | ❌ | HTML-template | trivial |
| File / Download | ❌ | HTML-template | trivial |
| Embed (YouTube/Vimeo/X/…) | ❌ | HTML-template (URL→iframe) | easy; oEmbed later |
| Cover (bg image + text) | ❌ | HTML-template (inner body) | medium |
| Media & Text | ❌ | HTML-template (inner body) + layout | medium |
| Columns / Group / Row | ❌ | needs **inner blocks** | hard (see §4) |
| Gallery | ❌ | image list | medium |
| Table of Contents | ❌ | build-side heading scan + anchors | medium |
| Custom HTML | ❌ | new "raw html" block | easy (sanitization caveat) |
| Social icons | ❌ | HTML-template | easy |
| Latest posts / query loop | ❌ | **deferred** dynamic-query design | out of scope here |

**Admonitions / callouts** (Notion, MkDocs, Docusaurus — not core Gutenberg but ubiquitous
and high-value): ❌, rides on md-template-form. A `callout` fixture already exists under
`crates/lopress-build/tests/fixtures/with-plugin/plugins/callout/`.

---

## 3. Prioritized roadmap

### Tier 0 — The unlock (foundational editor UX)

**0.1 Dynamic plugin-block inserter.** Make the slash menu (and/or a `+` button) list every
registered comment-container plugin block from the `PluginRegistry`, with a label + optional
icon/description from the manifest. Selecting one dispatches `InsertAfter` with a fresh
`Opaque` block carrying the plugin's default attrs and empty body. *Without this, none of the
block plugins below are reachable in the GUI.* Touches `slash_menu.rs`, the action_sink/registry
wiring, and `BlockDecl` (optional `title`/`description`/`icon` manifest fields for the menu).
**This should be built first.** Effort: medium.

**0.2 Slash-menu fuzzy filter.** Type `/cal` → narrows to "Callout". The menu currently shows
a fixed list with no text filtering. Small, high-feel-quality. Effort: low. (Natural follow-on
to 0.1, since the plugin list can grow long.)

### Tier 1 — Easy, high-value blocks (each a self-contained plugin; needs Tier 0 to be insertable)

These are mostly **manifest + template files** — little or no Rust — once Tier 0 lands. Great
candidates to ship as a *bundled default plugin set*.

1. **Callout / Notice** — md-template-form. Variants info/tip/warning/danger via a `select`
   attr; body via textarea (renders markdown). Highest value (docs authoring). Fixture exists.
2. **Button** — HTML-template. Attrs: `text`, `url`, `variant` (select), `new_tab` (checkbox).
3. **Embed** — HTML-template. Attr: `url`; map known hosts (YouTube/Vimeo) to a responsive
   `<iframe>` wrapper; generic URL → link card fallback. (Pure URL→iframe; no network/oEmbed.)
4. **Details / Accordion** — HTML-template with `inner_html`. Attr: `summary`; body is the
   collapsible content. `<details><summary>{summary}</summary>{inner_html}</details>`.
5. **Pullquote** — md-template-form. Attrs: `quote` (textarea), `cite`.
6. **Separator** — native: stop dropping `Event::Rule`; emit a `separator` block →`<hr>`.
   Add a slash entry. (Small core+render+editor change, not a plugin.)
7. **Spacer** — HTML-template. Attr: `height` (number) → a sized div. (Web-only; ignored in
   prose exports.)
8. **Audio / Video / File** — three thin HTML-template plugins over `<audio>`/`<video>`/`<a download>`.

### Tier 2 — Medium effort, high value

9. **Table** — the biggest real parity gap. pulldown-cmark already parses GFM tables; add a
   `table` core block (header row + rows of inline cells), a render arm, and an editor grid
   widget (add/remove row/column, per-cell inline editor). Effort: medium-high.
10. **Table of Contents** — build-side: scan headings, inject `id` anchors, emit a nav list.
    Needs a render-time pass over sibling blocks (more than a per-block template). A `[toc]`
    marker block + build support. Effort: medium.
11. **Media & Text** — HTML-template with inner body + an image attr; two-column responsive.
12. **Gallery** — a block holding an ordered list of image refs; reuse the image import +
    responsive pipeline; editor widget to add/reorder. Effort: medium.

### Tier 3 — Editor UX polish (parity of *feel*, not blocks)

13. **Block toolbar**: duplicate, move up/down, delete, and a "transform to…" menu. Some of
    this exists (Opaque fallback card has a toolbar) — generalize it.
14. **Markdown autoformat input rules**: `## ` → H2, `> ` → quote, `- `/`1. ` → list,
    ` ``` ` → code, `---` → separator. Big perceived-speed win; mirrors Gutenberg/Notion.
15. **Drag-to-reorder** blocks. Effort: medium-high (floem DnD).
16. **Copy / paste / duplicate** blocks across documents.

### Tier 4 — Structural (largest, enabling)

17. **Inner blocks / nesting** — the prerequisite for Columns, Group, Cover-with-blocks,
    Media&Text-with-blocks. lopress already nests (quote/list children), but there's no
    general container-with-arbitrary-children plugin block. This is the big architectural
    lift that unlocks Gutenberg's layout family. Worth a dedicated design.

---

## 4. Cross-cutting design notes

- **Manifest metadata for the inserter (Tier 0 dependency):** add optional
  `title`, `description`, `icon`, and `category` to `BlockDecl` so the inserter can present
  plugin blocks nicely and group them (Text / Media / Embeds / Layout). Backward-compatible
  (`#[serde(default)]`), mirrors the `markdown_template`/`label`/`help` additions.
- **Bundled default plugins:** Tier 1 argues for a shipped `plugins/` set (callout, button,
  embed, details, …) loaded by default, the way `base_plugins/` are — so a fresh `lopress new`
  site has a Gutenberg-ish palette out of the box. Decide: embed like base plugins, or scaffold
  into each new site's `plugins/` dir (the latter lets users edit templates).
- **Security for Custom HTML / Embed:** raw-HTML and iframe blocks need an explicit trust
  decision (sanitize? allowlist hosts? mark posts as trusted?). Don't ship Custom HTML without
  it.
- **Inner-body blocks vs values-only:** Details/Cover/Media&Text want a *persisted inner body*
  (HTML-template + `inner_html`), unlike the values-only template-form block. Both models
  coexist; pick per block by whether the body is authored prose (inner body) or pure field
  data (values-only).

---

## 5. Suggested build order (what this exploration will actually attempt)

1. **Tier 0.1 dynamic inserter** + manifest metadata (`title`/`description`/`category`). The unlock.
2. **Tier 1 callout** (md-template-form) — first plugin the inserter surfaces; doubles as the
   inserter's e2e.
3. **Tier 1 button** (HTML-template) — proves the inserter works for the HTML-template flavor too.
4. Then, as budget allows, **separator** (native) and **embed** (HTML-template).

Anything not reached is left as a specced/planned follow-up. The matrix above is the backlog.

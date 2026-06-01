# "Read More" Marker Block & Home-Page Excerpts

**Date:** 2026-06-01
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (the plugin capability model this builds on), `docs/architecture.md` (§6 plugin architecture, §8 build pipeline)

---

## 1. Background

The home page (`index.html`) currently shows, per post, a title link, the date, and the
front-matter `description` as a one-line "excerpt" (see
`crates/lopress-theme/assets/default-theme/templates/index.html`). There is no
content-derived excerpt: an author must hand-write a `description` to get any teaser
text, and that teaser is divorced from the post body.

This spec adds a **"Read more" marker** — a block an author drops into a post to mark
where the teaser ends. Everything before the marker becomes the post's excerpt on the
home page (rendered HTML, followed by a "Read more →" link to the full post). On the
post's own page the marker is invisible and the full content always renders.

This mirrors the long-standing WordPress `<!--more-->` convention, adapted to lopress's
`lopress:`-prefixed comment-container block format so it round-trips cleanly and stays
portable in plain markdown tools.

---

## 2. Scope

- A new base plugin block type `lopress:more` (the marker), embedded like `list` and
  `code`.
- A slash-menu entry to insert it, with **one-per-post** enforcement.
- A dedicated, minimal editor rendering: a slim full-width "Read more" divider — **not**
  the generic plugin attr-form chrome.
- Build-side excerpt extraction: render the blocks **before** the first marker to HTML,
  expose it on `PostSummary`, and show it on the home page with a "Read more →" link,
  falling back to the front-matter `description` when no marker is present.
- The marker renders to **nothing** on the post's own page.

### Non-goals

- No effect on the RSS/Atom feed — the feed keeps full content (see §9).
- No effect on tag archive pages in this spec (the tag-listing block was descoped).
- No truncation/collapse UI on the full post page — the marker only governs listings.
- No multiple markers per post — exactly zero or one.
- No per-post override of the "Read more" link text in this spec (a future `[site]`
  setting could add it).

---

## 3. On-Disk Representation

The marker is an **empty comment-container block**:

```markdown
A teaser paragraph that shows on the home page.

<!-- lopress:more -->
<!-- /lopress:more -->

The rest of the post, shown only on its own page.
```

This requires **no parser or serializer changes**. The delimiter scanner already emits
an `Open`/`Close` pair for an empty `lopress:` container (verified by
`crates/lopress-core/src/delimiter.rs` test `open_without_attrs_parses_cleanly`), and
the comment-container round-trip is the existing, tested path for blocks like
`lopress:callout`. The parsed core block is
`Block { type: "lopress:more", attrs: {}, children: [], text: None }`.

Because it carries no attributes and no children, the marker survives a verbatim
round-trip even if the `more` base plugin were ever absent (it would degrade to an
`Opaque` block and still serialize back to the same comment pair).

---

## 4. The `more` Base Plugin

A new embedded base plugin, mirroring `base_plugins/code` and `base_plugins/list`:

`base_plugins/more/manifest.toml`:

```toml
# Built-in "base" plugin: the read-more marker. Embedded at compile time via
# include_str! — see PluginRegistry::load_base_plugins.
name    = "lopress-more"
version = "0.1.0"

[[blocks]]
name    = "lopress:more"
editor  = "more"
builtin = true
```

Notes:

- **No `native` field.** The marker is a comment-container block, not a native markdown
  construct, so it serializes via the existing `plugin_block_to_core` comment path.
- **No attrs.** It is a pure marker.
- **`builtin = true`** suppresses the generic plugin chrome (header strip + attr form),
  per the existing rule in `crates/lopress-editor/src/ui/blocks/plugin.rs`
  (`if meta.builtin { … }`). The marker renders only its custom editor widget.

`load_base_plugins()` (in `crates/lopress-editor/src/state.rs`) is extended to seed this
third base plugin alongside `list` and `code`.

---

## 5. Editor Model & Classification

`lopress:more` flows through the existing `block_from_core` `other` arm
(`crates/lopress-editor/src/model/from_core.rs`): it is not native, so
`registry.block("lopress:more")` matches and `plugin_block_from_core` runs. The `editor`
key is `"more"`, which is not one of the `heading`/`code`/`list` arms, so it falls to the
`_` arm and produces `(BlockKind::Paragraph, BlockBody::Inline(empty))` with
`plugin: Some(PluginMeta { editor: Some("more"), builtin: true, … })`.

The `(kind, body)` pair is a placeholder only — the marker's rendering is driven entirely
by its `editor` key (§6), and `to_core` reconstructs it from `PluginMeta` (the empty
inline body yields no children, producing the empty comment pair). No new `BlockKind`
variant is introduced.

---

## 6. Editor Rendering

A new editor widget is registered under the key `"more"` in
`crates/lopress-editor/src/ui/blocks/editor_registry.rs` (`editor_for`), so
`render_body` in `plugin.rs` dispatches to it via the existing registry path.

The widget renders a **slim, full-width horizontal divider** with a small centered label
("Read more"), visually distinct from content blocks (e.g. a dashed rule). It:

- ignores `block.body` entirely (the body is the empty placeholder);
- is selectable/focusable so the block can be selected and deleted via the normal block
  controls (clicking it sets the focused block so the toolbar's Delete applies);
- shows no attr form and no header strip (guaranteed by `builtin = true`).

Because `builtin` blocks suppress chrome and the `more` widget renders no editable body,
the marker appears as a clean divider in the canvas.

---

## 7. Insertion & One-Per-Post Enforcement

### Slash-menu insertion of a plugin block

The slash menu (`crates/lopress-editor/src/ui/slash_menu.rs`) today offers only built-in
`BlockKind` variants and converts the current block via `ChangeType`. The marker is a
plugin block, so a new insertion path is needed.

`slash_menu_items()` is extended from returning `(&str, BlockKind)` to a small item enum
that can represent **either** a built-in `BlockKind` conversion (existing behavior) **or**
a plugin-block insertion identified by block name (e.g. `"lopress:more"`). When a
plugin-block item is chosen, the editor constructs an `EditorBlock` for that registry
block and inserts it via `BlockAction::InsertAfter` (replacing the current empty
paragraph when the menu was triggered in one, matching today's slash-menu UX).

A shared constructor — `new_plugin_block(registry, block_name) -> Option<EditorBlock>` —
builds the `EditorBlock` from the registry `BlockDecl` (stamping `PluginMeta` with
default attrs and the empty placeholder body), reusing the same `PluginMeta` assembly
that `plugin_block_from_core` performs.

> **Shared infrastructure note.** This slash-menu plugin-block insertion path and the
> `new_plugin_block` constructor are also required by the Image Block spec
> (`2026-06-01-image-block-design.md`). Whichever feature is implemented first builds
> this seam; the second reuses it. It is described self-containedly in both specs.

### One-per-post

A post may contain **at most one** `lopress:more` marker. Enforcement is twofold:

1. **Slash menu** omits (or disables) the "Read more" entry when the current document
   already contains a `lopress:more` block. The menu is built per-invocation, so it can
   inspect the live document.
2. **`apply()` guard** (`crates/lopress-editor/src/actions.rs`): the chokepoint rejects
   an `InsertAfter` (and a `ChangeType`, if ever routed there) that would introduce a
   second `lopress:more` block — it returns `None` (no-op, nothing recorded on the undo
   stack), the same convention used for other rejected actions. This guards against the
   debug control interface and any non-menu path, not just the GUI.

Deleting the marker is unrestricted (normal block delete).

---

## 8. Build: Excerpt Extraction

### Rendering blocks before the marker

A new helper in `crates/lopress-build/src/render.rs`:

```rust
/// Render the blocks that precede the first `lopress:more` marker to HTML.
/// Returns `None` when the document contains no marker.
pub fn render_excerpt(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<Option<String>, BuildError>
```

It scans `doc.blocks` for the first block whose `type == "lopress:more"`. If none is
found, it returns `None`. Otherwise it renders every block **before** the marker using
the same per-block logic as `render_body` (factor the block loop so `render_body` and
`render_excerpt` share it) and returns `Some(html)`.

### Marker renders to nothing on the full page

In `write_block`, `lopress:more` must produce no output. The existing
`custom if custom.starts_with("lopress:")` path already routes to `render_custom`, and a
base plugin with no `template` emits only its (empty) inner HTML — so the marker already
renders to nothing. For clarity and to avoid depending on that incidental behavior, add
an explicit early arm: `"lopress:more" => { /* marker: no output */ }`.

### `PostSummary.excerpt_html`

`PostSummary` (in `crates/lopress-theme/src/context.rs`) gains:

```rust
pub excerpt_html: Option<String>,
```

`post_summaries` (in `crates/lopress-build/src/pages.rs`) is the place excerpts are
computed. Because rendering needs the registry and the shared Tera, its signature grows
to accept them:

```rust
pub fn post_summaries(
    posts: &[DiscoveredPost],
    registry: &PluginRegistry,
    tera: &Tera,
) -> Vec<PostSummary>
```

(The unused `_base_url` parameter is dropped.) For each non-draft post it calls
`render_excerpt` and stores the result in `excerpt_html`. Both call sites —
`render_all` and `build` (in `crates/lopress-build/src/build.rs`) — are updated to pass
`registry` and the shared `tera`. A render failure for one post's excerpt is recorded as
a `PageFailure` (consistent with body-render failure handling) and that post's
`excerpt_html` is left `None`.

### Cache interaction

Excerpt content is part of the rendered index, which is already regenerated whenever
`post_set_changed` flips or on a forced full build. The excerpt is derived from the post
body; a body edit changes the source hash and re-renders the post, and the index is
regenerated when aggregate-visible metadata changes. **A body-only edit that changes the
excerpt but not title/date/tags/draft would not currently flip `post_set_changed`.** To
keep the home-page excerpt correct, `aggregate_metadata_changed` (in `pages.rs`) must
also treat a change in excerpt as aggregate-visible. The simplest robust approach: store
a hash (or the rendered excerpt) on the `PageEntry` and compare it. This spec adds an
`excerpt_hash: Option<String>` to `PageEntry` and includes it in the
`aggregate_metadata_changed` comparison, so editing teaser content regenerates the index.

---

## 9. Theme: Home-Page Excerpt Display

`index.html` is updated so each post entry prefers `excerpt_html` over `description`:

```jinja
{% if p.excerpt_html %}
  <div class="excerpt">{{ p.excerpt_html | safe }}</div>
  <a class="read-more" href="{{ p.url }}">Read more →</a>
{% elif p.description %}
  <p class="excerpt">{{ p.description }}</p>
{% endif %}
```

`excerpt_html` is already escaped/structured HTML from the renderer, so it is emitted
with the `safe` filter. The `description` fallback path is unchanged. A small
`.read-more` style is added to `theme.css`.

The RSS/Atom feed (`crates/lopress-build/src/feed.rs`) is **unchanged** — it continues to
use whatever it uses today (full content / description). The marker does not affect the
feed.

---

## 10. Testing

### Core round-trip

- A document containing `<!-- lopress:more -->` / `<!-- /lopress:more -->` round-trips
  byte-identically through parse → serialize (add to
  `crates/lopress-core/tests/roundtrip.rs`).
- The editor round-trip (`crates/lopress-editor/tests/from_to_core_tests.rs`): a doc with
  a marker survives `from_core` → `to_core` as the same empty comment pair, with base
  plugins loaded.

### Excerpt extraction

- `render_excerpt` returns `None` when no marker is present.
- `render_excerpt` returns only the HTML of blocks before the marker, excluding the
  marker and everything after it.
- A post with a marker yields `PostSummary.excerpt_html = Some(_)`; one without yields
  `None`.
- The full-post render (`render_body`) emits **no** output for the marker block.
- `aggregate_metadata_changed` flips when only the excerpt content changes.

### Editor behavior

- The slash menu omits "Read more" when a marker already exists.
- `apply()` rejects a second `lopress:more` insertion (returns `None`).
- Manifest parsing of `base_plugins/more/manifest.toml` (round-trips through
  `parse_manifest_str`).

### End-to-end (control interface)

Using the debug control server on `127.0.0.1:7878` (the `driving-lopress-editor`
capability): open a post, insert a "Read more" marker via the slash menu, confirm the
divider renders, confirm a second insertion is refused, save, and confirm the saved
markdown contains exactly one `lopress:more` comment pair and that the built
`index.html` shows the pre-marker content plus a "Read more →" link.

---

## 11. Implementation Order

1. `base_plugins/more/manifest.toml`; seed it in `load_base_plugins()`.
2. `editor_registry`: register the `"more"` editor widget (the divider view).
3. Slash-menu plugin-block insertion: item enum + `new_plugin_block` constructor +
   `InsertAfter` wiring (shared seam — see §7).
4. One-per-post enforcement: slash-menu omission + `apply()` guard.
5. `render.rs`: factor the block loop; add `render_excerpt`; add the explicit
   `lopress:more` no-output arm.
6. `PostSummary.excerpt_html`; `post_summaries` signature change; update `render_all`
   and `build` call sites.
7. `PageEntry.excerpt_hash` + `aggregate_metadata_changed`.
8. `index.html` + `.read-more` CSS.
9. Tests (core round-trip, excerpt unit tests, editor behavior, manifest parse, e2e).

---

## 12. Decisions

### Marker as an empty `lopress:more` comment container, not a new core block type

Rejected a bespoke `<!--more-->` token (would need new parser/serializer code and breaks
the uniform `lopress:` convention) and a new core `Block` variant (unnecessary — the
existing empty-container path already round-trips). The empty container needs zero
parser/serializer changes and degrades gracefully to `Opaque` if the plugin is absent.

### Base plugin, not a `BlockKind` variant

Consistent with the block-types-as-plugins direction (`list`, `code`). Avoids touching
the `BlockKind` enum and the `apply`/action machinery. The marker's behavior lives in a
registered editor widget keyed on the manifest `editor` field.

### Excerpt = rendered HTML of pre-marker blocks (not plain text)

Chosen so the home-page teaser preserves formatting (bold, links, lists) exactly as
authored. Plain-text extraction was rejected as lossy. The full marker-controls-fold
alternative (truncating the post's own page) was rejected — the marker governs listings
only.

### One-per-post enforced at both the slash menu and `apply()`

The menu gives immediate UX feedback; the `apply()` guard makes the invariant hold for
every insertion path (including the debug control interface), not just the GUI.

### `post_summaries` gains `registry`/`tera`; excerpt change is aggregate-visible

Excerpts are derived from the body and surface on the index, so they belong with the
other summary fields and must trigger index regeneration. Tracking an `excerpt_hash` on
the cache entry keeps incremental builds correct when only teaser content changes.

---

## 13. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no placeholders.

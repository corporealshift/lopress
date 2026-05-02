# Editor Migration to Floem — Design Spec

**Date:** 2026-05-02
**Status:** draft, awaiting review
**Replaces:** the egui editor in `crates/lopress-editor` and the `eframe` app shell in `src/main.rs`

## 1. Goals and Non-Goals

### Goals (single ship, no v1/v2 phasing)

- Block-level WYSIWYG editor in pure Rust on Floem. Inline bold, italic, links, and inline-code render as styled text in place — no visible markdown chars in the edit surface.
- Slash-command block insertion (`/heading`, `/code`, `/list`, etc.) from within any block.
- Block toolbar above the focused block: type changer, B/I/code/link buttons, delete.
- Drag handles for block reorder, hover-revealed.
- Multi-block keyboard selection: Shift+arrow extends across block boundaries; multi-block delete, copy/cut/paste, and inline-toggle operations all work.
- Plugin block types render with a built-in editor kind (declared by the plugin) plus an auto-generated form for the plugin's attrs. See section 12, "Plugin Block Rendering."
- Same on-disk format as today (markdown via `lopress-core::serialize`). No changes to `lopress-core`, `lopress-build`, `lopress-serve`, `lopress-watch`, or `lopress-gui-host`.
- Mac/Linux/Windows all ship from `main` continuously through the existing CI release matrix.
- Persisted last-window-size and position.

### Non-Goals (deferred indefinitely, not v2)

- WYSIWYG editing of tables, footnotes, raw HTML, or embeds. These round-trip losslessly through the file but render as opaque placeholder cards in the editor.
- Plugin code hooks (WASM/dylib/native plugin code that the editor calls to render a fully custom block UI). Path 1 — declarative plugin blocks reusing built-in editor kinds — ships in v1. Path 2 is a separate future direction; see `docs/superpowers/ideas/2026-05-02-plugin-code-hooks.md`.
- Syntax highlighting as a built-in editor feature. Implemented as a plugin block type, not part of the core editor. The plugin declares `editor = "code"` to reuse the built-in code-block editor and provides its own HTML template for build-time syntax-highlighted output.
- Collaborative editing, comment threads, edit history.
- Automated GUI tests. Manual smoke checklist only.

### Deferred to a follow-up phase (not v1, but planned)

- **Undo/redo.** Action stack, coalescing rules, Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z bindings, captured selection states. Architectural foundation is preserved in v1 (every mutation is a discrete `BlockAction` going through a single `apply` chokepoint), so the follow-up phase wires up the stack and shortcuts without rewriting mutation sites.
- **UI zoom.** `Cmd/Ctrl+=` / `Cmd/Ctrl+−` / `Cmd/Ctrl+0` keyboard zoom and a persisted `ui_zoom` setting. Tiny but not zero — borderline trivial. Defer with the rest.

## 2. Architecture

### Workspace and binary layout

Single in-place replacement. No parallel binaries, no feature flag.

```
crates/
  lopress-core/          unchanged — canonical Block tree, parse, serialize
  lopress-build/         unchanged
  lopress-serve/         unchanged
  lopress-watch/         unchanged
  lopress-gui-host/      unchanged — framework-independent Session layer
  lopress-editor/        REWRITTEN — Floem-based block editor + inline runs model
src/
  main.rs                REWRITTEN — Floem app shell (replaces eframe shell)
```

The egui code is deleted at the start of the migration. Fallback if Floem turns out to be wrong: `git revert` to a recent egui commit. Not free but not catastrophic.

During the build-out there is no working GUI editor on `main`. This is acceptable because the user can edit markdown files directly while the rewrite proceeds.

### Floem dependency

Pin Floem to a known-good crates.io version in the workspace `Cargo.toml`. The exact version is selected by the first task of the implementation plan — at that point we read the latest Floem release notes, pick a recent stable version that compiles cleanly, and pin it. Bumps thereafter are deliberate and reviewed.

If a needed feature only exists on `main`, switch the single Floem entry to a git rev with a comment explaining why. Don't track Floem `main` by default.

For inline-markdown parsing, use `pulldown-cmark` (already in the lopress dependency tree). Hand-roll the inline serializer; the supported subset is small enough that a focused implementation is simpler than wrestling a general-purpose markdown emitter.

### Cross-platform stance

Mac, Linux, and Windows are all first-class. Floem is winit-based and Lapce ships production Windows binaries on the same stack, so no special Windows accommodation is expected. The existing CI release artifact matrix continues to produce all three platforms.

## 3. Document and Inline-Runs Model

The editor crate gets its own working representation. `lopress-core::Block` stays the canonical on-disk shape; the editor's model is a working copy that converts in/out at load and save.

```rust
struct EditorDoc {
    blocks: Vec<EditorBlock>,
    front_matter: FrontMatter,        // passes through unchanged
}

struct EditorBlock {
    id: BlockId,                      // stable ID for focus/DnD; not persisted
    kind: BlockKind,
    body: BlockBody,
    plugin: Option<PluginMeta>,       // populated for plugin-declared block types
}

struct PluginMeta {
    block_type_name: String,          // e.g., "lopress:codehighlight"
    attrs: serde_json::Map<String, serde_json::Value>,
    attr_decls: Vec<AttrDecl>,        // snapshot from PluginRegistry at load time
}

enum BlockKind {
    Paragraph,
    Heading(u8),                      // 1..=6
    Code { lang: String },
    List { ordered: bool },
    Opaque { type_name: String },
}

enum BlockBody {
    Inline(Vec<InlineRun>),           // paragraph, heading
    Code(String),                     // raw code, no inline runs
    List(Vec<ListItem>),              // nested items
    Opaque(serde_json::Value),        // raw lossless round-trip
}

struct ListItem {
    id: BlockId,
    runs: Vec<InlineRun>,
}

struct InlineRun {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,                       // inline code, monospace span
    link: Option<String>,             // url; None if not a link
}
```

### Loading

`lopress-gui-host::load_document` returns a `lopress_core::Block` tree. The editor crate has a `from_core` converter:

- `paragraph` / `heading` → parse `block.text` via `pulldown-cmark` inline-only mode → `Vec<InlineRun>`. Adjacent runs with identical flags are coalesced.
- `code_block` → `BlockBody::Code(text)`, language preserved on `BlockKind::Code { lang }`.
- `list` whose `list_item` children each contain only a single paragraph → `BlockBody::List(items)`, each item's paragraph text parsed into runs.
- `list` with at least one `list_item` containing anything more complex (nested lists, multiple children, code blocks, etc.) → the entire list becomes `BlockBody::Opaque` to preserve structure losslessly. The editor will render it as an opaque card; full editing requires editing the markdown directly.
- A block type matching a plugin in `PluginRegistry` → look up the plugin's `BlockDecl`, read its `editor` field. Populate `body` based on the editor kind (`"paragraph"` → `Inline`, `"code"` → `Code`, `"list"` → `List`, etc.; default `"paragraph"` if unset). Populate `plugin` with the block type name, attrs from `block.attrs`, and a snapshot of the plugin's `AttrDecl`s.
- Any other block type (no matching plugin in registry) → `BlockBody::Opaque` storing the original block's serialized JSON, with `BlockKind::Opaque { type_name }`. Lossless round-trip preserved.

### Saving

A `to_core` converter does the reverse:

- `Vec<InlineRun>` → markdown string by emitting `**text**`, `_text_`, `` `text` ``, `[text](url)` for runs whose flags are set. Adjacent runs with identical flags coalesced. Nested formatting (e.g. bold inside link) preserved.
- Plugin blocks (`plugin: Some(...)`) reconstructed as `Block { r#type: plugin.block_type_name, attrs: plugin.attrs, ..body-derived-fields }`. The body is serialized via the same path as a built-in block of the matching editor kind (e.g., a plugin block with `editor = "code"` serializes its `Code(text)` body the same way a built-in `code_block` does).
- `Opaque` blocks reconstructed verbatim from their stored JSON.
- Resulting `lopress_core::Block` tree feeds `lopress-gui-host::save`.

### Round-trip discipline

The inline subset modeled is exactly: bold, italic, inline-code, link. Anything else (footnote refs, raw HTML, strikethrough, etc.) is preserved by passthrough — text outside the supported markers is stored verbatim in run text. Property-based tests assert round-trip equality on a fixture corpus that includes:

- Pure prose paragraphs.
- Mixed bold/italic/code/link runs in various combinations.
- Edge cases: `**` adjacent to whitespace, links with parentheses in URLs, escaped `\*`, code spans containing backticks.
- Nested structures (lists inside lists, formatted text inside list items).
- Documents with at least one opaque block to ensure passthrough integrity.

## 4. Editor Pane Structure

```
EditorPane (Floem view)
  reads &EditorDoc
  owns DocSelection
  routes keyboard input
  renders a vertically scrollable list of BlockView
  emits BlockAction up to the doc owner

BlockView (per-block Floem view, polymorphic on BlockKind)
  ParagraphView   custom inline-runs editor
  HeadingView     custom inline-runs editor + level-styled font
  CodeView        Floem text editor (plain) + lang dropdown
  ListView        nested column of list-item views
  OpaqueView      placeholder card showing block.type_name and a "raw view" toggle
```

The custom inline-runs editor is the central widget. It owns one `Vec<InlineRun>` for one block, renders the runs as styled text, and handles:

- Caret positioning (run-index + char-offset-within-run).
- Local selection rendering (when the doc selection overlaps this block).
- Character input (insert at caret, splitting the run if mid-run).
- Toggle commands (Bold / Italic / Code / Link via Ctrl+B/I/E/K, block toolbar buttons, or slash menu) — toggle the relevant flag on the runs touching the selection, splitting and merging adjacent runs as needed.
- Backspace, Delete, Enter (split block), Backspace-at-start (merge with previous block).
- IME via Floem's input handling.

### Selection model — document-level

```rust
struct DocPosition {
    block: BlockId,
    run_index: usize,
    char_offset: usize,
}

struct DocSelection {
    anchor: DocPosition,
    head: DocPosition,
}
// Collapsed iff anchor == head; "caret" is just collapsed selection.
```

`EditorPane` owns the `DocSelection`. Each `BlockView` reads the slice of the selection that overlaps its block (none / partial-leading / full / partial-trailing) and renders the highlight + caret accordingly. Only the block holding `head` paints a blinking caret.

### Keyboard routing

`EditorPane` intercepts navigation and selection keys before block views see them:

- `←` `→` `↑` `↓` `Home` `End` `PgUp` `PgDn` move `head` and collapse selection to it.
- `Shift+` modifier on any of those extends `head` while leaving `anchor` fixed.
- `Cmd/Ctrl+A` selects the whole document.

Vertical arrows cross block boundaries by computing the target block + nearest-x-position from a per-block geometry cache populated during the previous frame's render (see "Caret-x cache" below).

Character input, Backspace, Delete, Enter, and slash-menu trigger flow to the focused block's view (the one holding `head`) when the selection is collapsed.

### Multi-block operations

When the selection is non-collapsed and spans more than one block:

- *Delete / Backspace / character input replacing selection* — splits the leading and trailing blocks at the selection boundaries, deletes everything between, merges what's left into a single block whose kind is the leading block's kind. Implemented as a single `BlockAction` so it survives as one logical operation if undo is added later.
- *Ctrl+B / Ctrl+I / Ctrl+E* — toggle the inline flag across every run in the selection. Toggle direction: if every touched character has the flag, clear it; otherwise set it.
- *Ctrl+K (link)* — wrap selection in a link with an empty URL placeholder; focus the URL field for editing.
- *Copy / Cut* — serialize the selection to a multi-block clipboard format. Two payloads written to the clipboard simultaneously: markdown (for external paste into other apps) and a serialized `Vec<EditorBlock>` slice (internal MIME type, for round-trip preservation when pasting back into the editor).
- *Paste* — if the internal payload is present, splice it in. Otherwise parse pasted text as markdown into blocks and splice. Either way replaces the selection.

### Caret-x cache

Vertical arrow navigation across blocks needs to land at the visually-correct x-position in the target block, not at the same character index. Each `BlockView`'s render writes its run geometry (per-character logical-x positions for the current frame) into a per-block cache keyed by `BlockId`. When the user presses `↑`/`↓` and the target is a different block, `EditorPane` reads the source block's cached x for the current `head`, then walks the target block's cache to find the closest character position.

Cache is invalidated when the block's content or width changes; on a clean miss, navigation falls back to "char index N of target block" and the cache populates next frame.

### Drag handles

Each block has a hover-revealed `⋮⋮` handle on the left, opacity 0 by default and 1 on block hover. Dragging starts on mousedown over the handle and emits a "move from→to" action on drop. The drop indicator is a horizontal line at the gap between blocks, which thickens when a valid drop target is hovered.

### Block toolbar

Anchored above the **focused** block (the one holding `head`). Visible whenever a block is focused, regardless of mouse hover. Despite the "hover toolbar" shorthand we've used informally, the trigger is keyboard/mouse focus, not pointer hover. Contents:

- Type combobox: `P / H1 / H2 / H3 / Code / UL / OL`. Change type via `BlockAction::ChangeType`.
- Bold / Italic / Code / Link toggle buttons. Reflect the current selection state (filled if every char in selection has the flag).
- Delete button.

Toolbar position recomputes on every frame to track the focused block as the layout shifts.

### Slash command menu

When the user types `/` at the start of an empty paragraph block, a popup opens with selectable items: Paragraph, Heading 1, Heading 2, Heading 3, Code block, Unordered list, Ordered list. Up/Down arrows navigate, Enter confirms, Escape closes. Selecting an item transforms the current block via `BlockAction::ChangeType`.

When `/` is typed mid-text or in a non-empty block, it's treated as a literal character — no popup.

## 5. Block Types in v1

| Type | Body | Editor view | Notes |
|------|------|-------------|-------|
| Paragraph | `Inline` | inline-runs editor | proportional 15 logical px |
| Heading 1..=6 | `Inline` | inline-runs editor | sizes 32/26/22/18/16/14 logical px |
| Code (with lang) | `Code` | plain monospace text editor + lang dropdown | no syntax highlighting at editor level — plugins can layer it via build-time templates |
| Unordered list | `List` | column of inline-runs editors with bullet prefix | one nesting level |
| Ordered list | `List` | column of inline-runs editors with number prefix | one nesting level |
| Plugin block | varies | built-in editor for the declared kind + auto-generated attr form | see section 12 |
| Opaque (no matching plugin) | `Opaque` | placeholder card showing `[type_name]` and a raw-JSON toggle | round-trips losslessly |

List items can themselves have inline formatting. Nested lists (lists inside list items) are not editable in v1 — they round-trip as part of the opaque payload if encountered.

## 6. Other Panes

These are mechanical ports. Each becomes a Floem view; behavior matches the existing egui counterparts unless explicitly noted.

### Welcome

Single centered column. "Open workspace" button (uses `rfd` for the native dir picker, same as today), recent workspaces list (loaded from the existing `recents.rs` JSON in the same path — file format unchanged), error banner shown when the previous open failed. Clicking a recent calls `Session::open` and transitions to the editing view.

### Sidebar

Vertical list grouped by Posts / Pages. Each entry shows title, draft pill, parse-error pill. Active row highlighted. Click → `EditingState::open_document`. Sticky "+ New post" / "+ New page" buttons at the bottom (write a stub markdown file under the appropriate dir then open it).

### Inspector

Right-pinned panel, 280 logical px wide. Form for front-matter fields: title (text), slug (text, derived placeholder when empty), date (text, ISO format — no calendar widget in v1), tags (comma-separated text), draft (bool). Edits set `doc.dirty`.

### Footer

Single horizontal strip at the bottom. Build status indicator (idle/building/ok/failed with message), save state (saved / unsaved / save error), word count, server URL (click-to-copy). All values pulled from `Session::build_status()`, `Session::serve_status()`, and `EditorDoc` — same data sources as the egui version.

### App shell layout

`Welcome` is its own top-level view. Once a workspace opens, the layout is a 3-column split — Sidebar (220 logical px) | EditorPane (flex) | Inspector (280 logical px) — with the Footer pinned below the whole row.

## 7. Save Behavior

Every edit marks `doc.dirty`. A debouncer at the EditorPane level fires `Session::save` followed by `Session::rebuild` 500 ms after the last keystroke. On window close, force-save synchronously if dirty. Error from save shown in the footer; doesn't block the editor.

The 500 ms debounce is new — the egui editor saves on every keystroke. With Floem we have a real event loop that makes debouncing trivial, and it'll significantly cut watcher/rebuild churn during typing without changing the user-visible "live preview updates as I type" feel.

## 8. Action Shape (Undo Foundation)

Undo/redo itself is deferred to a follow-up phase. What ships in v1 is the architectural shape that lets undo be added cheaply later: every block-tree mutation goes through a single `apply(action: BlockAction)` chokepoint, where `BlockAction` is a discrete enum (Insert, Move, Delete, Split, Merge, ChangeType, ToggleInline, EditText, EditAttrs, etc.). No mutation happens in-place outside that path.

This costs nothing — the egui editor already does this — and the next phase's work becomes "wire up an undo stack that records actions and applies inverses," not "rewrite every mutation site to be invertible." Inverse construction itself is also deferred to that phase.

In v1, callers of `apply` discard the action after applying. The next phase changes that to push it onto a stack with its inverse.

## 9. Dimensions and Resolution Scaling

All dimensions in this spec are **logical pixels**. Floem (via winit) is DPI-aware: a `220.0` width on a 2x display renders at 440 device pixels and looks the same physical size as on a 1x display. Per-monitor DPI is handled when the window moves between monitors. Same convention as CSS `px`, gpui's `Pixels`, egui's `f32` sizes.

### UI zoom (deferred)

User-configurable UI zoom (`Cmd/Ctrl+=` / `Cmd/Ctrl+−` / `Cmd/Ctrl+0`) and a persisted `ui_zoom: f32` setting are deferred to a follow-up phase. Floem's existing per-monitor DPI handling means physical sizes already track the OS scale; the missing piece is a user override on top of that. When added later, it'll be a single multiplier into Floem's root scale factor plus three keyboard handlers and one settings-file field.

### Custom-painted UI

The inline-runs editor's caret line, selection highlights, drag-handle hit areas, and block-toolbar offsets must use logical-pixel coordinates. Floem's painting API operates in logical pixels by default, so this is the path of least resistance, but it's worth calling out as a discipline because mixing in raw device pixels would silently break on retina displays.

### Asset handling

v1 uses Unicode glyphs (`⠿`, `×`, `⋮⋮`, `•`) and SVG/path-rendered shapes for icons. Both scale cleanly. No bitmap assets in v1.

### Window startup size

Default `1200 × 800` logical px. Last window size and position persist to the settings file and are restored on launch, clamped to the available monitor.

## 10. Settings File

A single JSON file `settings.json` under the platform's standard config directory (resolved via the `directories` crate, same way `recents.rs` does today). Contents:

```json
{
  "recents": [...],
  "window": {
    "width": 1200.0,
    "height": 800.0,
    "x": 100.0,
    "y": 100.0,
    "maximized": false
  }
}
```

Loaded at app startup; written on relevant events (workspace opened, window resized/moved/closed). The `ui_zoom` field is reserved for a future phase; readers should ignore unknown fields and writers should not emit them in v1.

**Migration from existing `recents.json`.** On first launch under the new editor, if `settings.json` does not exist but `recents.json` does, the recent-workspaces list is read from `recents.json`, written into the new `settings.json`, and `recents.json` is deleted. One-shot migration; no compatibility shim retained beyond first launch.

## 11. Testing Strategy

### Unit tests

- **Inline-runs round-trip**: parse markdown → runs → serialize → markdown on a corpus of fixtures covering paragraphs with mixed bold/italic/code/link, edge cases (`**` adjacent to whitespace, links with parens in URLs, escaped `\*`, code spans with backticks).
- **`BlockAction::apply` and inverses**: every action has a `roundtrip(action, doc)` test that asserts applying the inverse to the post-state restores the pre-state.
- **Selection logic**: given a `Vec<EditorBlock>` and a `DocSelection`, assert what `delete_selection`, `toggle_bold`, `paste_blocks`, etc. produce. No Floem in these tests.

### Integration tests

- **Full document round-trip**: load a fixture markdown file via `lopress-gui-host`, convert to `EditorDoc`, convert back, save, diff against original. Must be byte-identical for the supported inline subset and lossless for opaque blocks.

### Manual smoke checklist

A short checklist file run before each release:

1. Launch the app, see Welcome screen.
2. Open a workspace, see Sidebar populated.
3. Open a post, see blocks rendered.
4. Type a paragraph with bold (Ctrl+B), italic (Ctrl+I), code (Ctrl+E), link (Ctrl+K).
5. Insert a heading via slash command.
6. Drag a block to reorder.
7. Select across blocks with Shift+Down, delete the selection.
8. Save (via debounce), see preview update in browser.
9. Close window, confirm dirty save flush.
10. Re-open, confirm window position restored.

No automated GUI tests in v1 — Floem doesn't yet have a great story for them, and the cost isn't worth it for a personal tool.

## 12. Plugin Block Rendering (Path 1)

Plugins declare new block types via `lopress-plugin`'s existing `BlockDecl` mechanism. The Floem editor consumes those declarations to give plugin blocks first-class editing. This is "Path 1" of the plugin extensibility story; "Path 2" (plugins that ship code to render their own editor UI) is documented as a future direction in `docs/superpowers/ideas/2026-05-02-plugin-code-hooks.md` and is out of scope here.

### How a plugin participates

The existing `BlockDecl` struct in `lopress-plugin` already has the fields we need:

```toml
# Example plugin manifest
[[blocks]]
name     = "lopress:codehighlight"
template = "blocks/code-highlight.html"   # build-time HTML template
editor   = "code"                          # which built-in editor kind to use

[blocks.attrs]
lang  = { type = "string", ui = "select", options = ["rust", "go", "python"] }
theme = { type = "string", ui = "select", options = ["github", "monokai"] }
```

The `editor` field is the load hint. v1 recognized values:

| Value | Editor surface |
|-------|----------------|
| `"paragraph"` | inline-runs editor |
| `"heading"` | inline-runs editor with heading-1 styling |
| `"code"` | monospace text editor (lang from `attrs` if declared) |
| `"list"` | bullet/numbered list editor (`ordered` from `attrs` if declared) |
| `null` (unset) | falls back to `"paragraph"` |

Other values (e.g., starting with `"wasm:"`) are reserved for future Path 2 use; in v1 they're treated as unrecognized and the block falls back to opaque rendering with a warning logged.

### Editor view for a plugin block

Two stacked sections inside one card:

1. **Plugin header.** A thin strip showing the block type name (`lopress:codehighlight`) styled like a tag. Optional collapse toggle (v1: always expanded).
2. **Attr form.** One row per attr in the `BlockDecl.attrs`. Each rendered per its `ui` hint:
   - `text` → single-line text field.
   - `select` (with `options`) → dropdown.
   - `checkbox` → checkbox.
   - `number` → numeric text field.
   - Unrecognized `ui` → text field (raw JSON value).
3. **Body editor.** The built-in editor matching the `editor` kind, operating on the block's `body`.

The card has the usual block affordances: drag handle, delete button, focus/selection support consistent with built-in blocks.

### Round-trip

- On load: `from_core` looks the block type up in `PluginRegistry`. If found, populates `body` per the `editor` kind and `plugin: Some(PluginMeta { ... })` with the attrs and a snapshot of the attr decls.
- On save: `to_core` reconstructs `lopress_core::Block { r#type: plugin.block_type_name, attrs: plugin.attrs, ... }` with body fields filled in the same shape a built-in block of that editor kind would produce.
- If the registry has no matching plugin (uninstalled, version mismatch, etc.), the block converts to `Opaque` instead — same lossless round-trip path as any unknown type.

### Plugin reload

`PluginRegistry` is loaded once per session at workspace open. Adding/removing/changing plugins requires reopening the workspace in v1. Hot-reload is not in scope.

### Why this is enough for v1

This design covers the WordPress-block common case: a plugin author wants a block with structured attrs, a familiar editor surface, and custom build-time HTML. Concrete examples expressible in v1:

- *Syntax-highlighted code block*: `editor = "code"`, attrs for lang/theme, template runs `syntect` server-side.
- *Video embed*: `editor = "paragraph"` (caption text), attrs for src/autoplay/poster, template emits `<video>`.
- *Callout box*: `editor = "paragraph"` (body text), attrs for severity/icon, template emits a styled `<aside>`.
- *Pull quote*: `editor = "paragraph"`, attrs for citation, template emits `<blockquote>` with citation styling.

What it doesn't cover, and where Path 2 would be needed: anything where the plugin needs custom editor-time rendering (interactive table builder, drag-to-arrange grid, live-preview embed of an external service, etc.).

## 13. Implementation Plan

After approval of this spec, the implementation plan will be written to `docs/superpowers/plans/2026-05-02-editor-floem.md` via the `writing-plans` skill. The plan will decompose the work into ordered tasks with explicit verifications, suitable for execution in subsequent sessions.

Expected high-level task ordering:

1. Workspace plumbing — delete egui code, add Floem dep, scaffold empty `lopress-editor` and Floem `main.rs` that opens an empty window.
2. App shell — Welcome → Editing view transition, settings file persistence, window size restore.
3. Document model — `EditorDoc`, `EditorBlock`, `InlineRun`, `PluginMeta`, `from_core` / `to_core` converters, round-trip tests.
4. Static block rendering (read-only) — render every built-in block kind with correct typography. No editing yet.
5. Single-block inline-runs editor — caret, character input, basic keyboard, no styling yet.
6. Inline toggles — Bold / Italic / Code / Link via Ctrl shortcuts and selection-flag math.
7. Block structural actions — Split / Merge / Insert / Delete / Move / ChangeType, all going through a single `apply(BlockAction)` chokepoint.
8. Block toolbar.
9. Slash command menu.
10. Drag-and-drop reorder.
11. Multi-block selection — DocSelection, keyboard routing, caret-x cache, multi-block delete/toggle/copy/paste.
12. Plugin block rendering — `PluginRegistry` integration, attr form generation, plugin block round-trip.
13. Sidebar, Inspector, Footer ports.
14. Save debounce + rebuild integration.
15. Manual smoke checklist run.

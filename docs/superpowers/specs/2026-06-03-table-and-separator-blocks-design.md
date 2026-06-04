# Table & Separator Blocks

**Date:** 2026-06-03
**Author:** Kyle
**Status:** spec — ready for implementation planning
**Related:** `docs/superpowers/specs/2026-06-01-image-block-design.md` (native base plugin pattern), `docs/superpowers/specs/2026-06-01-read-more-marker-design.md` (empty-Paragraph-with-PluginMeta pattern), `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` (plugin capability model)

---

## 1. Background

lopress has no support for two common markdown constructs: horizontal rules (thematic
breaks) and GFM tables. The parser silently drops `---` rule events, and table support
is not enabled in pulldown-cmark at all. There is no editor UI to insert either construct,
and the build renderer has no output for them.

This spec adds **two new base plugin block types**, both genuine native-markdown
constructs:

1. **Separator** — a horizontal rule / thematic break (`---` in markdown, `<hr>` on the
   built site).
2. **Table** — a GFM table: exactly one header row plus N body rows, single-line cells
   with inline formatting (bold/italic/code/links), and per-column alignment
   (left/center/right). Edited in the pane via a contextual toolbar strip for row/column/
   alignment operations.

Both ship as base plugins (the same mechanism as `image`, `list`, `code`): a manifest
under `base_plugins/`, `builtin = true`, a `native = "<type>"` claim, registered in
`PluginRegistry::load_base_plugins`. Because they claim a native core type they are
excluded from the dynamic plugin inserter (`model/inserter.rs::is_insertable` filters out
`native`/`builtin` blocks), so each gets a hardcoded `SlashChoice` entry, exactly like
the existing `SlashChoice::Image` and `SlashChoice::ReadMore`.

---

## 2. Scope

### Shared architecture

- Both blocks flow through the same four-layer pipeline:
  - **Core parse** (`crates/lopress-core/src/parser.rs`): markdown → `lopress_core::Block`
    tree.
  - **Core serialize** (`crates/lopress-core/src/serializer.rs`): `Block` tree → markdown.
  - **Build render** (`crates/lopress-build/src/render.rs`): renders the `Block` tree
    directly to HTML with one `write_block` arm per type.
  - **Editor model** (`crates/lopress-editor`): `from_core`/`to_core` conversions, a
    built-in editor widget dispatched by `editor_for(key)`, plus slash-menu and toolbar
    entry points.
- Each block is registered as a base plugin:
  `base_plugins/separator/manifest.toml` and `base_plugins/table/manifest.toml`, both
  `builtin = true` with a `native` claim, added to the `BASE_MANIFESTS` array in
  `crates/lopress-plugin/src/registry.rs::load_base_plugins`.

### Separator

- **Core type:** `separator`. No attrs, no children, no text. Canonical markdown form:
  `---` (a thematic break). `***` / `___` also parse to the same thing; serialize canonically
  as `---`.
- **Parser:** `Event::Rule` is currently matched and returns `None` (separators are silently
  dropped today). Change that arm to emit
  `Block { type: "separator", attrs: {}, children: [], text: None }`.
- **Serializer:** add a `"separator" =>` arm emitting `---\n`.
- **Build render (`render.rs::write_block`):** add a `"separator" =>` arm emitting
  `<hr>\n`.
- **Editor model:** No new `BlockKind` is required — reuse the read-more marker's trick.
  The separator EditorBlock is `kind: BlockKind::Paragraph`,
  `body: BlockBody::Inline(vec![])` (empty), with
  `plugin: Some(PluginMeta { block_type_name: "separator", attrs: {}, attr_decls: [],
  builtin: true, editor: Some("separator"), native: Some("separator") })`.
  Add `PluginMeta::separator()` and `EditorBlock::separator()` constructors next to the
  existing `read_more()` / `image()` ones in `model/types.rs`.
  - `from_core`: add a `Some("separator")` arm to `native_block_from_core` building that
    EditorBlock.
  - `to_core`: no special case needed. Because `meta.native == Some("separator")`,
    `block_to_core` routes to `native_block_to_core`, whose existing fallback `_` arm emits
    `Block { type: core_type, attrs: {…}, children: [], text: None }` — i.e. a clean,
    attr-less, empty `separator` block. The separator's `meta.attrs` is empty so the
    serializer's `---` arm round-trips.
- **Widget:** `editor_for("separator")` → a new widget in `ui/blocks/separator.rs`, cloned
  from `ui/blocks/read_more.rs`: a slim full-width horizontal rule, focusable on PointerDown
  (so it can be selected and deleted via the toolbar), with no text label (read_more shows
  "— Read more —"; the separator shows just the rule).
- **Slash menu (`ui/slash_menu.rs`):** add a `SlashChoice::Separator` variant and a
  `"Separator"` item to `slash_menu_items()`. The editor pane's select handler constructs
  `EditorBlock::separator()` and inserts it (same insertion path as `SlashChoice::Image` /
  `ReadMore`).
- Separator is **slash-menu only** — no toolbar button (only the table gets a toolbar
  button; see §8).

### Table

- **Core data model:**

  ```
  Block { type: "table", attrs: { "align": ["none"|"left"|"center"|"right", …] },
          children: [ table_row, table_row, … ] }     // children[0] IS the header row
    table_row  → Block { type: "table_row",  children: [ table_cell, … ] }
    table_cell → Block { type: "table_cell", text: "<inline markdown>", children: [] }
  ```

  `attrs.align` is an array whose length equals the column count; each entry is one of
  `"none"`, `"left"`, `"center"`, `"right"`. The header is **row 0 by position** — GFM
  guarantees exactly one header row and it is first. There is no separate `table_head` type;
  the first `table_row` child is the header. Each cell's `text` holds inline-markdown source
  (e.g. `**bold** and *italic*`), consistent with how paragraph/heading store their inline
  text in `Block.text`.

- **Editor model (`crates/lopress-editor/src/model/types.rs`):**
  - `BlockKind::Table`
  - `BlockBody::Table(TableData)` where:
    - `TableData { align: Vec<Align>, rows: Vec<TableRow> }`
    - `TableRow { id: BlockId, cells: Vec<TableCell> }`
    - `TableCell { id: BlockId, runs: Vec<InlineRun> }`
    - `Align { None, Left, Center, Right }`
    - `rows[0]` is the header row.
  - A `PluginMeta::table()` constructor and an `EditorBlock::table(...)` constructor, plus an
    `EditorBlock::table_default()` that builds the default inserted shape: **2 columns × 2
    rows (1 header row + 1 body row), all cells empty, alignment `none` for both columns.**
    Both the slash menu and the toolbar button use `table_default()`.

- **Core parser / serializer / build render:**
  - **Parser (`parser.rs`):** GFM tables are not parsed at all today because table support is
    not enabled. Replace `pulldown_cmark::Parser::new(body)` with
    `Parser::new_ext(body, Options::ENABLE_TABLES)` in **both** places it appears —
    `parse_plain_markdown` (the inner block parser) and `render_markdown` (the build-side
    md→HTML used by markdown-template plugins). Then add a `Tag::Table(alignments)` arm to
    `parse_one` that walks the `TableHead` / `TableRow` / `TableCell` events into the
    `table`/`table_row`/`table_cell` block tree. A `consume_table_cell` helper accumulates a
    cell's inline-markdown text, reusing the same inline conversions as the existing
    `consume_inline` (emphasis → `*`, strong → `**`, code → backticks, link → its text). Map
    pulldown's `Alignment::{None,Left,Center,Right}` to the `align` strings.
  - **Serializer (`serializer.rs`):** add a `"table" =>` arm emitting GFM:
    - header row from `children[0]`: `| h1 | h2 |`
    - the alignment delimiter row from `attrs.align`: e.g. `| :--- | ---: | :---: | --- |`
      (left = `:---`, right = `---:`, center = `:---:`, none = `---`)
    - one line per body row.
    - Pipe characters inside cell text are escaped as `\|`.
  - **Build render (`render.rs::write_block`):** add a `"table" =>` arm emitting
    `<table><thead><tr><th>…</th></tr></thead><tbody><tr><td>…</td></tr>…</tbody></table>`,
    applying `style="text-align:left|center|right"` per column from `attrs.align` (omit the
    style for `none`). Cell text is **escaped** via the existing `escape()` helper — matching
    how paragraph/heading/list cells render today. Inline-markdown → HTML on the build side is
    a **pre-existing gap shared by all blocks** — paragraphs already emit literal `**bold**`
    on the built site — and is explicitly **out of scope** for this feature.

- **Editor conversions & exhaustive-match fallout:**
  - **`model/from_core.rs`:** `native_block_from_core` gets a `Some("table")` arm parsing the
    `table_row`/`table_cell` children into `TableData` (alignments from `attrs.align`), then
    stamping native `PluginMeta`. A **malformed table** (ragged rows, wrong child types,
    missing header) falls back to `EditorBlock::opaque(...)` for a verbatim round-trip — the
    same defensive pattern `native_list_from_core` uses when a list isn't cleanly convertible.
  - **`model/to_core.rs`:** `native_block_to_core` gets a `BlockBody::Table` arm rebuilding the
    `table`/`table_row`/`table_cell` core tree (align back into `attrs.align`, header = rows[0]).
  - **`actions.rs`:** `apply_change_type`, `coerce_body_to_kind`, `body_matches_kind`,
    `body_to_flat_text`, and `apply_split` all match on kind/body and need a Table arm.
    Table is **not a free conversion target**. Converting an existing paragraph/heading/list/code
    into a table is unsupported (the toolbar/slash insert a fresh table instead of a
    ChangeType). For the degenerate flatten direction (`body_to_flat_text` of a Table, used by
    fallback rendering), join cell text with tabs within a row and newlines between rows.
    `body_matches_kind` must recognize `(Table, BlockBody::Table)`. `coerce_body_to_kind` keeps
    a Table body for a Table kind and otherwise leaves mismatches as-is (no widget commits a
    non-Table body into a Table block). `apply_split` on a Table body returns `None` (no split
    semantics — like the `Opaque` arm).
  - **`model/sync.rs::canonicalize_body`:** add a Table arm (canonicalize each cell's runs the
    same way list-item runs are canonicalized).
  - **Block render dispatch (`ui/blocks/mod.rs` / wherever `render_body` dispatches by kind):**
    route `BlockKind::Table` through the `editor_for("table")` widget path (it is plugin-
    flagged with `editor = "table"`, so it should already flow through the editor-registry
    dispatch the way list/code/image do — confirm and wire as needed).

- **Widget, actions, and entry points:**
  - **Widget (`ui/blocks/table.rs`, dispatched by `editor_for("table")`):**
    - A **contextual in-flow toolbar strip** rendered just above the grid, shown when any cell
      in this table is focused. Buttons: `Add Row`, `Del Row`, `Add Column`, `Del Column`, and
      an alignment control (`L` / `C` / `R`) acting on the focused cell's column. In-flow
      placement (not absolutely-positioned overlay) deliberately avoids the known floem 0.2
      overlay hit-test gotcha where absolutely-positioned children that overflow the parent
      paint but become dead to hit-testing.
    - Below the strip, a grid of cells. Each cell is the existing inline editor (the same
      `inline_editor` wiring `ui/blocks/list.rs` uses for list items), so cells get
      bold/italic/code/link editing for free. The header row (row 0) is styled bold.
    - Focus tracking determines which row/column the strip operates on (reuse the existing
      `focus_target` / `focus_pub` mechanism).
  - **New `BlockAction` variants (`actions.rs`):** `TableInsertRow`, `TableDeleteRow`,
    `TableInsertColumn`, `TableDeleteColumn`, `TableSetAlign` — each keyed by `block_id` plus
    a row and/or column index (and, for SetAlign, the target `Align`). Each returns a proper
    inverse so undo/redo works (mirror how the list mutations record inverses). Cell text
    edits do **not** need new variants — they flow through the existing `EditBlockBody` path
    carrying the new Table body, exactly as list-item text edits do. Guards: cannot delete the
    header row; cannot delete the last remaining column; cannot delete the last remaining body
    row. The `actions.rs` test asserting `size_of::<BlockAction>() <= 40` must still pass — box
    any new variant whose inline payload would exceed that (follow the existing boxed-variant
    precedent like `EditFrontMatter`/`EditBlockBody`).
  - **Slash menu:** `SlashChoice::Table`, label `"Table"`, inserts `EditorBlock::table_default()`.
  - **Toolbar button (`ui/toolbar.rs::block_toolbar_for`):** add a `Table` button. It is **not**
    a kind-cycler button — the existing P/H1/…/UL/OL buttons fire `BlockAction::ChangeType`, but
    a table is not a conversion target. The Table button instead fires
    `BlockAction::InsertAfter { anchor: block_id, new_block:
    Box::new(EditorBlock::table_default()) }`, dropping a fresh 2×2 table immediately after the
    focused block. Separate it from the kind-cycler row with the existing `separator()` spacer
    helper. It reuses the same `table_default()` constructor the slash menu uses, so all entry
    points produce an identical block.

A table can be created three ways — slash menu (`/table`), the toolbar button, and that's
the full set. The separator is slash-only.

---

## 3. Testing

### Core (`lopress-core`)

- Parser: `Event::Rule` → a `separator` block; a GFM table parses into `table`/`table_row`/`table_cell`
  with correct `align`; inline formatting inside cells is captured; escaped pipes survive.
- Serializer: `separator` → `---`; a `table` → GFM with the correct alignment delimiter row
  and `\|`-escaped cell pipes. Round-trip (`parse → serialize → parse`) is stable for both a
  separator and a table with mixed alignment + inline formatting.

### Build render (`lopress-build`)

- `separator` → `<hr>`; `table` → `<table>` with `text-align` styles from `align` and escaped
  cell text.

### Editor model (`lopress-editor`)

- `from_core`/`to_core` round-trip for a separator and for a table (header + body, mixed
  alignment, inline runs in cells).
- A malformed table block degrades to `Opaque` and round-trips verbatim.
- Each new table `BlockAction` and its undo inverse: insert/delete row, insert/delete column,
  set alignment — including the guard cases (refuse to delete the header row, the last column,
  the last body row).
- The `BlockAction` size-guard test still passes (≤ 40 bytes) after the new variants.

### End-to-end (driving the running editor via the debug control server on 127.0.0.1:7878)

Insert a separator and a table from the slash menu, insert a table via the toolbar button,
screenshot, and confirm the persisted `.md` contains the expected `---` and GFM table markup.

---

## 4. Implementation Order

1. `base_plugins/separator/manifest.toml` and `base_plugins/table/manifest.toml`; seed both
   in `load_base_plugins()`.
2. Parser: enable `Options::ENABLE_TABLES` in both `pulldown_cmark::Parser::new_ext` call
   sites; add `Event::Rule` → `separator` arm; add `Tag::Table` → `table`/`table_row`/`table_cell`
   arm with `consume_table_cell` helper.
3. Serializer: add `"separator"` and `"table"` arms.
4. Build render: add `"separator"` → `<hr>` and `"table"` → `<table>` arms to `write_block`.
5. Editor model types: `BlockKind::Table`, `BlockBody::Table(TableData)`, `PluginMeta::table()`,
   `EditorBlock::table()`/`table_default()`, `PluginMeta::separator()`, `EditorBlock::separator()`.
6. `from_core`: `native_block_from_core` arms for `"separator"` and `"table"` (with malformed
   table fallback to `Opaque`).
7. `to_core`: `native_block_to_core` arm for `BlockBody::Table`.
8. `actions.rs`: new `TableInsertRow`, `TableDeleteRow`, `TableInsertColumn`,
   `TableDeleteColumn`, `TableSetAlign` variants (boxed if needed); arms in
   `apply_change_type`, `coerce_body_to_kind`, `body_matches_kind`, `body_to_flat_text`,
   `apply_split`.
9. `model/sync.rs::canonicalize_body`: add Table arm.
10. Editor widgets: `ui/blocks/separator.rs` (cloned from read_more); `ui/blocks/table.rs`
    (in-flow toolbar strip + grid of inline-edit cells).
11. `editor_registry`: register `"separator"` and `"table"` keys.
12. Slash menu: `SlashChoice::Separator`, `SlashChoice::Table`, and their insertion paths.
13. Toolbar: add Table button using `InsertAfter` semantics.
14. Block render dispatch: confirm `BlockKind::Table` routes through `editor_for("table")`.
15. Tests (core round-trip, build render, editor round-trip, action guards, size test, e2e).

---

## 5. Decisions

### Both ship as native base plugins claiming core types (chosen) vs. lopress-comment-container
### blocks (rejected)

Separator and table **are** real markdown constructs (`---` and GFM tables), so they claim a
native core type and round-trip as ordinary markdown — not as `<!-- lopress:… -->` comment
containers. This matches the existing `image`/`list`/`code` precedent. Consequence: both are
excluded from the dynamic plugin inserter and get hardcoded `SlashChoice` entries (like
`Image`/`ReadMore`).

### Separator reuses the read-more "empty Paragraph + PluginMeta" representation (chosen) vs.
### a new `BlockKind::Separator` (rejected)

The separator has no editable content, so a dedicated `BlockKind` would add exhaustive-match
churn for no benefit. The read-more marker already established the empty-Paragraph-with-
PluginMeta pattern; the separator follows it. (The table, by contrast, **does** need a new
`BlockKind::Table` + `BlockBody::Table` because it has rich structured, editable content.)

### Table editing scope = structural + per-column alignment + inline formatting in cells
### (chosen)

Header + body rows, add/remove rows & columns, left/center/right column alignment, and
bold/italic/code/link inside cells (cells reuse the existing inline editor). This is
effectively full GFM parity.

Rejected: a plain-text-cell "minimal MVP" (less parity) and a "structural only, no inline"
middle option — the user wanted inline formatting in cells.

### Contextual in-flow toolbar strip for table controls (chosen) vs. edge +/- hover buttons
### (rejected) vs. block-inspector panel (rejected)

A small in-flow control strip above the table (shown when a cell is focused) keeps controls
near the cells while staying clear of the floem overlay hit-test trap that edge-overflow
buttons would hit. The inspector-panel option was rejected as putting the controls too far
from the cells.

### Table is not a conversion target (chosen)

Converting an existing block into a table (paragraph → table) is unsupported; you insert a
fresh table instead. There is no sensible, non-lossy way to coerce arbitrary block text into
a grid, so the toolbar Table button uses `InsertAfter` semantics rather than `ChangeType`,
and `apply_change_type` has no "to Table" arm. The degenerate flatten direction (Table →
plain text, for fallback rendering only) joins cells with tabs/newlines.

### Build-side HTML escapes cell text rather than rendering inline markdown (chosen, consistent
### with the status quo)

The build render path emits literal cell text the same way it emits literal paragraph text
today — inline-md → HTML on the build side is a pre-existing gap across all block types and
is out of scope here. Inline formatting is fully preserved in the markdown source and in the
editor.

### Header is row 0 by position (chosen) vs. a marker attr / a separate `table_head` block
### type (rejected)

GFM guarantees exactly one header row, first, so position is unambiguous and needs no extra
type or attribute.

---

## 6. Non-Goals

- No merged / split cells, no row/column spans.
- No block-level content inside cells (no lists/images/code blocks in a cell); cells are
  single-line inline text only — the GFM constraint.
- No multi-line cells.
- No table caption, no CSV/clipboard import, no column drag-to-reorder.
- No build-side inline-markdown → HTML rendering (pre-existing gap, shared by all blocks).
- No "convert existing block into a table" path.
- Separator gets no toolbar button (table only) and no styling variants — markdown has
  exactly one thematic break.

---

## 7. Open Questions for Claude

None. All design decisions above are resolved; the spec contains no placeholders.

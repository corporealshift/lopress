# Table & Separator Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two native-markdown block types to lopress — a separator (horizontal rule / `---`) and a GFM table (header + body rows, per-column alignment, inline formatting in cells) — across the core parser/serializer, the build HTML renderer, and the floem editor (model, conversions, widgets, slash menu, toolbar).

**Architecture:** Both ship as **native base plugins** (the `image`/`list`/`code` pattern): a `base_plugins/<name>/manifest.toml` registered in `load_base_plugins`, `builtin = true` with a `native = "<type>"` claim. The core parser emits the native block type, the serializer/build-renderer emit markdown/HTML, and the editor routes the block through `plugin_block_view` → `editor_for(<key>)` to a built-in widget. Because they claim native types they are excluded from the dynamic inserter and get hardcoded `SlashChoice` entries (like `Image`/`ReadMore`).

**Tech Stack:** Rust, `pulldown-cmark` 0.10.3 (markdown parse), `serde_json`, `floem` 0.2 (editor UI), `tera` (build templates — untouched here). Spec: `docs/superpowers/specs/2026-06-03-table-and-separator-blocks-design.md`.

**Standing rules for every task (this repo):**
- **Real code wins.** This plan was written against the live tree at branch `feat/table-and-separator-blocks` (parent commit `e0991be`) on 2026-06-03; cited line numbers/snippets are current. If a snippet doesn't match disk, grep/read the real construct, apply the *intent*, and STOP-and-report rather than hand-balancing braces.
- **Lints (AGENTS.md).** No `unwrap`/`expect`/`panic`/`unreachable`/`todo`/indexing/`as`-casts/integer-division in production code (tests are exempt via the crate-root `cfg_attr(test, allow(...))`). Prefer `match`/`let-else` over `is_some()` ladders. Justify every `#[allow]` with an adjacent comment.
- **Gate once, `--workspace`.** Final gate is `bash scripts/check.sh` (fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace`). Clippy caches: after a `cargo test/run`, touch a source file in each changed crate before trusting a green clippy.
- **Stage NAMED files per commit — never `git add -A`.** The tree has unrelated untracked files (`.pi-delegations/*`, `.claude/settings.local.json`, `rust-toolchain.toml`). Do not sweep them in.
- **One commit per task**, exactly as each task's commit step lists.

---

## Task 1: Base-plugin manifests for `separator` and `table`

**Files:**
- Create: `base_plugins/separator/manifest.toml`
- Create: `base_plugins/table/manifest.toml`
- Modify: `crates/lopress-plugin/src/registry.rs` (the `BASE_MANIFESTS` array in `load_base_plugins`, ~line 70; and add tests in the `mod tests` block)

- [ ] **Step 1: Write the failing tests** — append to the `mod tests` block in `crates/lopress-plugin/src/registry.rs`:

```rust
    #[test]
    fn base_plugins_include_separator() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_p, decl) = reg.native_block("separator").expect("separator native block");
        assert_eq!(decl.editor.as_deref(), Some("separator"));
        assert_eq!(decl.native.as_deref(), Some("separator"));
        assert!(decl.builtin);
        assert!(decl.attrs.is_empty());
    }

    #[test]
    fn base_plugins_include_table() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_p, decl) = reg.native_block("table").expect("table native block");
        assert_eq!(decl.editor.as_deref(), Some("table"));
        assert_eq!(decl.native.as_deref(), Some("table"));
        assert!(decl.builtin);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-plugin base_plugins_include_ -- --nocapture`
Expected: FAIL (`separator native block` / `table native block` panics — not registered yet).

- [ ] **Step 3: Create `base_plugins/separator/manifest.toml`:**

```toml
# Built-in "base" plugin: the separator block (horizontal rule / thematic
# break), claiming the native core `separator` type. Embedded at compile time
# via include_str! — see load_base_plugins.
name    = "lopress-separator"
version = "0.1.0"

[[blocks]]
name    = "separator"
editor  = "separator"
native  = "separator"
builtin = true
```

- [ ] **Step 4: Create `base_plugins/table/manifest.toml`:**

```toml
# Built-in "base" plugin: the GFM table block, claiming the native core
# `table` type. Embedded at compile time via include_str! — see
# load_base_plugins.
name    = "lopress-table"
version = "0.1.0"

[[blocks]]
name    = "table"
editor  = "table"
native  = "table"
builtin = true
```

- [ ] **Step 5: Register both in `load_base_plugins`** — extend the `BASE_MANIFESTS` array (currently list/code/more/image):

```rust
        const BASE_MANIFESTS: &[&str] = &[
            include_str!("../../../base_plugins/list/manifest.toml"),
            include_str!("../../../base_plugins/code/manifest.toml"),
            include_str!("../../../base_plugins/more/manifest.toml"),
            include_str!("../../../base_plugins/image/manifest.toml"),
            include_str!("../../../base_plugins/separator/manifest.toml"),
            include_str!("../../../base_plugins/table/manifest.toml"),
        ];
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p lopress-plugin base_plugins_include_`
Expected: PASS (both).

- [ ] **Step 7: Commit**

```bash
git add base_plugins/separator/manifest.toml base_plugins/table/manifest.toml crates/lopress-plugin/src/registry.rs
git commit -m "feat(plugin): register separator and table base plugins"
```

---

## Task 2: Core parser — emit a `separator` block from `Event::Rule`

**Files:**
- Modify: `crates/lopress-core/src/parser.rs` (the `parse_one` match — the `Event::Rule` currently sits in the `return Ok(None)` catch-all near line 250; and add a test in `mod tests`)

- [ ] **Step 1: Write the failing test** — add to `crates/lopress-core/src/parser.rs` `mod tests`:

```rust
    #[test]
    fn parses_thematic_break_as_separator() {
        let d = parse("before\n\n---\n\nafter\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["paragraph", "separator", "paragraph"]);
        let sep = &d.blocks[1];
        assert!(sep.children.is_empty());
        assert!(sep.text.is_none());
        assert_eq!(sep.attrs, json!({}));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-core parses_thematic_break_as_separator`
Expected: FAIL (`separator` missing — the rule is currently dropped, so only two paragraphs are produced).

- [ ] **Step 3: Add a dedicated `Event::Rule` arm in `parse_one`.** Find the catch-all arm that lists `Event::Rule` among the `return Ok(None)` variants and REMOVE `Event::Rule` from it, then add this arm above the catch-all (e.g. directly after the `Event::Start(Tag::Item) => parse_item(parser)?,` arm):

```rust
        Event::Rule => Block {
            r#type: "separator".into(),
            attrs: json!({}),
            children: vec![],
            text: None,
        },
```

The catch-all should now read (note `Event::Rule` is gone):

```rust
        Event::Html(_)
        | Event::InlineHtml(_)
        | Event::Text(_)
        | Event::Code(_)
        | Event::SoftBreak
        | Event::HardBreak
        | Event::TaskListMarker(_)
        | Event::FootnoteReference(_)
        | Event::Start(_)
        | Event::End(_) => {
            return Ok(None);
        }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p lopress-core parses_thematic_break_as_separator`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-core/src/parser.rs
git commit -m "feat(core): parse thematic break into a separator block"
```

---

## Task 3: Core parser — enable GFM tables and parse them into block tree

**Files:**
- Modify: `crates/lopress-core/src/parser.rs` (imports; `parse_plain_markdown`; `render_markdown`; `parse_one`; new `parse_table` + `consume_table_cell` helpers; tests)

**Context:** Today `parse_plain_markdown` and `render_markdown` both use `Parser::new(...)` with no options, so GFM tables are never emitted. pulldown-cmark 0.10.3 exposes `Options::ENABLE_TABLES`, `Parser::new_ext`, `Tag::Table(Vec<Alignment>)`, unit `Tag::TableHead`/`TableRow`/`TableCell`, matching `TagEnd::*`, and `Alignment::{None,Left,Center,Right}`.

- [ ] **Step 1: Write the failing tests** — add to `mod tests`:

```rust
    #[test]
    fn parses_gfm_table_with_alignment_and_inline() {
        let src = "| H1 | H2 |\n| :--- | ---: |\n| a | **b** |\n";
        let d = parse(src).unwrap();
        assert_eq!(types(&d.blocks), vec!["table"]);
        let t = &d.blocks[0];
        assert_eq!(t.attrs, json!({ "align": ["left", "right"] }));
        // children[0] is the header row; children[1] the body row.
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].r#type, "table_row");
        assert_eq!(t.children[0].children.len(), 2);
        assert_eq!(t.children[0].children[0].r#type, "table_cell");
        assert_eq!(t.children[0].children[0].text.as_deref(), Some("H1"));
        // inline strong is preserved as markdown source in the cell text.
        assert_eq!(t.children[1].children[1].text.as_deref(), Some("**b**"));
    }

    #[test]
    fn parses_table_cell_escaped_pipe() {
        let src = "| A |\n| --- |\n| x \\| y |\n";
        let d = parse(src).unwrap();
        let t = &d.blocks[0];
        // pulldown unescapes `\|` to a literal pipe inside the cell text.
        assert_eq!(t.children[1].children[0].text.as_deref(), Some("x | y"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-core parses_gfm_table parses_table_cell_escaped`
Expected: FAIL (tables not enabled → the source parses as paragraphs, no `table` block).

- [ ] **Step 3: Update imports** at the top of `parser.rs`:

```rust
use pulldown_cmark::{
    html, Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
```

- [ ] **Step 4: Enable tables in both parser construction sites.** In `parse_plain_markdown`:

```rust
fn parse_plain_markdown(body: &str) -> Result<Vec<Block>, ParseError> {
    let mut parser = Parser::new_ext(body, Options::ENABLE_TABLES);
    parse_blocks(&mut parser, None)
}
```

In `render_markdown`:

```rust
pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}
```

- [ ] **Step 5: Add the `Tag::Table` arm to `parse_one`** (place it near the other `Event::Start(Tag::...)` arms, e.g. after the `Event::Start(Tag::List(first))` arm):

```rust
        Event::Start(Tag::Table(alignments)) => parse_table(alignments, parser)?,
```

- [ ] **Step 6: Add the `parse_table` and `consume_table_cell` helpers** (place them after `parse_item`, before `consume_inline`):

```rust
/// Map a pulldown alignment to the lopress `attrs.align` string.
fn align_str(a: Alignment) -> &'static str {
    match a {
        Alignment::None => "none",
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
    }
}

/// Build a `table` block from a `Tag::Table` start event. The first emitted
/// row (inside `TableHead`) and each subsequent `TableRow` become `table_row`
/// children; the first child is the header. Cells are `table_cell` blocks whose
/// `text` holds inline-markdown source (mirroring `consume_inline`).
fn parse_table(alignments: Vec<Alignment>, parser: &mut Parser<'_>) -> Result<Block, ParseError> {
    let align: Vec<Value> = alignments
        .into_iter()
        .map(|a| Value::String(align_str(a).to_string()))
        .collect();
    let mut rows: Vec<Block> = Vec::new();
    let mut current_cells: Vec<Block> = Vec::new();
    while let Some(ev) = parser.next() {
        match ev {
            Event::Start(Tag::TableCell) => {
                let text = consume_table_cell(parser);
                current_cells.push(Block {
                    r#type: "table_cell".into(),
                    attrs: json!({}),
                    children: vec![],
                    text: Some(text),
                });
            }
            // End of a head or body row: flush the accumulated cells as a row.
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                rows.push(Block {
                    r#type: "table_row".into(),
                    attrs: json!({}),
                    children: std::mem::take(&mut current_cells),
                    text: None,
                });
            }
            Event::End(TagEnd::Table) => break,
            // TableHead/TableRow starts carry no data; ignore everything else.
            _ => {}
        }
    }
    Ok(Block {
        r#type: "table".into(),
        attrs: json!({ "align": align }),
        children: rows,
        text: None,
    })
}

/// Accumulate one table cell's inline content as markdown source, until the
/// matching `TagEnd::TableCell`. Inline conversions mirror `consume_inline`
/// (emphasis → `*`, strong → `**`, code → backticks, link → its text).
fn consume_table_cell(parser: &mut Parser<'_>) -> String {
    let mut text = String::new();
    while let Some(ev) = parser.next() {
        match ev {
            Event::Text(t) => text.push_str(&t),
            Event::Code(t) => {
                text.push('`');
                text.push_str(&t);
                text.push('`');
            }
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => text.push('*'),
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => text.push_str("**"),
            Event::Start(Tag::Link { .. }) => {
                for inner in parser.by_ref() {
                    match inner {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::Link) => break,
                        _ => {}
                    }
                }
            }
            Event::End(TagEnd::TableCell) => break,
            _ => {}
        }
    }
    text
}
```

- [ ] **Step 7: Run to verify they pass**

Run: `cargo test -p lopress-core parses_gfm_table parses_table_cell_escaped`
Expected: PASS. Also run `cargo test -p lopress-core` to confirm no regression in existing parser tests.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-core/src/parser.rs
git commit -m "feat(core): parse GFM tables into a table block tree"
```

---

## Task 4: Core serializer — emit `---` and GFM tables

**Files:**
- Modify: `crates/lopress-core/src/serializer.rs` (`write_block` match; tests)

- [ ] **Step 1: Write the failing tests** — add to `mod tests`:

```rust
    #[test]
    fn serializes_separator() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "separator".into(),
                attrs: serde_json::json!({}),
                children: vec![],
                text: None,
            }],
        };
        assert_eq!(serialize(&doc), "---\n");
    }

    #[test]
    fn separator_roundtrips() {
        let src = "a\n\n---\n\nb\n";
        let d = parse(src).unwrap();
        let once = serialize(&d);
        let twice = serialize(&parse(&once).unwrap());
        assert_eq!(once, twice);
        assert!(once.contains("---\n"));
    }

    #[test]
    fn table_roundtrips_with_alignment_and_inline() {
        let src = "| H1 | H2 |\n| :--- | ---: |\n| a | **b** |\n";
        let d = parse(src).unwrap();
        let once = serialize(&d);
        let reparsed = parse(&once).unwrap();
        assert_eq!(reparsed.blocks.len(), 1);
        assert_eq!(reparsed.blocks[0].r#type, "table");
        // Stable round-trip.
        assert_eq!(serialize(&reparsed), once);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-core serializes_separator separator_roundtrips table_roundtrips`
Expected: FAIL (no `separator`/`table` arm → they hit the `_ =>` "unknown block" arm).

- [ ] **Step 3: Add the `separator` and `table` arms to `write_block`** (insert before the `custom if custom.starts_with("lopress:")` arm):

```rust
        "separator" => {
            out.push_str("---\n");
        }
        "table" => {
            write_table(out, b);
        }
```

- [ ] **Step 4: Add the `write_table` helper** (place after `write_block`, before `is_empty_attrs`):

```rust
/// Serialize a `table` block to GFM. `children[0]` is the header row; the
/// alignment delimiter row is derived from `attrs.align`. Pipe characters in
/// cell text are escaped as `\|`.
fn write_table(out: &mut String, b: &Block) {
    let aligns: Vec<&str> = b
        .attrs
        .get("align")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap_or("none"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let cell_text = |cell: &Block| -> String { cell.text.as_deref().unwrap_or("").replace('|', "\\|") };
    let write_row = |out: &mut String, row: &Block| {
        out.push('|');
        for cell in &row.children {
            out.push(' ');
            out.push_str(&cell_text(cell));
            out.push_str(" |");
        }
        out.push('\n');
    };

    let mut rows = b.children.iter();
    // Header row.
    let Some(header) = rows.next() else { return };
    write_row(out, header);
    // Alignment delimiter row — one entry per header column.
    out.push('|');
    for i in 0..header.children.len() {
        let token = match aligns.get(i).copied().unwrap_or("none") {
            "left" => ":---",
            "right" => "---:",
            "center" => ":---:",
            _ => "---",
        };
        out.push(' ');
        out.push_str(token);
        out.push_str(" |");
    }
    out.push('\n');
    // Body rows.
    for row in rows {
        write_row(out, row);
    }
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-core serializes_separator separator_roundtrips table_roundtrips`
Expected: PASS. Run `cargo test -p lopress-core` for no regressions.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/src/serializer.rs
git commit -m "feat(core): serialize separator and table blocks to markdown"
```

---

## Task 5: Build render — `<hr>` and `<table>`

**Files:**
- Modify: `crates/lopress-build/src/render.rs` (`write_block` match; new `write_table` helper; tests)

**Context:** `render.rs::write_block` renders the `Block` tree directly to HTML with `escape()` on text (it does NOT render inline markdown — that's a pre-existing gap shared by all blocks and out of scope).

- [ ] **Step 1: Write the failing tests** — add to `mod tests`:

```rust
    #[test]
    fn renders_separator_as_hr() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "separator".into(),
                attrs: json!({}),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &empty_registry(), &Tera::default(), &ImageIndex::default()).unwrap();
        assert_eq!(html, "<hr>\n");
    }

    #[test]
    fn renders_table_with_alignment() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "table".into(),
                attrs: json!({ "align": ["left", "right"] }),
                children: vec![
                    Block {
                        r#type: "table_row".into(),
                        attrs: json!({}),
                        children: vec![
                            Block { r#type: "table_cell".into(), attrs: json!({}), children: vec![], text: Some("H1".into()) },
                            Block { r#type: "table_cell".into(), attrs: json!({}), children: vec![], text: Some("H2".into()) },
                        ],
                        text: None,
                    },
                    Block {
                        r#type: "table_row".into(),
                        attrs: json!({}),
                        children: vec![
                            Block { r#type: "table_cell".into(), attrs: json!({}), children: vec![], text: Some("a".into()) },
                            Block { r#type: "table_cell".into(), attrs: json!({}), children: vec![], text: Some("b & c".into()) },
                        ],
                        text: None,
                    },
                ],
                text: None,
            }],
        };
        let html = render_body(&doc, &empty_registry(), &Tera::default(), &ImageIndex::default()).unwrap();
        assert!(html.contains("<table>"), "got: {html}");
        assert!(html.contains("<thead>"));
        assert!(html.contains(r#"<th style="text-align:left">H1</th>"#), "got: {html}");
        assert!(html.contains(r#"<th style="text-align:right">H2</th>"#), "got: {html}");
        assert!(html.contains("<tbody>"));
        assert!(html.contains(r#"<td style="text-align:left">a</td>"#));
        assert!(html.contains("b &amp; c"), "cell text escaped: {html}");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-build renders_separator_as_hr renders_table_with_alignment`
Expected: FAIL (both hit the `other =>` "unknown block" comment arm).

- [ ] **Step 3: Add `separator` and `table` arms to `write_block`** (before the `"lopress:more"` arm):

```rust
        "separator" => {
            out.push_str("<hr>\n");
        }
        "table" => {
            write_table(out, b);
        }
```

- [ ] **Step 4: Add the `write_table` helper** (place after `write_image`, before `render_custom`):

```rust
/// Render a `table` block to `<table>`. `children[0]` is the header row
/// (`<th>`); the rest are body rows (`<td>`). Per-column `text-align` comes
/// from `attrs.align`. Cell text is escaped (inline-md→HTML is out of scope).
fn write_table(out: &mut String, b: &Block) {
    let aligns: Vec<&str> = b
        .attrs
        .get("align")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(|v| v.as_str().unwrap_or("none")).collect())
        .unwrap_or_default();
    let style_for = |col: usize| -> String {
        match aligns.get(col).copied().unwrap_or("none") {
            "left" => " style=\"text-align:left\"".to_string(),
            "right" => " style=\"text-align:right\"".to_string(),
            "center" => " style=\"text-align:center\"".to_string(),
            _ => String::new(),
        }
    };

    out.push_str("<table>\n");
    let mut rows = b.children.iter();
    if let Some(header) = rows.next() {
        out.push_str("<thead>\n<tr>");
        for (col, cell) in header.children.iter().enumerate() {
            let txt = escape(cell.text.as_deref().unwrap_or(""));
            let _ = write!(out, "<th{}>{txt}</th>", style_for(col));
        }
        out.push_str("</tr>\n</thead>\n");
    }
    out.push_str("<tbody>\n");
    for row in rows {
        out.push_str("<tr>");
        for (col, cell) in row.children.iter().enumerate() {
            let txt = escape(cell.text.as_deref().unwrap_or(""));
            let _ = write!(out, "<td{}>{txt}</td>", style_for(col));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}
```

`write!` and `writeln!` are already imported via `use std::fmt::Write;` at the top of the file.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p lopress-build renders_separator_as_hr renders_table_with_alignment`
Expected: PASS. Run `cargo test -p lopress-build` for no regressions.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/render.rs
git commit -m "feat(build): render separator as <hr> and table as <table>"
```

---

## Task 6: Editor model — Table/Separator types, constructors, conversions, round-trip

This is the largest single compiling unit: adding `BlockKind::Table` + `BlockBody::Table` breaks several exhaustive matches, so the type addition, the new constructors, the `from_core`/`to_core` arms, and the exhaustive-match arms in `actions.rs`/`sync.rs` must land together for the crate to compile. (The render dispatch in `plugin.rs`/`mod.rs` already has `_` fallbacks, so it compiles without changes; the widgets come in Tasks 8–9.)

**Files:**
- Modify: `crates/lopress-editor/src/model/types.rs` (new enum variants, `TableData`/`TableRow`/`TableCell`/`Align`, constructors)
- Modify: `crates/lopress-editor/src/model/from_core.rs` (`native_block_from_core` arms)
- Modify: `crates/lopress-editor/src/model/to_core.rs` (`native_block_to_core` arm)
- Modify: `crates/lopress-editor/src/actions.rs` (exhaustive-match arms in `apply_split`, `coerce_body_to_kind`, `body_matches_kind`, `body_to_flat_text`)
- Modify: `crates/lopress-editor/src/model/sync.rs` (`canonicalize_body`)
- Test: add a `#[cfg(test)] mod table_roundtrip_tests` in `from_core.rs` (or a new file `crates/lopress-editor/tests/table_roundtrip_tests.rs`)

- [ ] **Step 1: Add the table model types** to `crates/lopress-editor/src/model/types.rs`. Extend `BlockKind` and `BlockBody`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8), // 1..=6
    Code { lang: Rc<str> },
    List { ordered: bool },
    Image,
    Table,
    Opaque { type_name: Rc<str> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockBody {
    Inline(Vec<InlineRun>),
    Code(String),
    List(Vec<ListItem>),
    Table(TableData),
    Opaque(Value),
}
```

Add the supporting types (next to `ListItem`):

```rust
/// Column alignment for a table. Maps to the `attrs.align` strings on disk
/// ("none"/"left"/"center"/"right") and to GFM delimiter-row tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Align::None => "none",
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s {
            "left" => Align::Left,
            "center" => Align::Center,
            "right" => Align::Right,
            _ => Align::None,
        }
    }
}

/// One table cell: an id (for focus) plus its inline runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub id: BlockId,
    pub runs: Vec<InlineRun>,
}

/// One table row: an id plus its cells. `rows[0]` of a `TableData` is the header.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub id: BlockId,
    pub cells: Vec<TableCell>,
}

/// A table body: per-column alignment plus the rows (row 0 = header).
#[derive(Debug, Clone, PartialEq)]
pub struct TableData {
    pub align: Vec<Align>,
    pub rows: Vec<TableRow>,
}
```

- [ ] **Step 2: Add `PluginMeta::separator()` and `PluginMeta::table()`** (next to `read_more()`/`image()` in the `impl PluginMeta` block):

```rust
    /// `PluginMeta` for the separator: a native `separator` claim, built-in
    /// (chrome suppressed), edited via the `"separator"` divider widget. No attrs.
    pub fn separator() -> Self {
        Self {
            block_type_name: Rc::from("separator"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("separator")),
            native: Some(Rc::from("separator")),
        }
    }

    /// `PluginMeta` for a table: native `table` claim, built-in (chrome
    /// suppressed), edited via the `"table"` widget. No attr-form attrs (the
    /// align array lives in the table body, not the attr form).
    pub fn table() -> Self {
        Self {
            block_type_name: Rc::from("table"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("table")),
            native: Some(Rc::from("table")),
        }
    }
```

- [ ] **Step 3: Add `EditorBlock::separator()`, `EditorBlock::table()`, `EditorBlock::table_default()`** (in the `impl EditorBlock` block):

```rust
    /// The separator block: an empty-bodied plugin block carrying
    /// `PluginMeta::separator`. Renders via its divider widget and serializes
    /// to a bare `---`.
    pub fn separator() -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(vec![]),
            plugin: Some(PluginMeta::separator()),
        }
    }

    /// A table block from explicit data.
    pub fn table(data: TableData) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Table,
            body: BlockBody::Table(data),
            plugin: Some(PluginMeta::table()),
        }
    }

    /// The default inserted table: 2 columns × 2 rows (1 header + 1 body),
    /// empty cells, alignment `none`. Used by both the slash menu and the
    /// toolbar button.
    pub fn table_default() -> Self {
        let empty_cell = || TableCell {
            id: BlockId::new(),
            runs: vec![],
        };
        let row = || TableRow {
            id: BlockId::new(),
            cells: vec![empty_cell(), empty_cell()],
        };
        Self::table(TableData {
            align: vec![Align::None, Align::None],
            rows: vec![row(), row()],
        })
    }
```

- [ ] **Step 4: Fix the exhaustive matches in `actions.rs`.** Make these edits:

In `apply_split`, the `match body { ... }` — add before `BlockBody::Opaque(_) => None,`:

```rust
        BlockBody::Table(_) => None,
```

In `apply_change_type`, add an early guard right after the existing Opaque guard (the `if matches!(block.body, BlockBody::Opaque(_)) { return None; }`):

```rust
    // A table body has no sensible conversion to another kind, and the kind-
    // cycler toolbar buttons would otherwise leave a (Paragraph, Table)
    // mismatch that renders as an empty gap. Treat ChangeType on a table as a
    // no-op, exactly like the Opaque guard above.
    if matches!(block.body, BlockBody::Table(_)) {
        return None;
    }
```

In `body_matches_kind`, add `(Table, BlockBody::Table)` to the `matches!`:

```rust
fn body_matches_kind(kind: &BlockKind, body: &BlockBody) -> bool {
    matches!(
        (kind, body),
        (
            BlockKind::Paragraph | BlockKind::Heading(_),
            BlockBody::Inline(_)
        ) | (BlockKind::Code { .. }, BlockBody::Code(_))
            | (BlockKind::List { .. }, BlockBody::List(_))
            | (BlockKind::Table, BlockBody::Table(_))
            | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
    )
}
```

In `body_to_flat_text`, add a Table arm (join cells with tab within a row, rows with newline):

```rust
        BlockBody::Table(data) => data
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|c| c.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"),
```

In `coerce_body_to_kind`, the first (shape-already-matches) arm — add `(BlockKind::Table, BlockBody::Table(_))`:

```rust
        (BlockKind::Paragraph | BlockKind::Heading(_), BlockBody::Inline(_))
        | (BlockKind::Code { .. }, BlockBody::Code(_))
        | (BlockKind::List { .. }, BlockBody::List(_))
        | (BlockKind::Table, BlockBody::Table(_))
        | (BlockKind::Opaque { .. }, BlockBody::Opaque(_))
        | (BlockKind::Image, BlockBody::Opaque(_)) => body,
```

and add a catch-all arm for `BlockKind::Table` (before or alongside the `BlockKind::Image` catch-all near the end):

```rust
        // Table: body is always Table; any mismatch is a programming error —
        // return the body as-is rather than panic. No widget commits a
        // non-Table body into a Table block.
        (BlockKind::Table, _) => body,
```

Make sure the `use` line in `actions.rs` still imports what's needed — it already imports `BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta`. No `TableData` import is needed here (the arms match `BlockBody::Table(_)` and `data` is bound positionally).

- [ ] **Step 5: Fix `canonicalize_body` in `sync.rs`.** Add a Table arm (canonicalize each cell's runs, preserve ids):

```rust
        BlockBody::Table(data) => BlockBody::Table(crate::model::types::TableData {
            align: data.align.clone(),
            rows: data
                .rows
                .iter()
                .map(|row| crate::model::types::TableRow {
                    id: row.id,
                    cells: row
                        .cells
                        .iter()
                        .map(|cell| crate::model::types::TableCell {
                            id: cell.id,
                            runs: canonicalize_runs(&cell.runs),
                        })
                        .collect(),
                })
                .collect(),
        }),
```

(Place it among the existing `BlockBody` arms in `canonicalize_body`.)

- [ ] **Step 6: Add the `from_core` arms.** In `crates/lopress-editor/src/model/from_core.rs`, extend `native_block_from_core`'s match to add `separator` and `table`:

```rust
fn native_block_from_core(b: &Block, decl: &BlockDecl) -> EditorBlock {
    match decl.editor.as_deref() {
        Some("list") => native_list_from_core(b, decl),
        Some("code") => native_code_from_core(b, decl),
        Some("image") => native_image_from_core(b, decl),
        Some("separator") => EditorBlock::separator(),
        Some("table") => native_table_from_core(b),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

Add the `native_table_from_core` helper (after `native_image_from_core`):

```rust
/// Build a table `EditorBlock` from a core `table` block. A well-formed table
/// has only `table_row` children, each with only `table_cell` children whose
/// content is inline text. A malformed table degrades to `Opaque` so it
/// round-trips verbatim (mirrors `native_list_from_core`).
fn native_table_from_core(b: &Block) -> EditorBlock {
    use crate::model::types::{Align, TableCell, TableData, TableRow};

    let align: Vec<Align> = b
        .attrs
        .get("align")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| Align::from_str_lenient(v.as_str().unwrap_or("none")))
                .collect()
        })
        .unwrap_or_default();

    let rows: Option<Vec<TableRow>> = b
        .children
        .iter()
        .map(|row| {
            if row.r#type != "table_row" {
                return None;
            }
            let cells: Option<Vec<TableCell>> = row
                .children
                .iter()
                .map(|cell| {
                    if cell.r#type != "table_cell" || !cell.children.is_empty() {
                        return None;
                    }
                    Some(TableCell {
                        id: BlockId::new(),
                        runs: parse_inline(cell.text.as_deref().unwrap_or("")),
                    })
                })
                .collect();
            cells.map(|cells| TableRow {
                id: BlockId::new(),
                cells,
            })
        })
        .collect();

    match rows {
        // A table needs at least one row (the header).
        Some(rows) if !rows.is_empty() => EditorBlock::table(TableData { align, rows }),
        _ => EditorBlock::opaque(
            b.r#type.clone(),
            serde_json::to_value(b).unwrap_or(serde_json::Value::Null),
        ),
    }
}
```

- [ ] **Step 7: Add the `to_core` arm.** In `crates/lopress-editor/src/model/to_core.rs`, `native_block_to_core` currently matches `&b.body { List, Code, _ }`. Add a `Table` arm before the `_`:

```rust
        BlockBody::Table(data) => {
            let align: Vec<Value> = data
                .align
                .iter()
                .map(|a| Value::String(a.as_str().to_string()))
                .collect();
            let rows: Vec<Block> = data
                .rows
                .iter()
                .map(|row| Block {
                    r#type: "table_row".into(),
                    attrs: empty_attrs(),
                    children: row
                        .cells
                        .iter()
                        .map(|cell| Block {
                            r#type: "table_cell".into(),
                            attrs: empty_attrs(),
                            children: vec![],
                            text: Some(serialize_inline(&cell.runs)),
                        })
                        .collect(),
                    text: None,
                })
                .collect();
            Block {
                r#type: core_type.to_string(),
                attrs: json!({ "align": align }),
                children: rows,
                text: None,
            }
        }
```

(`Value`, `json!`, `serialize_inline`, `empty_attrs` are already in scope in `to_core.rs`.)

- [ ] **Step 8: Write the round-trip tests.** Create `crates/lopress-editor/tests/table_separator_roundtrip_tests.rs`:

```rust
//! Editor model round-trip for separator and table blocks.
use lopress_core::parser::parse;
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_core::serializer::serialize;
use lopress_plugin::PluginRegistry;

fn registry() -> PluginRegistry {
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    reg
}

fn roundtrip(src: &str) -> String {
    let core = parse(src).unwrap();
    let editor_doc = doc_from_core(&core, &registry());
    let back = doc_to_core(&editor_doc);
    serialize(&back)
}

#[test]
fn separator_survives_editor_roundtrip() {
    let out = roundtrip("a\n\n---\n\nb\n");
    assert!(out.contains("---\n"), "got: {out}");
    assert!(out.contains("a"));
    assert!(out.contains("b"));
}

#[test]
fn table_survives_editor_roundtrip() {
    let src = "| H1 | H2 |\n| :--- | ---: |\n| a | **b** |\n";
    let out = roundtrip(src);
    // Re-parse the output and confirm it is still one table with the same shape.
    let reparsed = parse(&out).unwrap();
    assert_eq!(reparsed.blocks.len(), 1);
    assert_eq!(reparsed.blocks[0].r#type, "table");
    assert_eq!(reparsed.blocks[0].attrs, serde_json::json!({ "align": ["left", "right"] }));
    assert_eq!(reparsed.blocks[0].children[1].children[1].text.as_deref(), Some("**b**"));
}
```

Confirm the crate exposes these paths publicly (`lopress_editor::model::from_core`, `::to_core`). If a path is private, either use the existing test module convention in the crate (an in-crate `#[cfg(test)] mod`) or add `pub use`. Check `crates/lopress-editor/src/lib.rs` and `model/mod.rs` for what's already `pub`; the existing `tests/from_to_core_tests.rs` shows the canonical import paths — mirror it.

- [ ] **Step 9: Run to verify it compiles and passes**

Run: `cargo test -p lopress-editor table_separator_roundtrip`
Expected: PASS (and the crate compiles — confirming all exhaustive matches were updated).

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-editor/src/model/types.rs crates/lopress-editor/src/model/from_core.rs crates/lopress-editor/src/model/to_core.rs crates/lopress-editor/src/actions.rs crates/lopress-editor/src/model/sync.rs crates/lopress-editor/tests/table_separator_roundtrip_tests.rs
git commit -m "feat(editor): table/separator model types and core round-trip"
```

---

## Task 7: Editor — table mutation actions (rows, columns, alignment) + undo

**Files:**
- Modify: `crates/lopress-editor/src/actions.rs` (new `BlockAction` variants, dispatch arms, apply functions; the `size_tests` guard)
- Test: a new `#[cfg(test)] mod table_action_tests` in `actions.rs`

**Design:** Five new variants. Each operates on a `Table`-bodied block, mutates `TableData`, and returns an `(canonical, inverse)` pair. Keep payloads small (indices + a small `Align`) so the `size_of::<BlockAction>() <= 40` guard still holds — these carry only a `BlockId` + `usize`(es) + `Align`, all small, so no boxing is required (verify with the size test in Step 6).

- [ ] **Step 1: Write the failing tests** — add `mod table_action_tests` to `actions.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod table_action_tests {
    use super::*;
    use crate::model::types::{Align, EditorBlock, EditorDoc};

    fn doc_with_table() -> EditorDoc {
        EditorDoc {
            blocks: vec![EditorBlock::table_default()], // 2x2
            front_matter: lopress_core::FrontMatter::default(),
        }
    }

    fn table_data(doc: &EditorDoc) -> crate::model::types::TableData {
        match &doc.blocks[0].body {
            BlockBody::Table(d) => d.clone(),
            _ => panic!("expected table body"),
        }
    }

    #[test]
    fn insert_row_appends_and_undoes() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        let (_c, inverse) = apply(&mut doc, BlockAction::TableInsertRow { block_id: id, at: 2 }).unwrap();
        assert_eq!(table_data(&doc).rows.len(), 3);
        apply(&mut doc, inverse);
        assert_eq!(table_data(&doc).rows.len(), 2);
    }

    #[test]
    fn delete_row_refuses_header_and_last_body() {
        let mut doc = doc_with_table(); // header + 1 body row
        let id = doc.blocks[0].id;
        // Deleting the header (row 0) is refused.
        assert!(apply(&mut doc, BlockAction::TableDeleteRow { block_id: id, row: 0 }).is_none());
        // Deleting the only body row is refused (must keep >= 1 body row).
        assert!(apply(&mut doc, BlockAction::TableDeleteRow { block_id: id, row: 1 }).is_none());
        assert_eq!(table_data(&doc).rows.len(), 2);
    }

    #[test]
    fn insert_and_delete_column_roundtrip() {
        let mut doc = doc_with_table(); // 2 columns
        let id = doc.blocks[0].id;
        let (_c, inv) = apply(&mut doc, BlockAction::TableInsertColumn { block_id: id, at: 2 }).unwrap();
        assert_eq!(table_data(&doc).align.len(), 3);
        assert!(table_data(&doc).rows.iter().all(|r| r.cells.len() == 3));
        apply(&mut doc, inv);
        assert_eq!(table_data(&doc).align.len(), 2);
        assert!(table_data(&doc).rows.iter().all(|r| r.cells.len() == 2));
    }

    #[test]
    fn delete_column_refuses_last() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        apply(&mut doc, BlockAction::TableDeleteColumn { block_id: id, col: 0 }).unwrap();
        assert_eq!(table_data(&doc).align.len(), 1);
        // Refuse to delete the last remaining column.
        assert!(apply(&mut doc, BlockAction::TableDeleteColumn { block_id: id, col: 0 }).is_none());
    }

    #[test]
    fn set_align_and_undo() {
        let mut doc = doc_with_table();
        let id = doc.blocks[0].id;
        let (_c, inv) = apply(&mut doc, BlockAction::TableSetAlign { block_id: id, col: 1, align: Align::Center }).unwrap();
        assert_eq!(table_data(&doc).align[1], Align::Center);
        apply(&mut doc, inv);
        assert_eq!(table_data(&doc).align[1], Align::None);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p lopress-editor table_action_tests`
Expected: FAIL to compile (variants don't exist yet).

- [ ] **Step 3: Add the five `BlockAction` variants** to the enum (after `EditFrontMatter`):

```rust
    /// Insert an empty row at index `at` (0..=rows.len()). New cells match the
    /// current column count. `at == 0` would insert above the header — callers
    /// pass `at >= 1`; the apply clamps into the body region defensively.
    TableInsertRow { block_id: BlockId, at: usize },
    /// Delete body row `row`. No-op (returns None) for the header row (0) or
    /// when it is the last remaining body row.
    TableDeleteRow { block_id: BlockId, row: usize },
    /// Insert an empty column at index `at` (0..=col_count) across every row,
    /// with `Align::None`.
    TableInsertColumn { block_id: BlockId, at: usize },
    /// Delete column `col` across every row. No-op when it is the last column.
    TableDeleteColumn { block_id: BlockId, col: usize },
    /// Set column `col`'s alignment.
    TableSetAlign { block_id: BlockId, col: usize, align: Align },
```

Add `Align` to the `use crate::model::types::{...}` import at the top of `actions.rs`.

- [ ] **Step 4: Add dispatch arms** to the `apply` match (before the closing `}`):

```rust
        BlockAction::TableInsertRow { block_id, at } => apply_table_insert_row(doc, block_id, at),
        BlockAction::TableDeleteRow { block_id, row } => apply_table_delete_row(doc, block_id, row),
        BlockAction::TableInsertColumn { block_id, at } => {
            apply_table_insert_column(doc, block_id, at)
        }
        BlockAction::TableDeleteColumn { block_id, col } => {
            apply_table_delete_column(doc, block_id, col)
        }
        BlockAction::TableSetAlign {
            block_id,
            col,
            align,
        } => apply_table_set_align(doc, block_id, col, align),
```

- [ ] **Step 5: Add the apply functions** (place near `apply_edit_attrs`). They share a helper to borrow the table body:

```rust
fn table_body_mut(doc: &mut EditorDoc, id: BlockId) -> Option<&mut crate::model::types::TableData> {
    let idx = find_idx(doc, id)?;
    match &mut doc.blocks.get_mut(idx)?.body {
        BlockBody::Table(data) => Some(data),
        _ => None,
    }
}

fn apply_table_insert_row(
    doc: &mut EditorDoc,
    id: BlockId,
    at: usize,
) -> Option<(BlockAction, BlockAction)> {
    use crate::model::types::{BlockId as Bid, TableCell, TableRow};
    let data = table_body_mut(doc, id)?;
    let cols = data.align.len();
    // Never insert above the header row.
    let at = at.clamp(1, data.rows.len());
    let new_row = TableRow {
        id: Bid::new(),
        cells: (0..cols)
            .map(|_| TableCell {
                id: Bid::new(),
                runs: vec![],
            })
            .collect(),
    };
    data.rows.insert(at, new_row);
    Some((
        BlockAction::TableInsertRow { block_id: id, at },
        BlockAction::TableDeleteRow {
            block_id: id,
            row: at,
        },
    ))
}

fn apply_table_delete_row(
    doc: &mut EditorDoc,
    id: BlockId,
    row: usize,
) -> Option<(BlockAction, BlockAction)> {
    let data = table_body_mut(doc, id)?;
    // Refuse: the header (row 0), an out-of-range row, or the last body row.
    if row == 0 || row >= data.rows.len() || data.rows.len() <= 2 {
        return None;
    }
    let removed = data.rows.remove(row);
    Some((
        BlockAction::TableDeleteRow { block_id: id, row },
        // Inverse: reinsert the exact removed row at the same index. We model
        // it as a generic body restore so the cells/ids come back intact.
        rebuild_inverse_for_row(id, row, removed, doc),
    ))
}
```

Because re-inserting a *specific* removed row (with its original cell ids/content) isn't expressible by `TableInsertRow` (which inserts an empty row), model the delete-row inverse as an `EditBlockBody` restoring the pre-delete body. Replace the two delete functions to capture the whole body for the inverse instead — simpler and id-stable:

```rust
fn apply_table_delete_row(
    doc: &mut EditorDoc,
    id: BlockId,
    row: usize,
) -> Option<(BlockAction, BlockAction)> {
    let before = {
        let data = table_body_mut(doc, id)?;
        if row == 0 || row >= data.rows.len() || data.rows.len() <= 2 {
            return None;
        }
        BlockBody::Table(data.clone())
    };
    let data = table_body_mut(doc, id)?;
    data.rows.remove(row);
    Some((
        BlockAction::TableDeleteRow { block_id: id, row },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(before),
            built_in: true,
        },
    ))
}

fn apply_table_insert_column(
    doc: &mut EditorDoc,
    id: BlockId,
    at: usize,
) -> Option<(BlockAction, BlockAction)> {
    use crate::model::types::{Align, BlockId as Bid, TableCell};
    let data = table_body_mut(doc, id)?;
    let at = at.min(data.align.len());
    data.align.insert(at, Align::None);
    for row in &mut data.rows {
        let col = at.min(row.cells.len());
        row.cells.insert(
            col,
            TableCell {
                id: Bid::new(),
                runs: vec![],
            },
        );
    }
    Some((
        BlockAction::TableInsertColumn { block_id: id, at },
        BlockAction::TableDeleteColumn {
            block_id: id,
            col: at,
        },
    ))
}

fn apply_table_delete_column(
    doc: &mut EditorDoc,
    id: BlockId,
    col: usize,
) -> Option<(BlockAction, BlockAction)> {
    let before = {
        let data = table_body_mut(doc, id)?;
        if col >= data.align.len() || data.align.len() <= 1 {
            return None;
        }
        BlockBody::Table(data.clone())
    };
    let data = table_body_mut(doc, id)?;
    data.align.remove(col);
    for row in &mut data.rows {
        if col < row.cells.len() {
            row.cells.remove(col);
        }
    }
    Some((
        BlockAction::TableDeleteColumn { block_id: id, col },
        BlockAction::EditBlockBody {
            block_id: id,
            new_body: Box::new(before),
            built_in: true,
        },
    ))
}

fn apply_table_set_align(
    doc: &mut EditorDoc,
    id: BlockId,
    col: usize,
    align: crate::model::types::Align,
) -> Option<(BlockAction, BlockAction)> {
    let data = table_body_mut(doc, id)?;
    let old = *data.align.get(col)?;
    if old == align {
        return None;
    }
    let slot = data.align.get_mut(col)?;
    *slot = align;
    Some((
        BlockAction::TableSetAlign {
            block_id: id,
            col,
            align,
        },
        BlockAction::TableSetAlign {
            block_id: id,
            col,
            align: old,
        },
    ))
}
```

> Delete the first (placeholder) `apply_table_delete_row` + `rebuild_inverse_for_row` draft from Step 5's first block — only the `EditBlockBody`-inverse versions above are kept. Do not define `rebuild_inverse_for_row`.

- [ ] **Step 6: Update the size guard.** Run the existing `block_action_size_is_compact` test:

Run: `cargo test -p lopress-editor block_action_size_is_compact`
Expected: PASS (the new variants carry `BlockId` + `usize`(es) + `Align`, all small — the enum should stay ≤ 40 bytes; the largest existing variant is already boxed). If it unexpectedly fails, box the offending payload behind a struct and report.

- [ ] **Step 7: Run the table action tests**

Run: `cargo test -p lopress-editor table_action_tests`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/lopress-editor/src/actions.rs
git commit -m "feat(editor): table row/column/alignment actions with undo"
```

---

## Task 8: Separator widget + registry

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/separator.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (`pub mod separator;`)
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` (`editor_for` arm + use)

- [ ] **Step 1: Create `crates/lopress-editor/src/ui/blocks/separator.rs`** — modeled on `read_more.rs`, a full-width rule with no label:

```rust
//! The separator block's editor widget: a slim, full-width horizontal rule.
//! It ignores the (empty) body and is focusable on PointerDown so the block
//! can be selected and deleted via the toolbar — mirroring `read_more.rs`.

use crate::ui::blocks::editor_registry::EditorContext;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{empty, Decorators};
use floem::{AnyView, IntoView};

const RULE: Color = Color::rgb8(180, 180, 188);

pub fn separator_widget(ctx: &EditorContext) -> AnyView {
    let block_id = ctx.block.id;
    let focus_pub = ctx.focus_pub;
    empty()
        .style(move |s| {
            s.width_full()
                .height(1.)
                .margin_vert(10.)
                .background(RULE)
        })
        .on_event(EventListener::PointerDown, move |_| {
            focus_pub.block.set(Some(block_id));
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        })
        .into_any()
}
```

> Verify against `read_more.rs`: the `EditorContext` field names (`block`, `focus_pub`) and `focus_pub.block` / `focus_pub.editor_and_spans` signals are used exactly as `read_more_widget` uses them. If `empty()` cannot be styled to a visible 1px bar in floem 0.2, use a `label(|| String::new())` with the same style (as read_more does with a label).

- [ ] **Step 2: Register the module** — add to `crates/lopress-editor/src/ui/blocks/mod.rs` (alphabetically near `read_more`):

```rust
pub mod separator;
```

- [ ] **Step 3: Register the widget** in `editor_registry.rs` — add the `use` and the match arm:

```rust
use crate::ui::blocks::{code_editor, image, list, read_more, separator};
```

```rust
        "more" => Some(read_more::read_more_widget),
        "separator" => Some(separator::separator_widget),
        "image" => Some(image::image_widget),
```

- [ ] **Step 4: Add a registry test** in `editor_registry.rs` `mod tests`:

```rust
    #[test]
    fn editor_for_resolves_separator() {
        assert!(editor_for("separator").is_some());
    }
```

- [ ] **Step 5: Run to verify it compiles and passes**

Run: `cargo test -p lopress-editor editor_for_resolves_separator`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/separator.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "feat(editor): separator divider widget"
```

---

## Task 9: Table widget + registry

**Files:**
- Create: `crates/lopress-editor/src/ui/blocks/table.rs`
- Modify: `crates/lopress-editor/src/ui/blocks/mod.rs` (`pub mod table;`)
- Modify: `crates/lopress-editor/src/ui/blocks/editor_registry.rs` (`use` + `editor_for` arm + `table_editor_widget` fn)

**This is the most floem-heavy task. Read `list.rs` and `inline_editor.rs` first** — the table reuses `build_block_editor` + `mount_block_editor` per cell exactly as `list.rs` does per item. Verify the real signatures of `build_block_editor`, `mount_block_editor`, `CommitClosure`, `StructuralKey`, and `EditorContext` before writing; the snippet below uses them as `list.rs` does (2026-06-03). If a signature differs, adapt to the real one — the requirement is unchanged: a grid of inline-edit cells whose edits rebuild a `BlockBody::Table` and commit via `EditBlockBody`, plus an in-flow control strip dispatching the Task-7 actions.

- [ ] **Step 1: Create `crates/lopress-editor/src/ui/blocks/table.rs`:**

```rust
//! Editable GFM table widget (the `editor = "table"` implementation).
//!
//! A `v_stack` of: a contextual control strip (Add/Del Row, Add/Del Column,
//! L/C/R alignment) shown when a cell in this table is focused, then a grid of
//! rows. Each cell is a native `BlockEditorState` mounted via
//! `mount_block_editor` (the same machinery as `list.rs`). A shared
//! `CellHandles` collects every cell's editor signals so any edit rebuilds a
//! fresh `BlockBody::Table` and emits one `EditBlockBody`. Structural changes
//! (rows/columns/alignment) come from the control strip, which dispatches the
//! table `BlockAction`s.

use crate::actions::BlockAction;
use crate::model::style_span::StyleSpan;
use crate::model::sync::{canonicalize_body, rope_and_spans_to_runs};
use crate::model::types::{Align, BlockBody, BlockId, TableCell, TableData, TableRow};
use crate::ui::blocks::editor_registry::EditorContext;
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
};
use crate::ui::blocks::paragraph::BODY_FONT_SIZE;
use floem::peniko::Color;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::Editor;
use floem::views::{button, h_stack, h_stack_from_iter, label, v_stack, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::cell::RefCell;
use std::rc::Rc;

const HEADER_BG: Color = Color::rgb8(238, 238, 244);
const CELL_BORDER: Color = Color::rgb8(214, 214, 222);
const STRIP_BG: Color = Color::rgb8(250, 250, 252);

/// (row, col, editor_sig, spans_sig) for every cell, in row-major order.
type CellHandles = Rc<RefCell<Vec<(usize, usize, RwSignal<Editor>, RwSignal<Vec<StyleSpan>>)>>>;

/// The currently-focused cell within this table, as (row, col). Drives which
/// row/column the control strip operates on.
type FocusedCell = RwSignal<Option<(usize, usize)>>;

/// Rebuild a `TableData` from the live cell buffers, preserving the original
/// `align` and the row/cell ids captured at build time.
fn collect_table(handles: &CellHandles, align: &[Align], row_ids: &[BlockId], cell_ids: &[Vec<BlockId>]) -> TableData {
    let n_rows = row_ids.len();
    let mut rows: Vec<TableRow> = row_ids
        .iter()
        .enumerate()
        .map(|(r, &rid)| TableRow {
            id: rid,
            cells: cell_ids
                .get(r)
                .map(|ids| {
                    ids.iter()
                        .map(|&cid| TableCell { id: cid, runs: vec![] })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    for (r, c, editor_sig, spans_sig) in handles.borrow().iter() {
        if *r >= n_rows {
            continue;
        }
        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
        let spans = spans_sig.get_untracked();
        let rope = Rope::from(text.as_str());
        let runs = rope_and_spans_to_runs(&rope, &spans);
        if let Some(row) = rows.get_mut(*r) {
            if let Some(cell) = row.cells.get_mut(*c) {
                cell.runs = runs;
            }
        }
    }
    TableData {
        align: align.to_vec(),
        rows,
    }
}

/// Build the editable table view.
#[allow(clippy::too_many_arguments)]
pub fn table_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::Table(data) = &ctx.block.body else {
        #[cfg(debug_assertions)]
        eprintln!("[fallback] table widget: {:?} has non-table body", ctx.block.id);
        return crate::ui::blocks::fallback::fallback_block_view(ctx.block, ctx.focus_pub).into_any();
    };
    let block_id = ctx.block.id;
    let on_action = ctx.on_action.clone();
    let focus_target = ctx.focus_target;
    let focus_pub = ctx.focus_pub;
    let current_doc = ctx.current_doc;
    let on_undo = Rc::clone(&ctx.on_undo);
    let on_redo = Rc::clone(&ctx.on_redo);

    let align = data.align.clone();
    let row_ids: Vec<BlockId> = data.rows.iter().map(|r| r.id).collect();
    let cell_ids: Vec<Vec<BlockId>> = data.rows.iter().map(|r| r.cells.iter().map(|c| c.id).collect()).collect();
    let handles: CellHandles = Rc::new(RefCell::new(Vec::new()));
    let focused_cell: FocusedCell = RwSignal::new(None);

    // Shared bits for the commit closure (rebuild whole table body on any cell edit).
    let collect_ctx = (align.clone(), row_ids.clone(), cell_ids.clone());

    let mut row_views: Vec<AnyView> = Vec::with_capacity(data.rows.len());
    for (r, row) in data.rows.iter().enumerate() {
        let mut cell_views: Vec<AnyView> = Vec::with_capacity(row.cells.len());
        for (c, cell) in row.cells.iter().enumerate() {
            let cx = Scope::current();
            let state = build_block_editor(cx, &cell.runs, BODY_FONT_SIZE as usize);
            let editor_sig = state.editor_sig;
            let spans_sig = state.spans_sig;
            handles.borrow_mut().push((r, c, editor_sig, spans_sig));

            // Track focus → focused_cell, so the strip knows the active row/col.
            let focused_cell_for_cell = focused_cell;
            // Commit closure: rebuild the full table body from all cells.
            let commit_handles = Rc::clone(&handles);
            let commit_on_action = on_action.clone();
            let (c_align, c_row_ids, c_cell_ids) = collect_ctx.clone();
            let commit: CommitClosure = Rc::new(move || {
                let live = collect_table(&commit_handles, &c_align, &c_row_ids, &c_cell_ids);
                // `BlockBody` derives `PartialEq`; compare whole canonical bodies
                // (mirrors `list.rs::commit_live_if_changed`).
                let live_body = canonicalize_body(&BlockBody::Table(live));
                let differs = current_doc.with_untracked(|maybe| {
                    maybe
                        .as_ref()
                        .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
                        .map(|b| canonicalize_body(&b.body) != live_body)
                        .unwrap_or(false)
                });
                if differs {
                    commit_on_action(BlockAction::EditBlockBody {
                        block_id,
                        new_body: Box::new(live_body),
                        built_in: true,
                    });
                }
            });

            let view = mount_block_editor(
                state,
                cell.id,
                block_id,
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
                commit,
                Rc::new(move |_kp, _ms| {
                    // No table-specific keyboard structural ops; record the
                    // focused cell on any keypress and fall through to default.
                    focused_cell_for_cell.set(Some((r, c)));
                    None
                }),
                /* slash_eligible */ false,
            );

            let is_header = r == 0;
            cell_views.push(
                view.style(move |s| {
                    let s = s
                        .border(1.)
                        .border_color(CELL_BORDER)
                        .padding_horiz(6.)
                        .padding_vert(4.)
                        .min_width(80.)
                        .flex_grow(1.0);
                    if is_header {
                        s.background(HEADER_BG).font_weight(floem::text::Weight::SEMIBOLD)
                    } else {
                        s
                    }
                })
                .into_any(),
            );
        }
        row_views.push(h_stack_from_iter(cell_views).style(|s| s.width_full()).into_any());
    }

    let grid = v_stack_from_iter(row_views).style(|s| s.width_full());

    let strip = control_strip(block_id, on_action.clone(), focused_cell, focus_pub);

    v_stack((strip, grid)).style(|s| s.width_full().padding_vert(4.)).into_any()
}

/// The in-flow control strip: shown when a cell in this table is focused.
fn control_strip(
    block_id: BlockId,
    on_action: ActionSink,
    focused_cell: FocusedCell,
    focus_pub: FocusPublisher,
) -> AnyView {
    let mk = move |lbl: &'static str, make_action: Rc<dyn Fn((usize, usize)) -> Option<BlockAction>>| {
        let on_action = on_action.clone();
        button(label(move || lbl.to_string()))
            .action(move || {
                if let Some(rc) = focused_cell.get_untracked() {
                    if let Some(act) = make_action(rc) {
                        on_action(act);
                    }
                }
            })
            .style(|s| s.padding_horiz(6.).padding_vert(1.).font_size(12.))
            .into_any()
    };

    let add_row = mk("+ Row", Rc::new(move |(r, _c)| Some(BlockAction::TableInsertRow { block_id, at: r + 1 })));
    let del_row = mk("− Row", Rc::new(move |(r, _c)| Some(BlockAction::TableDeleteRow { block_id, row: r })));
    let add_col = mk("+ Col", Rc::new(move |(_r, c)| Some(BlockAction::TableInsertColumn { block_id, at: c + 1 })));
    let del_col = mk("− Col", Rc::new(move |(_r, c)| Some(BlockAction::TableDeleteColumn { block_id, col: c })));
    let al_l = mk("L", Rc::new(move |(_r, c)| Some(BlockAction::TableSetAlign { block_id, col: c, align: Align::Left })));
    let al_c = mk("C", Rc::new(move |(_r, c)| Some(BlockAction::TableSetAlign { block_id, col: c, align: Align::Center })));
    let al_r = mk("R", Rc::new(move |(_r, c)| Some(BlockAction::TableSetAlign { block_id, col: c, align: Align::Right })));

    // Only render the strip's buttons when this table holds focus (a cell here
    // is focused). `focus_pub.block == Some(block_id)` covers it.
    let _ = focus_pub;
    h_stack((add_row, del_row, add_col, del_col, al_l, al_c, al_r))
        .style(|s| {
            s.gap(4.)
                .padding_horiz(6.)
                .padding_vert(3.)
                .margin_bottom(4.)
                .background(STRIP_BG)
                .border(1.)
                .border_color(CELL_BORDER)
                .border_radius(4.)
        })
        .into_any()
}
```

> **Known rough edges to resolve while compiling (do NOT ship a guess):**
> 1. Verify `mount_block_editor`'s exact parameter list and the `StructuralKey` type alias against `inline_editor.rs` (the `Rc::new(move |_kp, _ms| ...)` closure must match `StructuralKey`'s signature — in `list.rs` it is `Rc<dyn Fn(&KeyPress, Modifiers) -> Option<CommandExecuted>>`; import those types and match exactly, returning `None` to fall through). The arg order in the `mount_block_editor(...)` call above mirrors `list.rs::list_item_editor` (2026-06-03) — diff against the real signature and reorder if it has drifted.
> 2. If `BODY_FONT_SIZE as usize` triggers a clippy cast lint in this file, mirror `list.rs` which allows it at the function with a justification, or use the same construction `list.rs` uses.
> 3. `focused_cell` is set from the structural-key closure on any keypress; also set it on cell PointerDown for click-to-focus (add an `.on_event(EventListener::PointerDown, move |_| { focused_cell.set(Some((r, c))); EventPropagation::Continue })` on the cell `view`, importing `floem::event::{EventListener, EventPropagation}`).
> 4. **Focus-gate the control strip** to match the spec ("shown when a cell in this table is focused"). Wrap `control_strip`'s `h_stack(...)` in a `dyn_container(move || focus_pub.block.get() == Some(block_id), move |shown| if shown { <buttons> } else { empty().into_any() })`, mirroring how `mod.rs::wrap_block` gates its `toolbar_slot`. The button-construction (`mk`/`add_row`/…) must move inside the builder closure so it rebuilds on focus change; capture `block_id`, `on_action`, and `focused_cell` (all cheap to clone/Copy). The always-visible version shown above is the starting point — gate it before marking the task done.

- [ ] **Step 2: Register the module** in `mod.rs`:

```rust
pub mod table;
```

- [ ] **Step 3: Register the widget** in `editor_registry.rs`. Add to the `use` line and add a `table_editor_widget` adapter + the match arm:

```rust
        "image" => Some(image::image_widget),
        "table" => Some(table::table_editor_widget),
```

Add `table` to `use crate::ui::blocks::{code_editor, image, list, read_more, separator, table};`. (The widget signature already matches `EditorWidget = fn(&EditorContext) -> AnyView`, so no adapter wrapper is needed — register `table::table_editor_widget` directly.)

- [ ] **Step 4: Add a registry test:**

```rust
    #[test]
    fn editor_for_resolves_table() {
        assert!(editor_for("table").is_some());
    }
```

- [ ] **Step 5: Compile and run**

Run: `cargo test -p lopress-editor editor_for_resolves_table`
Expected: PASS (compiles; widget registered). Resolve the rough edges from Step 1's note until it compiles cleanly under `-D warnings`.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/table.rs crates/lopress-editor/src/ui/blocks/mod.rs crates/lopress-editor/src/ui/blocks/editor_registry.rs
git commit -m "feat(editor): editable table widget with control strip"
```

---

## Task 10: Slash menu — Separator and Table entries

**Files:**
- Modify: `crates/lopress-editor/src/ui/slash_menu.rs` (`SlashChoice` enum, `slash_menu_items()`)
- Modify: `crates/lopress-editor/src/ui/editor_pane.rs` (the `on_select` match, ~line 120)
- Modify: `crates/lopress-editor/tests/slash_menu_tests.rs` (the index assertions shift)

- [ ] **Step 1: Write/adjust the failing test** — in `crates/lopress-editor/tests/slash_menu_tests.rs`, add membership assertions (and fix any positional index assertions that shift because two items are appended after "Read more"). Prefer membership over fixed indices:

```rust
    #[test]
    fn includes_separator_and_table() {
        let items = slash_menu_items();
        assert!(items.iter().any(|(l, c)| l == "Separator" && matches!(c, SlashChoice::Separator)));
        assert!(items.iter().any(|(l, c)| l == "Table" && matches!(c, SlashChoice::Table)));
    }
```

(If the existing `items[7]`/`items[8]` index assertions still hold — Separator/Table are added *after* Read more — they need no change. Confirm by reading the test; adjust only if indices shifted.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-editor includes_separator_and_table`
Expected: FAIL to compile (`SlashChoice::Separator`/`Table` don't exist).

- [ ] **Step 3: Add the `SlashChoice` variants:**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
    Separator,
    Table,
    Plugin { type_name: Rc<str> },
}
```

- [ ] **Step 4: Add the menu items** — in `slash_menu_items()`, after the `("Read more", SlashChoice::ReadMore)` entry:

```rust
        ("Separator".to_string(), SlashChoice::Separator),
        ("Table".to_string(), SlashChoice::Table),
```

- [ ] **Step 5: Handle the new choices** in `editor_pane.rs`'s `on_select` match (next to the `SlashChoice::Image` arm):

```rust
                    SlashChoice::Separator => {
                        on_action_for_select(BlockAction::InsertAfter {
                            anchor: block_id,
                            new_block: Box::new(EditorBlock::separator()),
                        });
                    }
                    SlashChoice::Table => {
                        on_action_for_select(BlockAction::InsertAfter {
                            anchor: block_id,
                            new_block: Box::new(EditorBlock::table_default()),
                        });
                    }
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p lopress-editor includes_separator_and_table`
Expected: PASS. Run `cargo test -p lopress-editor` to confirm the other slash-menu tests still pass (fix index assertions if they shifted).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-editor/src/ui/slash_menu.rs crates/lopress-editor/src/ui/editor_pane.rs crates/lopress-editor/tests/slash_menu_tests.rs
git commit -m "feat(editor): slash-menu entries for separator and table"
```

---

## Task 11: Toolbar — Table insert button

**Files:**
- Modify: `crates/lopress-editor/src/ui/toolbar.rs` (`block_toolbar_for` — add the button using `InsertAfter` semantics)
- Test: add to `toolbar.rs` `mod tests`

**Context:** The kind-cycler buttons fire `ChangeType`. The Table button must instead `InsertAfter` a fresh `table_default()` — a table is not a conversion target.

- [ ] **Step 1: Write the failing test** — a small unit asserting the action constructed is an `InsertAfter` of a table. Since the button wiring is UI, test the action-construction helper instead. Add a tiny pure helper and test it:

In `toolbar.rs`, add near the bottom (above `#[cfg(test)]`):

```rust
/// The action the toolbar's Table button dispatches: insert a fresh default
/// table immediately after `block_id`. Extracted so it can be unit-tested
/// without driving the UI.
fn table_insert_action(block_id: BlockId) -> BlockAction {
    BlockAction::InsertAfter {
        anchor: block_id,
        new_block: Box::new(crate::model::types::EditorBlock::table_default()),
    }
}
```

Test:

```rust
    #[test]
    fn table_button_inserts_a_table_after_block() {
        let id = BlockId::new();
        let action = table_insert_action(id);
        match action {
            BlockAction::InsertAfter { anchor, new_block } => {
                assert_eq!(anchor, id);
                assert!(matches!(new_block.kind, BlockKind::Table));
            }
            _ => panic!("expected InsertAfter"),
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lopress-editor table_button_inserts_a_table_after_block`
Expected: FAIL to compile (`table_insert_action` not yet present, or `BlockKind`/`BlockAction` imports missing in `toolbar.rs` tests — add `use crate::model::types::BlockKind;` to the test module if needed; `BlockAction` is already imported at file top).

- [ ] **Step 3: Add the Table button** in `block_toolbar_for`. After the inline-flag toggle buttons and their trailing `separator()` (just before the Delete button is pushed), insert:

```rust
    // Table insert button — distinct from the kind-cycler: it inserts a fresh
    // table after the focused block rather than converting the block.
    let on_action_for_table = on_action.clone();
    let table_btn = button(label(|| "Table".to_string()))
        .on_event_stop(EventListener::PointerDown, move |_| {
            on_action_for_table(table_insert_action(block_id));
        })
        .style(|s| s.padding_horiz(6.).padding_vert(2.));
    buttons.push(table_btn.into_any());
    buttons.push(separator().into_any());
```

(Place this block immediately before the existing `// Delete.` section so the order is: kind buttons | sep | B/I/code/link | sep | **Table | sep** | Delete.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p lopress-editor table_button_inserts_a_table_after_block`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-editor/src/ui/toolbar.rs
git commit -m "feat(editor): toolbar button to insert a table"
```

---

## Task 12: Full gate + live-GUI end-to-end verification

**Files:** none (verification only; any fixes get folded into the relevant task's files and committed).

- [ ] **Step 1: Run the full workspace gate.** Force a clippy re-lint first to defeat the cache false-pass (touch one source file per changed crate, e.g. `touch crates/lopress-core/src/lib.rs crates/lopress-build/src/lib.rs crates/lopress-plugin/src/lib.rs crates/lopress-editor/src/lib.rs`), then:

Run: `bash scripts/check.sh`
Expected: PASS — `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all green. Fix any failures in their owning task's files and amend/commit as a follow-up `fix:` commit (named files only).

- [ ] **Step 2: Live-GUI e2e via the control server** (use the `driving-lopress-editor` capability; you have the `bg_run` background-command tool + helpers — launch the editor with `bg_run` so `cargo run` does not block your turn). Standing rules: visible non-minimized window; run from repo root with plain `cargo run` (debug); poll `/ping` until ok before `/open`; first `/open` is an ABSOLUTE path into a workspace dir with `lopress.toml`; use a throwaway workspace under `$env:TEMP` (never commit a `lopress.toml`).

  Procedure:
  1. Scaffold a throwaway workspace: `lopress new $env:TEMP\lopress-tbl-e2e` (or the project's scaffold command — see the `driving-lopress-editor` skill / `reference_e2e_workspace_needs_scaffold` for the exact bootstrap).
  2. Launch the editor with `bg_run` (`cargo run` from repo root), poll `http://127.0.0.1:7878/ping` until ok.
  3. `/open` an absolute path to a post `.md` in that workspace.
  4. Insert a **separator**: focus an empty paragraph, send `/` then type `separ`, Enter (or drive via the documented `/action` for slash insert). Re-read `/state` and `/screenshot`.
  5. Insert a **table** from the slash menu (`/` → `table` → Enter), and a second table via the **toolbar** Table button. Type into a couple of cells; use the control strip to add a row and set a column alignment.
  6. Save (the editor's save path), then read the persisted `.md` from disk and confirm it contains `---` for the separator and a GFM table (`| … |` header + `| :--- | …` delimiter row) reflecting the alignment you set.
  - No step is PASS without verbatim command + output. `dispatched`/`200` ≠ effect happened — re-read `/state` and the saved file to confirm. If a check genuinely needs a physical mouse/eye, hand it back with a concrete checklist; otherwise drive it yourself.

- [ ] **Step 3: Final gate-pass commit (only if Step 1/2 required fixes).** Stage NAMED files only:

```bash
git add <only the files you changed>
git commit -m "test: verify table and separator end-to-end"
```

---

## Done when

All 12 tasks are committed (one commit each, named-file staging), `bash scripts/check.sh` passes clean, and the live-GUI e2e (Task 12 Step 2) is recorded with verbatim command+output showing a separator (`---`) and a GFM table persisted to a throwaway workspace `.md` — or any check that genuinely needs a human is handed back with a concrete checklist.

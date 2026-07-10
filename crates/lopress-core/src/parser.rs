use crate::delimiter;
use crate::error::ParseError;
use crate::frontmatter;
use crate::types::{Block, Document};
use pulldown_cmark::{
    html, Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use serde_json::{json, Value};

/// Parse a markdown source (with optional front-matter) into a Document.
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let (front_matter, body) = frontmatter::split(src)?;
    let blocks = parse_body(body)?;
    Ok(Document {
        front_matter,
        blocks,
    })
}

/// Render a markdown string to HTML using pulldown-cmark.
pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Render an inline markdown fragment to HTML *without* the enclosing block
/// `<p>` wrapper. Block body text (paragraph/heading) is stored as inline
/// markdown source; the caller's own HTML element (`<p>`, `<h2>`, …) supplies
/// the wrapper, so the paragraph tags pulldown would emit are filtered out.
pub fn render_inline_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES).filter(|ev| {
        !matches!(
            ev,
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph)
        )
    });
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

fn parse_body(body: &str) -> Result<Vec<Block>, ParseError> {
    let delims = delimiter::scan(body)?;
    if delims.is_empty() {
        return parse_plain_markdown(body);
    }
    let mut idx = 0usize;
    let (blocks, _) = build_tree(body, &delims, &mut idx, 0, body.len(), None)?;
    Ok(blocks)
}

/// Build blocks for the slice `body[start..end]`, consuming delimiters from
/// `delims[*idx..]`. If `expected_close` is Some, the function returns when
/// it encounters that closing delimiter (consuming it).
fn build_tree(
    body: &str,
    delims: &[delimiter::Delim],
    idx: &mut usize,
    start: usize,
    end: usize,
    expected_close: Option<&str>,
) -> Result<(Vec<Block>, bool), ParseError> {
    let mut out = Vec::new();
    let mut cursor = start;

    while let Some(current) = delims.get(*idx).cloned() {
        let next_span = match &current {
            delimiter::Delim::Open { span, .. } | delimiter::Delim::Close { span, .. } => *span,
        };
        if next_span.0 >= end {
            break;
        }
        // Flush plain markdown before this delimiter.
        if cursor < next_span.0 {
            let slice = body
                .get(cursor..next_span.0)
                .ok_or_else(|| ParseError::BlockAttrs {
                    line: line_of(body, cursor),
                    message: "invalid body span while flushing markdown".into(),
                })?;
            out.extend(parse_plain_markdown(slice)?);
        }
        match current {
            delimiter::Delim::Open {
                name,
                attrs_json,
                span,
            } => {
                *idx += 1;
                let (children, _) = build_tree(body, delims, idx, span.1, end, Some(&name))?;
                let attrs = if attrs_json.is_empty() {
                    Value::Object(Default::default())
                } else {
                    serde_json::from_str(&attrs_json).map_err(|e| ParseError::BlockAttrs {
                        line: line_of(body, span.0),
                        message: e.to_string(),
                    })?
                };
                out.push(Block {
                    r#type: format!("lopress:{name}"),
                    attrs,
                    children,
                    text: None,
                });
                // Cursor sits just past the matching close consumed in the inner call.
                // Peek at the just-consumed delimiter to learn where it ended.
                cursor = idx
                    .checked_sub(1)
                    .and_then(|i| delims.get(i))
                    .and_then(|d| match d {
                        delimiter::Delim::Close { span: cspan, .. } => Some(cspan.1),
                        _ => None,
                    })
                    .unwrap_or(span.1);
            }
            delimiter::Delim::Close { name, span } => match expected_close {
                Some(exp) if exp == name => {
                    *idx += 1;
                    return Ok((out, true));
                }
                Some(exp) => {
                    return Err(ParseError::MismatchedClose {
                        expected: exp.to_string(),
                        actual: name,
                        line: line_of(body, span.0),
                    });
                }
                None => {
                    return Err(ParseError::MismatchedClose {
                        expected: "<none>".into(),
                        actual: name,
                        line: line_of(body, span.0),
                    });
                }
            },
        }
    }

    if let Some(exp) = expected_close {
        return Err(ParseError::UnterminatedBlock {
            block_type: exp.to_string(),
            line: 0,
        });
    }

    // Flush trailing plain markdown.
    if cursor < end {
        let slice = body
            .get(cursor..end)
            .ok_or_else(|| ParseError::BlockAttrs {
                line: line_of(body, cursor),
                message: "invalid trailing body span".into(),
            })?;
        out.extend(parse_plain_markdown(slice)?);
    }

    Ok((out, false))
}

fn line_of(src: &str, byte_offset: usize) -> usize {
    let cap = byte_offset.min(src.len());
    src.get(..cap)
        .unwrap_or("")
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

fn parse_plain_markdown(body: &str) -> Result<Vec<Block>, ParseError> {
    let mut parser = Parser::new_ext(body, Options::ENABLE_TABLES);
    parse_blocks(&mut parser, None)
}

fn parse_blocks(parser: &mut Parser<'_>, stop: Option<TagEnd>) -> Result<Vec<Block>, ParseError> {
    let mut blocks = Vec::new();
    while let Some(event) = parser.next() {
        if let Event::End(ref end) = event {
            if Some(end) == stop.as_ref() {
                return Ok(blocks);
            }
        }
        if let Some(block) = parse_one(event, parser)? {
            blocks.push(block);
        }
    }
    Ok(blocks)
}

fn parse_one(event: Event<'_>, parser: &mut Parser<'_>) -> Result<Option<Block>, ParseError> {
    Ok(Some(match event {
        Event::Start(Tag::Paragraph) => {
            let (text, image) = consume_inline(parser, TagEnd::Paragraph);
            if let Some(img) = image {
                img
            } else {
                Block {
                    r#type: "paragraph".into(),
                    attrs: json!({}),
                    children: vec![],
                    text: Some(text),
                }
            }
        }
        Event::Start(Tag::Heading { level, .. }) => {
            let lvl = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            };
            let (text, _) = consume_inline(parser, TagEnd::Heading(level));
            Block {
                r#type: "heading".into(),
                attrs: json!({ "level": lvl }),
                children: vec![],
                text: Some(text),
            }
        }
        Event::Start(Tag::BlockQuote) => {
            let children = parse_blocks(parser, Some(TagEnd::BlockQuote))?;
            Block {
                r#type: "quote".into(),
                attrs: json!({}),
                children,
                text: None,
            }
        }
        Event::Start(Tag::CodeBlock(kind)) => {
            let lang = match kind {
                CodeBlockKind::Fenced(l) => l.to_string(),
                CodeBlockKind::Indented => String::new(),
            };
            let mut body = String::new();
            for ev in parser.by_ref() {
                match ev {
                    Event::Text(t) => body.push_str(&t),
                    Event::End(TagEnd::CodeBlock) => break,
                    _ => {}
                }
            }
            Block {
                r#type: "code".into(),
                attrs: if lang.is_empty() {
                    json!({})
                } else {
                    json!({ "lang": lang })
                },
                children: vec![],
                text: Some(body),
            }
        }
        Event::Start(Tag::Table(alignments)) => parse_table(alignments, parser)?,
        Event::Start(Tag::List(first)) => {
            let ordered = first.is_some();
            let children = parse_blocks(parser, Some(TagEnd::List(ordered)))?;
            Block {
                r#type: "list".into(),
                attrs: json!({ "ordered": ordered }),
                children,
                text: None,
            }
        }
        Event::Start(Tag::Item) => parse_item(parser)?,
        Event::Rule => Block {
            r#type: "separator".into(),
            attrs: json!({}),
            children: vec![],
            text: None,
        },
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
    }))
}

/// Parse the children of a list item.
///
/// Loose-list items wrap their content in `Paragraph` events (handled by
/// `parse_one`); tight-list items emit bare inline events with no wrapper.
/// Bare inline content is accumulated and synthesised into a `paragraph`
/// child so every `list_item` has the uniform `list_item > paragraph` shape
/// regardless of list tightness. An item may also hold block children (e.g.
/// a nested list) — pending inline text is flushed as a paragraph before
/// each. An item with no content at all still gets one empty paragraph.
fn parse_item(parser: &mut Parser<'_>) -> Result<Block, ParseError> {
    let mut children: Vec<Block> = Vec::new();
    // Pending bare-inline text of a tight-list item. The inline conversions
    // here mirror `consume_inline` (emphasis → `*`, strong → `**`, code →
    // backticks); keep the two in step if either changes.
    let mut inline = String::new();

    while let Some(ev) = parser.next() {
        match ev {
            Event::End(TagEnd::Item) => break,
            Event::Text(t) => inline.push_str(&t),
            Event::Code(t) => {
                inline.push('`');
                inline.push_str(&t);
                inline.push('`');
            }
            Event::SoftBreak | Event::HardBreak => inline.push('\n'),
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => inline.push('*'),
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => inline.push_str("**"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                push_link(&mut inline, &dest_url, parser);
            }
            Event::Start(Tag::Image { .. }) => {
                // Consistent with `consume_inline`: an image mixed with text
                // is dropped (only a standalone image becomes its own block,
                // which a list item is not).
                for inner in parser.by_ref() {
                    if matches!(inner, Event::End(TagEnd::Image)) {
                        break;
                    }
                }
            }
            // A block-level child (loose-list paragraph, nested list, …).
            // Flush any pending tight-list inline text first.
            block_start @ Event::Start(_) => {
                flush_item_paragraph(&mut children, &mut inline);
                if let Some(block) = parse_one(block_start, parser)? {
                    children.push(block);
                }
            }
            // Stray End events, task markers, raw HTML — ignore.
            _ => {}
        }
    }
    flush_item_paragraph(&mut children, &mut inline);
    if children.is_empty() {
        children.push(Block {
            r#type: "paragraph".into(),
            attrs: json!({}),
            children: vec![],
            text: Some(String::new()),
        });
    }

    Ok(Block {
        r#type: "list_item".into(),
        attrs: json!({}),
        children,
        text: None,
    })
}

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
            Event::Start(Tag::Link { dest_url, .. }) => {
                push_link(&mut text, &dest_url, parser);
            }
            Event::End(TagEnd::TableCell) => break,
            _ => {}
        }
    }
    text
}

/// Drain `inline` into a synthesised `paragraph` child when it holds
/// non-whitespace text; otherwise discard it.
fn flush_item_paragraph(children: &mut Vec<Block>, inline: &mut String) {
    if inline.trim().is_empty() {
        inline.clear();
    } else {
        children.push(Block {
            r#type: "paragraph".into(),
            attrs: json!({}),
            children: vec![],
            text: Some(std::mem::take(inline)),
        });
    }
}

/// Consume a link's inner events up to `TagEnd::Link`, appending it to `out`
/// as markdown `[text](dest)`. Only the link's visible text is captured; any
/// nested inline styling inside the link text is flattened to plain text
/// (consistent with how the surrounding inline conversions treat link text).
fn push_link(out: &mut String, dest_url: &str, parser: &mut Parser<'_>) {
    let mut inner = String::new();
    for ev in parser.by_ref() {
        match ev {
            Event::Text(t) => inner.push_str(&t),
            Event::End(TagEnd::Link) => break,
            _ => {}
        }
    }
    out.push('[');
    out.push_str(&inner);
    out.push_str("](");
    out.push_str(dest_url);
    out.push(')');
}

fn consume_inline(parser: &mut Parser<'_>, end: TagEnd) -> (String, Option<Block>) {
    let mut text = String::new();
    let mut only_image: Option<Block> = None;
    let mut other_text = false;
    while let Some(ev) = parser.next() {
        match ev {
            Event::Text(t) => {
                other_text = other_text || !t.trim().is_empty();
                text.push_str(&t);
            }
            Event::Code(t) => {
                other_text = true;
                text.push('`');
                text.push_str(&t);
                text.push('`');
            }
            Event::SoftBreak => text.push('\n'),
            Event::HardBreak => text.push('\n'),
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let src = dest_url.to_string();
                let caption = title.to_string();
                let mut alt = String::new();
                for inner in parser.by_ref() {
                    match inner {
                        Event::Text(t) => alt.push_str(&t),
                        Event::End(TagEnd::Image) => break,
                        _ => {}
                    }
                }
                let mut attrs = serde_json::Map::new();
                attrs.insert("src".into(), serde_json::Value::String(src));
                attrs.insert("alt".into(), serde_json::Value::String(alt));
                if !caption.is_empty() {
                    attrs.insert("caption".into(), serde_json::Value::String(caption));
                }
                only_image = Some(Block {
                    r#type: "image".into(),
                    attrs: Value::Object(attrs),
                    children: vec![],
                    text: None,
                });
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                other_text = true;
                push_link(&mut text, &dest_url, parser);
            }
            Event::Start(Tag::Emphasis) => text.push('*'),
            Event::End(TagEnd::Emphasis) => text.push('*'),
            Event::Start(Tag::Strong) => text.push_str("**"),
            Event::End(TagEnd::Strong) => text.push_str("**"),
            Event::End(ref e) if *e == end => break,
            _ => {}
        }
    }
    if !other_text && only_image.is_some() {
        (text, only_image)
    } else {
        (text, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|b| b.r#type.as_str()).collect()
    }

    #[test]
    fn parses_front_matter_and_single_paragraph() {
        let d = parse("---\ntitle: X\n---\nhello\n").unwrap();
        assert_eq!(d.front_matter.title.as_deref(), Some("X"));
        assert_eq!(types(&d.blocks), vec!["paragraph"]);
        assert_eq!(d.blocks[0].text.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_heading_level() {
        let d = parse("## H2 heading\n").unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].r#type, "heading");
        assert_eq!(d.blocks[0].attrs, json!({"level": 2}));
        assert_eq!(d.blocks[0].text.as_deref(), Some("H2 heading"));
    }

    #[test]
    fn paragraph_link_preserves_url() {
        let d = parse("[take a look!](https://example.com)\n").unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].r#type, "paragraph");
        assert_eq!(
            d.blocks[0].text.as_deref(),
            Some("[take a look!](https://example.com)")
        );
    }

    #[test]
    fn list_item_link_preserves_url() {
        let d = parse("- see [docs](https://example.com)\n").unwrap();
        let list = &d.blocks[0];
        assert_eq!(list.r#type, "list");
        let para = &list.children[0].children[0];
        assert_eq!(
            para.text.as_deref(),
            Some("see [docs](https://example.com)")
        );
    }

    #[test]
    fn parses_unordered_list() {
        let d = parse("- one\n- two\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["list"]);
        assert_eq!(d.blocks[0].attrs, json!({"ordered": false}));
        assert_eq!(d.blocks[0].children.len(), 2);
        assert_eq!(d.blocks[0].children[0].r#type, "list_item");
    }

    #[test]
    fn tight_list_items_carry_their_text() {
        // Tight lists (no blank line between items) emit bare inline events
        // with no Paragraph wrapper — the item text must still be captured.
        let d = parse("- one\n- two\n").unwrap();
        let list = &d.blocks[0];
        assert_eq!(list.children.len(), 2);
        for (item, expected) in list.children.iter().zip(["one", "two"]) {
            assert_eq!(item.r#type, "list_item");
            assert_eq!(
                item.children.len(),
                1,
                "item should have one paragraph child"
            );
            assert_eq!(item.children[0].r#type, "paragraph");
            assert_eq!(item.children[0].text.as_deref(), Some(expected));
        }
    }

    #[test]
    fn loose_list_items_carry_their_text() {
        let d = parse("- one\n\n- two\n").unwrap();
        let list = &d.blocks[0];
        assert_eq!(list.children.len(), 2);
        assert_eq!(list.children[0].children[0].text.as_deref(), Some("one"));
        assert_eq!(list.children[1].children[0].text.as_deref(), Some("two"));
    }

    #[test]
    fn ordered_tight_list_items_carry_their_text() {
        let d = parse("1. first\n2. second\n").unwrap();
        let list = &d.blocks[0];
        assert_eq!(list.attrs, json!({"ordered": true}));
        assert_eq!(list.children[0].children[0].text.as_deref(), Some("first"));
        assert_eq!(list.children[1].children[0].text.as_deref(), Some("second"));
    }

    #[test]
    fn tight_list_item_with_nested_list_keeps_both() {
        let d = parse("- one\n  - sub\n").unwrap();
        let item0 = &d.blocks[0].children[0];
        assert_eq!(item0.children.len(), 2);
        assert_eq!(item0.children[0].r#type, "paragraph");
        assert_eq!(item0.children[0].text.as_deref(), Some("one"));
        assert_eq!(item0.children[1].r#type, "list");
    }

    #[test]
    fn parses_fenced_code_with_language() {
        let d = parse("```rust\nfn main() {}\n```\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["code"]);
        assert_eq!(d.blocks[0].attrs, json!({"lang": "rust"}));
        assert_eq!(d.blocks[0].text.as_deref(), Some("fn main() {}\n"));
    }

    #[test]
    fn parses_blockquote_containing_paragraph() {
        let d = parse("> quoted\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["quote"]);
        assert_eq!(d.blocks[0].children.len(), 1);
        assert_eq!(d.blocks[0].children[0].r#type, "paragraph");
    }

    #[test]
    fn parses_image_block_from_standalone_markdown_image() {
        let d = parse("![alt](foo.jpg)\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["image"]);
        assert_eq!(d.blocks[0].attrs, json!({"src": "foo.jpg", "alt": "alt"}));
    }

    #[test]
    fn parses_self_closing_custom_block() {
        let src = r#"before

<!-- lopress:video {"src":"a.mp4"} -->
<!-- /lopress:video -->

after
"#;
        let d = parse(src).unwrap();
        let names: Vec<&str> = d.blocks.iter().map(|b| b.r#type.as_str()).collect();
        assert_eq!(names, vec!["paragraph", "lopress:video", "paragraph"]);
        assert_eq!(d.blocks[1].attrs, json!({"src":"a.mp4"}));
        assert!(d.blocks[1].children.is_empty());
    }

    #[test]
    fn parses_custom_block_with_inner_markdown() {
        let src = "<!-- lopress:callout {\"kind\":\"warning\"} -->\nbody para\n<!-- /lopress:callout -->\n";
        let d = parse(src).unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].r#type, "lopress:callout");
        assert_eq!(d.blocks[0].attrs, json!({"kind": "warning"}));
        assert_eq!(d.blocks[0].children.len(), 1);
        assert_eq!(d.blocks[0].children[0].r#type, "paragraph");
    }

    #[test]
    fn parses_nested_columns() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let d = parse(src).unwrap();
        assert_eq!(d.blocks.len(), 1);
        let cols = &d.blocks[0];
        assert_eq!(cols.r#type, "lopress:columns");
        assert_eq!(cols.children.len(), 2);
        for col in &cols.children {
            assert_eq!(col.r#type, "lopress:column");
            assert_eq!(col.children.len(), 1);
        }
    }

    #[test]
    fn parses_image_caption_from_title() {
        let d = parse("![alt](foo.jpg \"My caption\")\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["image"]);
        assert_eq!(
            d.blocks[0].attrs,
            json!({ "src": "foo.jpg", "alt": "alt", "caption": "My caption" })
        );
    }

    #[test]
    fn parses_image_without_title_has_no_caption() {
        let d = parse("![alt](foo.jpg)\n").unwrap();
        assert_eq!(d.blocks[0].attrs, json!({ "src": "foo.jpg", "alt": "alt" }));
    }

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

    #[test]
    fn parses_thematic_break_as_separator() {
        let d = parse("before\n\n---\n\nafter\n").unwrap();
        assert_eq!(
            types(&d.blocks),
            vec!["paragraph", "separator", "paragraph"]
        );
        let sep = &d.blocks[1];
        assert!(sep.children.is_empty());
        assert!(sep.text.is_none());
        assert_eq!(sep.attrs, json!({}));
    }

    #[test]
    fn mismatched_close_is_error() {
        let src = "<!-- lopress:a -->\n<!-- /lopress:b -->\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn unterminated_open_is_error() {
        let src = "<!-- lopress:a -->\nhi\n";
        assert!(parse(src).is_err());
    }
}

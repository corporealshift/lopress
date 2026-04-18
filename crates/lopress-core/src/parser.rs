use crate::delimiter;
use crate::error::ParseError;
use crate::frontmatter;
use crate::types::{Block, Document};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::json;

/// Parse a markdown source (with optional front-matter) into a Document.
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let (front_matter, body) = frontmatter::split(src)?;
    let delims = delimiter::scan(body)?;
    let _ = delims;

    let mut parser = Parser::new(body);
    let blocks = parse_blocks(&mut parser, None)?;
    Ok(Document {
        front_matter,
        blocks,
    })
}

fn parse_blocks(
    parser: &mut Parser<'_>,
    stop: Option<TagEnd>,
) -> Result<Vec<Block>, ParseError> {
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

fn parse_one(
    event: Event<'_>,
    parser: &mut Parser<'_>,
) -> Result<Option<Block>, ParseError> {
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
            while let Some(ev) = parser.next() {
                match ev {
                    Event::Text(t) => body.push_str(&t),
                    Event::End(TagEnd::CodeBlock) => break,
                    _ => {}
                }
            }
            Block {
                r#type: "code_block".into(),
                attrs: if lang.is_empty() {
                    json!({})
                } else {
                    json!({ "lang": lang })
                },
                children: vec![],
                text: Some(body),
            }
        }
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
        Event::Start(Tag::Item) => {
            let children = parse_blocks(parser, Some(TagEnd::Item))?;
            Block {
                r#type: "list_item".into(),
                attrs: json!({}),
                children,
                text: None,
            }
        }
        Event::Html(_) | Event::InlineHtml(_) | Event::Text(_) | Event::Code(_)
        | Event::SoftBreak | Event::HardBreak | Event::Rule | Event::TaskListMarker(_)
        | Event::FootnoteReference(_) | Event::Start(_) | Event::End(_) => {
            return Ok(None);
        }
    }))
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
            Event::Start(Tag::Image { dest_url, title: _, id: _, .. }) => {
                let src = dest_url.to_string();
                let mut alt = String::new();
                while let Some(inner) = parser.next() {
                    match inner {
                        Event::Text(t) => alt.push_str(&t),
                        Event::End(TagEnd::Image) => break,
                        _ => {}
                    }
                }
                only_image = Some(Block {
                    r#type: "image".into(),
                    attrs: json!({ "src": src, "alt": alt }),
                    children: vec![],
                    text: None,
                });
            }
            Event::Start(Tag::Link { .. }) => {
                other_text = true;
                while let Some(inner) = parser.next() {
                    match inner {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::Link) => break,
                        _ => {}
                    }
                }
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
    fn parses_unordered_list() {
        let d = parse("- one\n- two\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["list"]);
        assert_eq!(d.blocks[0].attrs, json!({"ordered": false}));
        assert_eq!(d.blocks[0].children.len(), 2);
        assert_eq!(d.blocks[0].children[0].r#type, "list_item");
    }

    #[test]
    fn parses_fenced_code_block_with_language() {
        let d = parse("```rust\nfn main() {}\n```\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["code_block"]);
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
}

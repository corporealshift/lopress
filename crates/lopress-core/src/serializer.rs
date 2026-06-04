use crate::types::{Block, Document, FrontMatter};
use serde_json::Value;
use std::fmt::Write;

/// Render a Document back to markdown source.
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    if !is_default_frontmatter(&doc.front_matter) {
        // FrontMatter is a plain owned struct of Option<String>/Vec<String>/bool/
        // DateTime/Map<String,Value>; serde_yaml has no documented failure path
        // for these. On the impossible error we emit empty yaml rather than panic.
        let yaml = serde_yaml::to_string(&doc.front_matter).unwrap_or_default();
        out.push_str("---\n");
        out.push_str(&yaml);
        if !yaml.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n");
    }
    for (i, b) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_block(&mut out, b, 0);
    }
    out
}

fn is_default_frontmatter(fm: &FrontMatter) -> bool {
    fm.title.is_none()
        && fm.slug.is_none()
        && fm.date.is_none()
        && fm.tags.is_empty()
        && !fm.draft
        && fm.description.is_none()
        && fm.image.is_none()
        && fm.extra.is_empty()
}

fn write_block(out: &mut String, b: &Block, _depth: usize) {
    match b.r#type.as_str() {
        "paragraph" => {
            if let Some(t) = &b.text {
                out.push_str(t);
                if !t.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        "heading" => {
            let level_u64 = b.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1);
            let level = usize::try_from(level_u64).unwrap_or(1).max(1);
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
            if let Some(t) = &b.text {
                // A Markdown heading is a single line: only the first line
                // carries the `#` prefix. Collapse any soft line breaks to
                // spaces so a continuation does not reparse as a separate
                // paragraph (which would break round-tripping).
                out.push_str(&t.replace('\n', " "));
            }
            out.push('\n');
        }
        "quote" => {
            for c in &b.children {
                let mut inner = String::new();
                write_block(&mut inner, c, 0);
                for line in inner.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        "code" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str("```");
            out.push_str(lang);
            out.push('\n');
            if let Some(t) = &b.text {
                out.push_str(t);
                if !t.ends_with('\n') {
                    out.push('\n');
                }
            }
            out.push_str("```\n");
        }
        "list" => {
            let ordered = b
                .attrs
                .get("ordered")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            for (idx, item) in b.children.iter().enumerate() {
                let mut inner = String::new();
                for c in &item.children {
                    write_block(&mut inner, c, 0);
                }
                let marker = if ordered {
                    format!("{}. ", idx + 1)
                } else {
                    "- ".to_string()
                };
                let text = inner.trim_end_matches('\n');
                if text.is_empty() {
                    // An item with no content lines must still emit its
                    // marker; otherwise the list block vanishes on
                    // re-serialization and the round-trip is unstable.
                    out.push_str(marker.trim_end());
                    out.push('\n');
                } else {
                    let mut first = true;
                    for line in text.lines() {
                        if first {
                            out.push_str(&marker);
                            first = false;
                        } else {
                            out.push_str("  ");
                        }
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
        "separator" => {
            out.push_str("---\n");
        }
        "table" => {
            write_table(out, b);
        }
        "image" => {
            let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
            let alt = b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or("");
            let caption = b
                .attrs
                .get("caption")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if caption.is_empty() {
                let _ = writeln!(out, "![{alt}]({src})");
            } else {
                // Markdown image title is double-quoted; escape embedded quotes.
                let cap = caption.replace('"', "\\\"");
                let _ = writeln!(out, "![{alt}]({src} \"{cap}\")");
            }
        }
        custom if custom.starts_with("lopress:") => {
            let name = custom.strip_prefix("lopress:").unwrap_or(custom);
            out.push_str("<!-- lopress:");
            out.push_str(name);
            if !is_empty_attrs(&b.attrs) {
                out.push(' ');
                out.push_str(&serde_json::to_string(&b.attrs).unwrap_or_default());
            }
            out.push_str(" -->\n");
            for (i, c) in b.children.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                write_block(out, c, 0);
            }
            out.push_str("<!-- /lopress:");
            out.push_str(name);
            out.push_str(" -->\n");
        }
        _ => {
            out.push_str("<!-- unknown block: ");
            out.push_str(&b.r#type);
            out.push_str(" -->\n");
        }
    }
}

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

    let cell_text = |cell: &Block| -> String {
        cell.text.as_deref().unwrap_or("").replace('|', "\\|")
    };
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
    let Some(header) = rows.next() else {
        return;
    };
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

fn is_empty_attrs(v: &Value) -> bool {
    matches!(v, Value::Object(m) if m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn serializes_frontmatter_when_set() {
        let s = serialize(&Document {
            front_matter: FrontMatter {
                title: Some("Hi".into()),
                draft: true,
                ..Default::default()
            },
            blocks: vec![Block::paragraph("hello")],
        });
        assert!(s.starts_with("---\n"));
        assert!(s.contains("title: Hi\n"));
        assert!(s.contains("draft: true\n"));
        assert!(s.ends_with("hello\n"));
    }

    #[test]
    fn omits_frontmatter_when_default() {
        let s = serialize(&Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::paragraph("hi")],
        });
        assert!(!s.starts_with("---"));
    }

    #[test]
    fn serializes_heading_at_right_level() {
        let s = serialize(&Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::heading(3, "title")],
        });
        assert_eq!(s, "### title\n");
    }

    #[test]
    fn serializes_custom_block_with_attrs() {
        use serde_json::json;
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:video".into(),
                attrs: json!({"src":"a.mp4"}),
                children: vec![],
                text: None,
            }],
        };
        let s = serialize(&doc);
        assert!(s.contains(r#"<!-- lopress:video {"src":"a.mp4"} -->"#));
        assert!(s.contains("<!-- /lopress:video -->"));
    }

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

    #[test]
    fn roundtrip_simple_doc() {
        let src = "---\ntitle: t\n---\nhello\n\n## section\n";
        let d = parse(src).unwrap();
        let s = serialize(&d);
        let d2 = parse(&s).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn heading_with_soft_newline_stays_a_single_heading() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::heading(2, "line one\nline two".to_string())],
        };
        let s = serialize(&doc);
        // The continuation must not be emitted as a bare (prefix-less) line.
        let parsed = parse(&s).unwrap();
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].r#type, "heading");
        assert_eq!(parsed.blocks[0].text.as_deref(), Some("line one line two"));
        // Re-serializing the parsed doc is stable.
        assert_eq!(serialize(&parsed), s);
    }

    #[test]
    fn empty_list_item_survives_roundtrip() {
        // `0.` parses as an ordered list with a single empty item. The
        // serializer must still emit a marker so the list does not vanish.
        let canonical = parse("0.\n\n?\n").unwrap();
        let once = serialize(&canonical);
        let twice = serialize(&parse(&once).unwrap());
        assert_eq!(once, twice);
    }

    #[test]
    fn roundtrip_nested_columns() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let d = parse(src).unwrap();
        let s = serialize(&d);
        let d2 = parse(&s).unwrap();
        assert_eq!(d, d2);
    }
}

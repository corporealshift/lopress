use crate::error::ParseError;

/// A delimiter token found in source.
#[derive(Debug, Clone, PartialEq)]
pub enum Delim {
    Open {
        name: String,
        attrs_json: String,
        span: (usize, usize),
    },
    Close {
        name: String,
        span: (usize, usize),
    },
}

/// Scan `src` for lopress block delimiters. Returns them in source order.
/// Non-lopress HTML comments are ignored.
pub fn scan(src: &str) -> Result<Vec<Delim>, ParseError> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let rest = &src[i + 4..];
            let end_off = match rest.find("-->") {
                Some(o) => o,
                None => break, // unterminated comment; leave for pulldown-cmark
            };
            let inner = rest[..end_off].trim();
            let span = (i, i + 4 + end_off + 3);

            if let Some(after_lop) = inner.strip_prefix("lopress:") {
                let (name, attrs_json) = split_name_and_attrs(after_lop);
                out.push(Delim::Open {
                    name,
                    attrs_json,
                    span,
                });
            } else if let Some(after_slash) = inner.strip_prefix("/lopress:") {
                let name = after_slash.trim().to_string();
                if name.is_empty() {
                    return Err(ParseError::FrontMatter(format!(
                        "empty close delimiter at byte {i}"
                    )));
                }
                out.push(Delim::Close { name, span });
            }
            i = span.1;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

/// Split `"<name> [<json>]"` into the name and the JSON string (empty if absent).
fn split_name_and_attrs(s: &str) -> (String, String) {
    let s = s.trim();
    match s.find(|c: char| c.is_whitespace() || c == '{') {
        Some(split) if s.as_bytes()[split] == b'{' => {
            let name = s[..split].trim().to_string();
            let attrs = s[split..].trim().to_string();
            (name, attrs)
        }
        Some(split) => {
            let name = s[..split].to_string();
            let attrs = s[split..].trim().to_string();
            (name, attrs)
        }
        None => (s.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_delimiters_in_plain_markdown() {
        assert!(scan("# hello\n\nparagraph\n").unwrap().is_empty());
    }

    #[test]
    fn self_closing_block_produces_open_and_close() {
        let src = r#"<!-- lopress:video {"src":"a.mp4"} -->
<!-- /lopress:video -->"#;
        let ds = scan(src).unwrap();
        assert_eq!(ds.len(), 2);
        match &ds[0] {
            Delim::Open {
                name, attrs_json, ..
            } => {
                assert_eq!(name, "video");
                assert_eq!(attrs_json, r#"{"src":"a.mp4"}"#);
            }
            _ => panic!("expected Open"),
        }
        match &ds[1] {
            Delim::Close { name, .. } => assert_eq!(name, "video"),
            _ => panic!("expected Close"),
        }
    }

    #[test]
    fn open_without_attrs_parses_cleanly() {
        let src = "<!-- lopress:callout -->\nhi\n<!-- /lopress:callout -->";
        let ds = scan(src).unwrap();
        assert_eq!(ds.len(), 2);
        if let Delim::Open {
            name, attrs_json, ..
        } = &ds[0]
        {
            assert_eq!(name, "callout");
            assert_eq!(attrs_json, "");
        } else {
            panic!("expected Open");
        }
    }

    #[test]
    fn non_lopress_comments_ignored() {
        let src = "<!-- just a comment -->\ntext\n<!-- another -->";
        assert!(scan(src).unwrap().is_empty());
    }

    #[test]
    fn nested_delimiters_preserved_in_order() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let ds = scan(src).unwrap();
        let names: Vec<_> = ds
            .iter()
            .map(|d| match d {
                Delim::Open { name, .. } => format!("+{name}"),
                Delim::Close { name, .. } => format!("-{name}"),
            })
            .collect();
        assert_eq!(
            names,
            vec!["+columns", "+column", "-column", "+column", "-column", "-columns"]
        );
    }
}

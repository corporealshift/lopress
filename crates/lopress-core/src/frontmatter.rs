use crate::error::ParseError;
use crate::types::FrontMatter;

/// Split `(front_matter, body)` from raw markdown. Returns the parsed
/// front-matter and the body content with leading `---\n...---\n` removed.
/// If there is no front-matter block, returns the default FrontMatter and
/// the input unchanged.
pub fn split(input: &str) -> Result<(FrontMatter, &str), ParseError> {
    // A front-matter block starts with a line that is exactly "---" and ends
    // with a subsequent line that is exactly "---". The content between is YAML.
    // Line terminators may be LF or CRLF (Windows checkouts with autocrlf).
    let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input); // BOM tolerance
    let open_len = if trimmed.starts_with("---\r\n") {
        5
    } else if trimmed.starts_with("---\n") {
        4
    } else {
        return Ok((FrontMatter::default(), input));
    };

    let after_open = trimmed
        .get(open_len..)
        .ok_or_else(|| ParseError::FrontMatter("truncated front-matter opener".into()))?;
    let mut offset = 0usize;
    let mut close: Option<(usize, usize)> = None;
    for segment in after_open.split_inclusive('\n') {
        let content = segment.trim_end_matches('\n').trim_end_matches('\r');
        if content == "---" {
            close = Some((offset, segment.len()));
            break;
        }
        offset += segment.len();
    }
    let (close_offset, close_segment_len) =
        close.ok_or_else(|| ParseError::FrontMatter("unterminated front-matter".into()))?;

    let yaml_src = after_open
        .get(..close_offset)
        .ok_or_else(|| ParseError::FrontMatter("invalid front-matter body span".into()))?;
    let fm: FrontMatter = if yaml_src.trim().is_empty() {
        FrontMatter::default()
    } else {
        serde_yaml::from_str(yaml_src)?
    };
    let body_start = open_len + close_offset + close_segment_len;
    let body = trimmed.get(body_start..).unwrap_or("");
    Ok((fm, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_front_matter_returns_default_and_full_body() {
        let (fm, body) = split("# hello\n").unwrap();
        assert_eq!(fm, FrontMatter::default());
        assert_eq!(body, "# hello\n");
    }

    #[test]
    fn parses_title_and_tags() {
        let input = "---\ntitle: Hi\ntags: [a, b]\n---\n# body\n";
        let (fm, body) = split(input).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Hi"));
        assert_eq!(fm.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(body, "# body\n");
    }

    #[test]
    fn parses_draft_and_date() {
        let input = "---\ndraft: true\ndate: 2026-04-18\n---\nbody\n";
        let (fm, body) = split(input).unwrap();
        assert!(fm.draft);
        assert_eq!(fm.date.map(|d| d.to_string()), Some("2026-04-18".into()));
        assert_eq!(body, "body\n");
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        let input = "---\ntitle: oops\n# body\n";
        assert!(split(input).is_err());
    }

    #[test]
    fn accepts_crlf_line_endings() {
        let input = "---\r\ntitle: Hi\r\ndraft: true\r\ntags: [intro]\r\n---\r\n# body\r\n";
        let (fm, body) = split(input).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Hi"));
        assert!(fm.draft);
        assert_eq!(fm.tags, vec!["intro".to_string()]);
        assert_eq!(body, "# body\r\n");
    }

    #[test]
    fn extra_fields_captured_in_extra() {
        let input = "---\ntitle: t\ncustom: value\n---\nbody\n";
        let (fm, _) = split(input).unwrap();
        assert_eq!(
            fm.extra.get("custom").and_then(|v| v.as_str()),
            Some("value")
        );
    }
}

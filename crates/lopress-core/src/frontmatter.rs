use crate::error::ParseError;
use crate::types::FrontMatter;

/// Split `(front_matter, body)` from raw markdown. Returns the parsed
/// front-matter and the body content with leading `---\n...---\n` removed.
/// If there is no front-matter block, returns the default FrontMatter and
/// the input unchanged.
pub fn split(input: &str) -> Result<(FrontMatter, &str), ParseError> {
    // A front-matter block starts with a line that is exactly "---" and ends
    // with a subsequent line that is exactly "---". The content between is YAML.
    let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input); // BOM tolerance
    if !trimmed.starts_with("---\n") && trimmed != "---" {
        return Ok((FrontMatter::default(), input));
    }
    let after_open = &trimmed[4..]; // skip "---\n"
    let close = after_open
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1; // assume trailing \n
            Some((start, line))
        })
        .find(|(_, line)| *line == "---")
        .ok_or_else(|| ParseError::FrontMatter("unterminated front-matter".into()))?;

    let (close_offset, _) = close;
    let yaml_src = &after_open[..close_offset];
    let fm: FrontMatter = if yaml_src.trim().is_empty() {
        FrontMatter::default()
    } else {
        serde_yaml::from_str(yaml_src)?
    };
    let body_start = 4 + close_offset + "---\n".len();
    let body = if body_start >= trimmed.len() {
        ""
    } else {
        &trimmed[body_start..]
    };
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
    fn extra_fields_captured_in_extra() {
        let input = "---\ntitle: t\ncustom: value\n---\nbody\n";
        let (fm, _) = split(input).unwrap();
        assert_eq!(fm.extra.get("custom").and_then(|v| v.as_str()), Some("value"));
    }
}

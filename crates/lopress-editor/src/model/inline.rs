use crate::model::types::InlineRun;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Parse a markdown inline string into `InlineRun`s.
///
/// Supported markers (round-trip preserved): bold (`**`), italic (`_`),
/// inline code (`` ` ``), links (`[text](url)`). Unsupported markers
/// (strikethrough, footnotes, raw HTML, etc.) are preserved verbatim
/// in the run text.
///
/// Escape policy: pulldown-cmark strips backslash-escapes on parse (e.g.
/// `\*` becomes `*` in the text). The serializer re-escapes bare `*`, `_`,
/// and `` ` `` characters that appear in plain (non-styled) runs so that
/// a round-trip parse → serialize → parse produces identical `InlineRun`s.
/// This means the raw bytes after one round-trip may differ from the
/// original source (the user's `\*` form is normalised to `\*` again via
/// the serializer's escaping), but the *semantic* content is preserved.
pub fn parse_inline(input: &str) -> Vec<InlineRun> {
    if input.is_empty() {
        return Vec::new();
    }
    let opts = Options::empty();
    let parser = Parser::new_ext(input, opts);

    let mut runs: Vec<InlineRun> = Vec::new();
    let mut style = StyleStack::default();

    for event in parser {
        match event {
            Event::Text(t) => push(&mut runs, &style, t.into_string()),
            Event::Code(t) => {
                let was_code = style.code;
                style.code = true;
                push(&mut runs, &style, t.into_string());
                style.code = was_code;
            }
            Event::Start(Tag::Strong) => style.bold += 1,
            Event::End(TagEnd::Strong) => style.bold = style.bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => style.italic += 1,
            Event::End(TagEnd::Emphasis) => style.italic = style.italic.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                style.link = Some(dest_url.into_string());
            }
            Event::End(TagEnd::Link) => style.link = None,
            Event::SoftBreak => push(&mut runs, &style, "\n".into()),
            Event::HardBreak => push(&mut runs, &style, "  \n".into()),
            // Block-level events shouldn't appear in inline-only input; ignore defensively.
            _ => {}
        }
    }

    coalesce(runs)
}

#[derive(Default)]
struct StyleStack {
    bold: u32,
    italic: u32,
    code: bool,
    link: Option<String>,
}

impl StyleStack {
    fn snapshot(&self) -> (bool, bool, bool, Option<String>) {
        (self.bold > 0, self.italic > 0, self.code, self.link.clone())
    }
}

fn push(out: &mut Vec<InlineRun>, style: &StyleStack, text: String) {
    if text.is_empty() {
        return;
    }
    let (b, i, c, l) = style.snapshot();
    out.push(InlineRun {
        text,
        bold: b,
        italic: i,
        code: c,
        link: l,
    });
}

fn coalesce(runs: Vec<InlineRun>) -> Vec<InlineRun> {
    let mut out: Vec<InlineRun> = Vec::with_capacity(runs.len());
    for r in runs {
        if let Some(last) = out.last_mut() {
            if last.bold == r.bold
                && last.italic == r.italic
                && last.code == r.code
                && last.link == r.link
            {
                last.text.push_str(&r.text);
                continue;
            }
        }
        out.push(r);
    }
    out
}

/// Escape markdown special characters in plain-text runs so that
/// a re-parse of the serialized output produces identical `InlineRun`s.
///
/// Only escapes characters that pulldown-cmark treats as inline markers
/// when they appear unprotected in plain text: `*`, `_`, `` ` ``.
/// `[` is escaped to prevent accidental link parsing.
fn escape_plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '*' | '_' | '`' | '[' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Serialize `InlineRun`s back to markdown.
///
/// Wrapping order for combined styles: code → italic → bold → link (link outermost).
/// Plain runs have their special characters escaped so the output is safe to re-parse.
/// Adjacent runs with identical style flags are merged before serializing so that
/// the output markdown does not produce ambiguous multi-`**` sequences.
pub fn serialize_inline(runs: &[InlineRun]) -> String {
    let merged = coalesce(runs.to_vec());
    let mut out = String::new();
    for r in &merged {
        let text: String = if r.code {
            // Inside a code span, content is literal — no escaping needed.
            format!("`{}`", r.text)
        } else {
            escape_plain(&r.text)
        };

        let text = if r.italic { format!("_{text}_") } else { text };

        let text = if r.bold { format!("**{text}**") } else { text };

        let text = if let Some(url) = &r.link {
            format!("[{text}]({url})")
        } else {
            text
        };

        out.push_str(&text);
    }
    out
}

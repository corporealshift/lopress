use crate::model::style_span::{coalesce_spans, StyleSpan};
use crate::model::types::{BlockBody, InlineRun, ListItem};
use lapce_xi_rope::Rope;

/// Convert `Vec<InlineRun>` into a flat `Rope` and parallel style spans.
/// Adjacent runs with identical styles coalesce into one span.
/// `\n` inside run text becomes a real newline in the rope.
pub fn inline_runs_to_rope_and_spans(runs: &[InlineRun]) -> (Rope, Vec<StyleSpan>) {
    let mut text = String::new();
    let mut spans: Vec<StyleSpan> = Vec::with_capacity(runs.len());
    let mut acc = 0usize;

    for run in runs {
        let byte_len = run.text.len();
        if byte_len > 0 {
            spans.push(StyleSpan {
                start: acc,
                end: acc + byte_len,
                bold: run.bold,
                italic: run.italic,
                code: run.code,
                link: run.link.clone(),
            });
        }
        text.push_str(&run.text);
        acc += byte_len;
    }

    coalesce_spans(&mut spans);
    (Rope::from(text.as_str()), spans)
}

/// Return `runs` in canonical form: empty-text runs dropped, and adjacent
/// runs with identical styling merged into one.
///
/// This is the single definition of "canonical" for an inline run list.
/// Two run lists that render identically compare equal once both are
/// canonicalized. The commit-diff logic in the block editors relies on this
/// to avoid emitting phantom no-op `EditBlockBody` actions: a body that was
/// just collected from the live editors must compare equal to the same body
/// stored in the model even though one path may have produced split runs
/// (e.g. a styled span plus a typed plain tail) and the other merged runs.
pub fn canonicalize_runs(runs: &[InlineRun]) -> Vec<InlineRun> {
    let mut out: Vec<InlineRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.bold == run.bold
                && last.italic == run.italic
                && last.code == run.code
                && last.link == run.link
            {
                last.text.push_str(&run.text);
                continue;
            }
        }
        out.push(run.clone());
    }
    out
}

/// Return `body` in canonical form. `Inline` and `List` bodies have their
/// run lists canonicalized via [`canonicalize_runs`]; `Code` and `Opaque`
/// bodies are returned unchanged. See [`canonicalize_runs`] for why the
/// `EditBlockBody` apply path canonicalizes before comparing.
pub fn canonicalize_body(body: &BlockBody) -> BlockBody {
    match body {
        BlockBody::Inline(runs) => BlockBody::Inline(canonicalize_runs(runs)),
        BlockBody::List(items) => BlockBody::List(
            items
                .iter()
                .map(|item| ListItem {
                    id: item.id,
                    runs: canonicalize_runs(&item.runs),
                })
                .collect(),
        ),
        BlockBody::Code(text) => BlockBody::Code(text.clone()),
        BlockBody::Opaque(value) => BlockBody::Opaque(value.clone()),
    }
}

/// Reconstruct `Vec<InlineRun>` from a `Rope` and its style spans.
///
/// Produces one `InlineRun` per span; `\n` in span text is preserved.
///
/// Spans are expected to be sorted by `start` and non-overlapping (as
/// produced by [`inline_runs_to_rope_and_spans`]). Any rope bytes *not*
/// covered by a span — gaps between adjacent spans, or a trailing range
/// after the last span — are emitted as plain runs. This makes the
/// function safe to call on a rope that has been edited past its original
/// styled extent (e.g. text typed at the end of a styled item), so the
/// typed bytes aren't silently dropped.
///
/// The result is in canonical form (see [`canonicalize_runs`]): this is
/// what makes `runs → inline_runs_to_rope_and_spans → editor →
/// rope_and_spans_to_runs` a round-trip identity, so callers comparing a
/// freshly-collected body against the stored model don't see phantom edits.
///
/// This function is designed for per-block usage. Calling it on a large rope
/// (document-level) allocates O(N) in document size.
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let rope_len = rope.len();
    let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);

    let mut cursor = 0usize;
    for span in spans {
        // Gap run before this span.
        if span.start > cursor {
            let gap_end = span.start.min(rope_len);
            let text = rope.slice_to_cow(cursor..gap_end);
            if !text.is_empty() {
                runs.push(InlineRun::plain(text.into_owned()));
            }
        }
        // The span itself.
        let span_end = span.end.min(rope_len);
        let span_start = span.start.min(span_end);
        let text = rope.slice_to_cow(span_start..span_end);
        if !text.is_empty() {
            runs.push(InlineRun {
                text: text.into_owned(),
                bold: span.bold,
                italic: span.italic,
                code: span.code,
                link: span.link.clone(),
            });
        }
        cursor = span_end.max(cursor);
    }
    // Trailing uncovered tail.
    if cursor < rope_len {
        let text = rope.slice_to_cow(cursor..rope_len);
        if !text.is_empty() {
            runs.push(InlineRun::plain(text.into_owned()));
        }
    }
    canonicalize_runs(&runs)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn rope_and_spans_to_runs_matches_string_roundtrip() {
        // Build a rope with known content: "Hello **bold** world"
        // Positions: H=0 e=1 l=2 l=3 o=4 ' '=5 *=6 *=7 b=8 o=9 l=10 d=11 *=12 *=13 ' '=14 ...
        let rope = Rope::from("Hello **bold** world");
        let spans = vec![StyleSpan {
            start: 6,
            end: 14,
            bold: true,
            italic: false,
            code: false,
            link: None,
        }];
        let runs = rope_and_spans_to_runs(&rope, &spans);

        // The output should have three runs: "Hello " (plain), "**bold**" (styled), " world" (plain).
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].text, "Hello ");
        assert!(!runs[0].bold);
        assert_eq!(runs[1].text, "**bold**");
        assert!(runs[1].bold);
        assert_eq!(runs[2].text, " world");
        assert!(!runs[2].bold);
    }

    #[test]
    fn rope_and_spans_to_runs_empty_spans_returns_plain() {
        let rope = Rope::from("plain text");
        let runs = rope_and_spans_to_runs(&rope, &[]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "plain text");
    }

    #[test]
    fn rope_and_spans_to_runs_multiline_preserves_newlines() {
        let rope = Rope::from("line1\nline2\nline3");
        let spans = vec![];
        let runs = rope_and_spans_to_runs(&rope, &spans);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "line1\nline2\nline3");
    }
}

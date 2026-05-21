use crate::model::style_span::{coalesce_spans, StyleSpan};
use crate::model::types::InlineRun;
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
/// This function is designed for per-block usage. Calling it on a large rope
/// (document-level) allocates O(N) in document size.
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let full = String::from(rope);
    let rope_len = full.len();
    let mut runs: Vec<InlineRun> = Vec::with_capacity(spans.len() + 1);
    let mut cursor = 0usize;
    for span in spans {
        // Emit a plain run for any gap between the previous span's end and
        // this span's start (typed text that the spans don't cover yet).
        if span.start > cursor {
            if let Some(text) = full.get(cursor..span.start) {
                if !text.is_empty() {
                    runs.push(InlineRun::plain(text.to_owned()));
                }
            }
        }
        // Clip the span to the rope's actual extent.
        let span_end = span.end.min(rope_len);
        let span_start = span.start.min(span_end);
        if let Some(text) = full.get(span_start..span_end) {
            if !text.is_empty() {
                runs.push(InlineRun {
                    text: text.to_owned(),
                    bold: span.bold,
                    italic: span.italic,
                    code: span.code,
                    link: span.link.clone(),
                });
            }
        }
        cursor = span_end.max(cursor);
    }
    // Trailing uncovered tail (typed text appended past the last span).
    if cursor < rope_len {
        if let Some(text) = full.get(cursor..rope_len) {
            if !text.is_empty() {
                runs.push(InlineRun::plain(text.to_owned()));
            }
        }
    }
    runs
}

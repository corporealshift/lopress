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
/// **Precondition:** `spans` must form a contiguous partition of `[0, rope.len())`.
/// Bytes not covered by any span are silently dropped. This invariant is guaranteed
/// by `inline_runs_to_rope_and_spans` and must be maintained by callers that
/// mutate spans directly.
///
/// This function is designed for per-block usage. Calling it on a large rope
/// (document-level) allocates O(N) in document size.
pub fn rope_and_spans_to_runs(rope: &Rope, spans: &[StyleSpan]) -> Vec<InlineRun> {
    let full = String::from(rope);
    spans
        .iter()
        .filter_map(|span| {
            let text = full.get(span.start..span.end)?.to_owned();
            Some(InlineRun {
                text,
                bold: span.bold,
                italic: span.italic,
                code: span.code,
                link: span.link.clone(),
            })
        })
        .collect()
}

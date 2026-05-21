#![allow(clippy::indexing_slicing)]

use lopress_editor::model::sync::{inline_runs_to_rope_and_spans, rope_and_spans_to_runs};
use lopress_editor::model::types::InlineRun;

fn plain_run(text: &str) -> InlineRun {
    InlineRun {
        text: text.into(),
        bold: false,
        italic: false,
        code: false,
        link: None,
    }
}
fn bold_run(text: &str) -> InlineRun {
    InlineRun {
        text: text.into(),
        bold: true,
        italic: false,
        code: false,
        link: None,
    }
}

#[test]
fn test_plain_roundtrip() {
    let runs = vec![plain_run("hello world")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_empty_roundtrip() {
    let runs: Vec<InlineRun> = vec![];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_mixed_style_roundtrip() {
    let runs = vec![plain_run("hello "), bold_run("world"), plain_run("!")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_coalesce_same_style() {
    let runs = vec![plain_run("hello "), plain_run("world")];
    let (_, spans) = inline_runs_to_rope_and_spans(&runs);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans.first().map(|s| s.start), Some(0));
    assert_eq!(spans.first().map(|s| s.end), Some(11));
}

#[test]
fn test_newline_roundtrip() {
    let runs = vec![plain_run("line one\nline two")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_unicode_roundtrip() {
    let runs = vec![plain_run("héllo"), bold_run(" wörld")];
    let (rope, spans) = inline_runs_to_rope_and_spans(&runs);
    let out = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(out, runs);
}

#[test]
fn test_spans_cover_full_byte_range() {
    let runs = vec![plain_run("abc"), bold_run("def")];
    let (_, spans) = inline_runs_to_rope_and_spans(&runs);
    assert_eq!(spans.first().map(|s| s.start), Some(0));
    assert_eq!(spans.first().map(|s| s.end), Some(3));
    assert_eq!(spans.get(1).map(|s| s.start), Some(3));
    assert_eq!(spans.get(1).map(|s| s.end), Some(6));
}

#[test]
fn test_trailing_uncovered_text_becomes_plain_run() {
    // Regression: when the user types past the end of the existing styled
    // extent (the editor mutates the rope but spans aren't auto-grown),
    // rope_and_spans_to_runs must NOT silently drop those bytes. The
    // typed tail becomes a plain run.
    use lapce_xi_rope::Rope;
    use lopress_editor::model::style_span::StyleSpan;
    let rope = Rope::from("Firsthello");
    // Spans only cover "First" (the original styled extent); "hello" was
    // typed after that and the spans haven't been extended.
    let spans = vec![StyleSpan {
        start: 0,
        end: 5,
        bold: false,
        italic: false,
        code: false,
        link: None,
    }];
    let runs = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "First");
    assert_eq!(runs[1].text, "hello");
    assert!(!runs[1].bold);
    assert!(!runs[1].italic);
}

#[test]
fn test_leading_uncovered_text_becomes_plain_run() {
    // Gap before the first span — also fills as plain.
    use lapce_xi_rope::Rope;
    use lopress_editor::model::style_span::StyleSpan;
    let rope = Rope::from("preBold");
    let spans = vec![StyleSpan {
        start: 3,
        end: 7,
        bold: true,
        italic: false,
        code: false,
        link: None,
    }];
    let runs = rope_and_spans_to_runs(&rope, &spans);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "pre");
    assert!(!runs[0].bold);
    assert_eq!(runs[1].text, "Bold");
    assert!(runs[1].bold);
}

#[test]
fn test_empty_spans_with_text_yields_plain_run() {
    // A rope with content and no spans (e.g. typing into an editor whose
    // initial runs were empty) — the whole rope is plain.
    use lapce_xi_rope::Rope;
    let rope = Rope::from("just typed text");
    let runs = rope_and_spans_to_runs(&rope, &[]);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "just typed text");
    assert!(!runs[0].bold);
}

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

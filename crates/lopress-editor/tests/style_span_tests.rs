use lopress_editor::ui::blocks::style_span::{
    coalesce_spans, split_span_at, toggle_inline, InlineFlag, StyleSpan,
};

fn plain(start: usize, end: usize) -> StyleSpan {
    StyleSpan { start, end, bold: false, italic: false, code: false, link: None }
}
fn bold(start: usize, end: usize) -> StyleSpan {
    StyleSpan { start, end, bold: true, italic: false, code: false, link: None }
}

#[test]
fn test_split_span_at_mid() {
    let mut spans = vec![plain(0, 10)];
    split_span_at(&mut spans, 4);
    assert_eq!(spans, vec![plain(0, 4), plain(4, 10)]);
}

#[test]
fn test_split_span_at_boundary_noop() {
    let mut spans = vec![plain(0, 5), plain(5, 10)];
    split_span_at(&mut spans, 5);
    assert_eq!(spans.len(), 2);
}

#[test]
fn test_coalesce_merges_same_style() {
    let mut spans = vec![plain(0, 3), plain(3, 7)];
    coalesce_spans(&mut spans);
    assert_eq!(spans, vec![plain(0, 7)]);
}

#[test]
fn test_coalesce_keeps_different_style() {
    let mut spans = vec![plain(0, 3), bold(3, 7)];
    coalesce_spans(&mut spans);
    assert_eq!(spans.len(), 2);
}

#[test]
fn test_toggle_sets_flag_when_partial() {
    let mut spans = vec![bold(0, 5), plain(5, 11)];
    toggle_inline(&mut spans, 0, 11, InlineFlag::Bold);
    assert!(spans.iter().all(|s| s.bold));
}

#[test]
fn test_toggle_clears_flag_when_all_set() {
    let mut spans = vec![bold(0, 5), bold(5, 11)];
    toggle_inline(&mut spans, 0, 11, InlineFlag::Bold);
    assert!(spans.iter().all(|s| !s.bold));
}

#[test]
fn test_toggle_collapsed_selection_noop() {
    let mut spans = vec![plain(0, 10)];
    toggle_inline(&mut spans, 5, 5, InlineFlag::Bold);
    assert!(!spans.get(0).unwrap().bold);
}

#[test]
fn test_toggle_partial_range() {
    let mut spans = vec![plain(0, 10)];
    toggle_inline(&mut spans, 2, 7, InlineFlag::Italic);
    assert_eq!(spans.len(), 3);
    assert!(!spans[0].italic);
    assert!(spans[1].italic);
    assert!(!spans[2].italic);
}

#[test]
fn test_toggle_coalesces_after_clear() {
    let mut spans = vec![bold(0, 3), bold(3, 6), bold(6, 10)];
    toggle_inline(&mut spans, 0, 10, InlineFlag::Bold);
    assert_eq!(spans, vec![plain(0, 10)]);
}

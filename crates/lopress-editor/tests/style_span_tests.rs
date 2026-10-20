use lopress_editor::model::style_span::{
    coalesce_spans, split_span_at, toggle_inline, InlineFlag, StyleSpan,
};

fn plain(start: usize, end: usize) -> StyleSpan {
    StyleSpan {
        start,
        end,
        bold: false,
        italic: false,
        code: false,
        link: None,
    }
}
fn bold(start: usize, end: usize) -> StyleSpan {
    StyleSpan {
        start,
        end,
        bold: true,
        italic: false,
        code: false,
        link: None,
    }
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
    assert!(spans.first().map(|s| !s.bold).unwrap_or(false));
}

#[test]
fn test_toggle_partial_range() {
    let mut spans = vec![plain(0, 10)];
    toggle_inline(&mut spans, 2, 7, InlineFlag::Italic);
    assert_eq!(spans.len(), 3);
    assert!(spans.first().map(|s| !s.italic).unwrap_or(false));
    assert!(spans.get(1).map(|s| s.italic).unwrap_or(false));
    assert!(spans.get(2).map(|s| !s.italic).unwrap_or(false));
}

#[test]
fn test_toggle_coalesces_after_clear() {
    let mut spans = vec![bold(0, 3), bold(3, 6), bold(6, 10)];
    toggle_inline(&mut spans, 0, 10, InlineFlag::Bold);
    assert_eq!(spans, vec![plain(0, 10)]);
}

// ── transform_spans: styles follow their text through edits (issue #57) ──

use lapce_xi_rope::{Delta, Interval, Rope};
use lopress_editor::model::style_span::transform_spans;

fn link(start: usize, end: usize) -> StyleSpan {
    StyleSpan {
        link: Some("https://openspec.dev".to_string()),
        ..plain(start, end)
    }
}

/// "I use OpenSpec at work." with "OpenSpec" linked at bytes 6..14.
fn openspec_spans() -> Vec<StyleSpan> {
    vec![plain(0, 6), link(6, 14), plain(14, 23)]
}

fn insert(spans: &mut Vec<StyleSpan>, at: usize, text: &str, base_len: usize) {
    let delta = Delta::simple_edit(Interval::new(at, at), Rope::from(text), base_len);
    transform_spans(spans, &delta);
}

fn delete(spans: &mut Vec<StyleSpan>, start: usize, end: usize, base_len: usize) {
    let delta = Delta::simple_edit(Interval::new(start, end), Rope::from(""), base_len);
    transform_spans(spans, &delta);
}

#[test]
fn typing_before_a_link_moves_the_link_with_its_text() {
    let mut spans = openspec_spans();
    insert(&mut spans, 0, "Hey. ", 23);
    assert_eq!(spans, vec![plain(0, 11), link(11, 19), plain(19, 28)]);
}

#[test]
fn typing_at_a_links_end_does_not_extend_it() {
    let mut spans = openspec_spans();
    insert(&mut spans, 14, "!", 23);
    assert_eq!(spans, vec![plain(0, 6), link(6, 14), plain(14, 24)]);
}

#[test]
fn typing_at_a_links_start_joins_the_text_before_it() {
    let mut spans = openspec_spans();
    insert(&mut spans, 6, "a ", 23);
    assert_eq!(spans, vec![plain(0, 8), link(8, 16), plain(16, 25)]);
}

#[test]
fn typing_inside_a_link_extends_it() {
    let mut spans = openspec_spans();
    insert(&mut spans, 10, "XY", 23);
    assert_eq!(spans, vec![plain(0, 6), link(6, 16), plain(16, 25)]);
}

#[test]
fn typing_before_a_link_at_the_start_of_the_text_leaves_a_plain_gap() {
    let mut spans = vec![link(0, 8), plain(8, 12)];
    insert(&mut spans, 0, "Go ", 12);
    assert_eq!(spans, vec![link(3, 11), plain(11, 15)]);
}

#[test]
fn typing_at_the_end_of_bold_text_continues_bold() {
    let mut spans = vec![plain(0, 4), bold(4, 8)];
    insert(&mut spans, 8, "er", 8);
    assert_eq!(spans, vec![plain(0, 4), bold(4, 10)]);
}

#[test]
fn deleting_text_before_a_link_moves_it_back() {
    let mut spans = openspec_spans();
    delete(&mut spans, 0, 2, 23);
    assert_eq!(spans, vec![plain(0, 4), link(4, 12), plain(12, 21)]);
}

#[test]
fn deleting_a_whole_link_drops_its_span() {
    let mut spans = openspec_spans();
    delete(&mut spans, 6, 14, 23);
    assert_eq!(spans, vec![plain(0, 6), plain(6, 15)]);
}

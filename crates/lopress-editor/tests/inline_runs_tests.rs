#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use lopress_editor::model::inline::{parse_inline, serialize_inline};
use lopress_editor::model::types::InlineRun;

fn r(text: &str, bold: bool, italic: bool, code: bool, link: Option<&str>) -> InlineRun {
    InlineRun {
        text: text.into(),
        bold,
        italic,
        code,
        link: link.map(String::from),
    }
}

#[test]
fn plain_text() {
    let runs = parse_inline("hello world");
    assert_eq!(runs, vec![r("hello world", false, false, false, None)]);
    assert_eq!(serialize_inline(&runs), "hello world");
}

#[test]
fn bold() {
    let runs = parse_inline("hello **world**");
    assert_eq!(
        runs,
        vec![
            r("hello ", false, false, false, None),
            r("world", true, false, false, None),
        ]
    );
    assert_eq!(serialize_inline(&runs), "hello **world**");
}

#[test]
fn italic_underscore() {
    let runs = parse_inline("hello _world_");
    assert_eq!(serialize_inline(&runs), "hello _world_");
}

#[test]
fn inline_code() {
    let runs = parse_inline("call `foo()`");
    assert_eq!(serialize_inline(&runs), "call `foo()`");
}

#[test]
fn link_simple() {
    let runs = parse_inline("see [docs](https://example.com)");
    assert_eq!(serialize_inline(&runs), "see [docs](https://example.com)");
}

#[test]
fn bold_inside_link() {
    let runs = parse_inline("[**bold link**](https://example.com)");
    assert_eq!(
        serialize_inline(&runs),
        "[**bold link**](https://example.com)"
    );
}

#[test]
fn link_with_parens_in_url() {
    let runs = parse_inline("[wikipedia](https://en.wikipedia.org/wiki/Foo_(bar))");
    let s = serialize_inline(&runs);
    let reparsed = parse_inline(&s);
    assert_eq!(reparsed, runs, "must round-trip identically");
}

#[test]
fn escaped_asterisk_is_literal() {
    // pulldown-cmark strips the backslash and emits the literal char as text.
    // After one parse, we have a plain-text run containing "*literal*".
    // The serializer escapes bare `*` in plain runs so the output round-trips.
    // We assert idempotence after one pass rather than byte-identity with the
    // original input (which used the escape sequence form the user typed).
    let runs = parse_inline(r"this is \*literal\*");
    let s = serialize_inline(&runs);
    let reparsed = parse_inline(&s);
    assert_eq!(reparsed, runs, "must be idempotent after one round-trip");
}

#[test]
fn empty_string() {
    let runs = parse_inline("");
    assert!(runs.is_empty());
    assert_eq!(serialize_inline(&runs), "");
}

#[test]
fn adjacent_same_style_coalesced() {
    // Use a form that pulldown-cmark actually parses as two distinct Strong spans:
    // bold run, then plain space, then another bold run. After removing the
    // space (by using a zero-width approach isn't feasible in commonmark), we
    // instead verify coalescing of runs that share ALL style flags — here we
    // construct the runs directly and verify coalesce via a round-trip.
    //
    // For the parser path: pulldown-cmark treats "**foo****bar**" as a single
    // Strong span containing the text "foo****bar" (the inner `****` are literal
    // asterisks per CommonMark spec). We therefore test coalescing through
    // serialize+reparse of two adjacent identical-style runs constructed manually.
    let runs = vec![
        r("foo", true, false, false, None),
        r("bar", true, false, false, None),
    ];
    let s = serialize_inline(&runs);
    // serialize produces "**foobar**" — one merged bold span
    let reparsed = parse_inline(&s);
    assert_eq!(
        reparsed.len(),
        1,
        "serialized adjacent bold should parse as one run"
    );
    assert_eq!(reparsed[0].text, "foobar");
    assert!(reparsed[0].bold);
}

#[test]
fn unsupported_strikethrough_passes_through_text() {
    // Strikethrough not in our subset; should be preserved verbatim
    let runs = parse_inline("~~struck~~");
    let s = serialize_inline(&runs);
    let reparsed = parse_inline(&s);
    assert_eq!(reparsed, runs);
}

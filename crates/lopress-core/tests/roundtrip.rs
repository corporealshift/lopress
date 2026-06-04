#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice
)]

use lopress_core::{parse, serialize, Block, Document, FrontMatter};
use proptest::prelude::*;
use serde_json::json;

fn arb_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,!?]{1,40}".prop_filter("no empty", |s| !s.trim().is_empty())
}

fn arb_paragraph() -> impl Strategy<Value = Block> {
    arb_text().prop_map(Block::paragraph)
}

fn arb_heading() -> impl Strategy<Value = Block> {
    (1u8..=6, arb_text()).prop_map(|(lvl, t)| Block::heading(lvl, t))
}

fn arb_custom_block() -> impl Strategy<Value = Block> {
    ("video|callout|note", arb_text()).prop_map(|(name, body)| Block {
        r#type: format!("lopress:{name}"),
        attrs: json!({ "id": body.len() }),
        children: vec![Block::paragraph(body)],
        text: None,
    })
}

fn arb_block() -> impl Strategy<Value = Block> {
    prop_oneof![arb_paragraph(), arb_heading(), arb_custom_block()]
}

fn arb_doc() -> impl Strategy<Value = Document> {
    prop::collection::vec(arb_block(), 1..8).prop_map(|blocks| Document {
        front_matter: FrontMatter::default(),
        blocks,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Spec contract: parse(serialize(parse(x))) == parse(x).
    /// We start with a Document from the generator, take it through one
    /// serialize+parse cycle to canonicalize it (so trailing whitespace and
    /// other "insignificant" variations are normalized), then verify that
    /// the canonical form is stable under further round-trips.
    #[test]
    fn parse_is_stable_under_roundtrip(doc in arb_doc()) {
        let canonical = parse(&serialize(&doc)).unwrap();
        let once = serialize(&canonical);
        let twice = serialize(&parse(&once).unwrap());
        prop_assert_eq!(once, twice);
    }
}

#[test]
fn read_more_marker_round_trips() {
    let src = "before\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nafter\n";
    let doc = parse(src).unwrap();
    let out = serialize(&doc);
    assert_eq!(out, src);
}

/// Sanity check: a fenced code block round-trips through parse → serialize
/// with the type name preserved end-to-end. Today this asserts `"code"` and
/// will FAIL until the parser is renamed in Task 2 — that failure is the
/// point of this characterization test.
#[test]
fn code_fence_round_trips() {
    let src = "```rust\nfn main() {}\n```\n";
    let doc = parse(src).unwrap();
    assert_eq!(doc.blocks[0].r#type, "code");
    assert_eq!(doc.blocks[0].attrs, json!({ "lang": "rust" }));
    assert_eq!(doc.blocks[0].text.as_deref(), Some("fn main() {}\n"));
    let out = serialize(&doc);
    assert_eq!(out, src);
}

#[test]
fn image_with_caption_round_trips() {
    let src = "![alt](foo.jpg \"My caption\")\n";
    let doc = parse(src).unwrap();
    assert_eq!(serialize(&doc), src);
}

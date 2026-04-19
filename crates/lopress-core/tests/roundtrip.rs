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

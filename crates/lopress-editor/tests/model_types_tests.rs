#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use lopress_editor::model::types::{BlockBody, EditorBlock, InlineRun};
use serde_json::json;

#[test]
fn paragraph_block_has_inline_body() {
    let p = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
    assert!(matches!(p.body, BlockBody::Inline(_)));
    let meta = &p.plugin;
    assert_eq!(meta.block_type_name.as_ref(), "paragraph");
    assert_eq!(meta.native.as_deref(), Some("paragraph"));
    assert!(meta.builtin);
    assert!(meta.attrs.is_empty());
}

#[test]
fn editor_block_constructors() {
    let p = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
    assert!(matches!(p.body, BlockBody::Inline(_)));
    if let BlockBody::Inline(runs) = &p.body {
        assert_eq!(runs.len(), 1);
    } else {
        panic!("expected Inline body");
    }
    let meta = &p.plugin;
    assert_eq!(meta.block_type_name.as_ref(), "paragraph");
    assert_eq!(meta.native.as_deref(), Some("paragraph"));
    assert!(meta.builtin);
    assert!(meta.attrs.is_empty());
}

#[test]
fn opaque_block_round_trips_value() {
    let v = json!({"foo": "bar"});
    let b = EditorBlock::opaque("custom".into(), v.clone());
    assert!(matches!(b.body, BlockBody::Opaque(_)));
    if let BlockBody::Opaque(stored) = &b.body {
        assert_eq!(stored, &v);
    } else {
        panic!("expected Opaque body");
    }
}

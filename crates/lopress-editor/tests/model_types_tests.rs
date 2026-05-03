#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use lopress_editor::model::types::*;
use serde_json::json;

#[test]
fn block_id_is_unique_and_monotonic() {
    let a = BlockId::new();
    let b = BlockId::new();
    assert_ne!(a, b);
}

#[test]
fn inline_run_default_has_no_styles() {
    let r = InlineRun::plain("hi");
    assert_eq!(r.text, "hi");
    assert!(!r.bold && !r.italic && !r.code);
    assert!(r.link.is_none());
}

#[test]
fn block_kind_paragraph_default() {
    let k = BlockKind::Paragraph;
    assert!(matches!(k, BlockKind::Paragraph));
}

#[test]
fn editor_block_constructors() {
    let p = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
    assert!(matches!(p.kind, BlockKind::Paragraph));
    if let BlockBody::Inline(runs) = &p.body {
        assert_eq!(runs.len(), 1);
    } else {
        panic!("expected Inline body");
    }
    assert!(p.plugin.is_none());
}

#[test]
fn opaque_block_round_trips_value() {
    let v = json!({"foo": "bar"});
    let b = EditorBlock::opaque("custom".into(), v.clone());
    assert!(matches!(b.kind, BlockKind::Opaque { .. }));
    if let BlockBody::Opaque(stored) = &b.body {
        assert_eq!(stored, &v);
    } else {
        panic!("expected Opaque body");
    }
}

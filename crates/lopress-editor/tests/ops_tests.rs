#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::missing_panics_doc
)]
use lopress_core::Block;
use lopress_editor::ops;
use serde_json::json;

fn para(t: &str) -> Block {
    Block::paragraph(t)
}
fn heading(lvl: u8, t: &str) -> Block {
    Block::heading(lvl, t)
}

// ── split_block_at_caret ────────────────────────────────────────────────────

#[test]
fn split_paragraph_at_middle() {
    let mut blocks = vec![para("hello world")];
    ops::split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hello"));
    assert_eq!(blocks[1].text.as_deref(), Some(" world"));
    assert_eq!(blocks[1].r#type, "paragraph");
}

#[test]
fn split_at_start_leaves_empty_first() {
    let mut blocks = vec![para("hello")];
    ops::split_block_at_caret(&mut blocks, 0, 0);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some(""));
    assert_eq!(blocks[1].text.as_deref(), Some("hello"));
}

#[test]
fn split_at_end_leaves_empty_second() {
    let mut blocks = vec![para("hello")];
    ops::split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hello"));
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

#[test]
fn split_heading_preserves_type() {
    let mut blocks = vec![heading(2, "Sec A rest")];
    ops::split_block_at_caret(&mut blocks, 0, 5);
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(blocks[1].r#type, "heading");
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(2)
    );
}

#[test]
fn split_caret_beyond_length_clamps() {
    let mut blocks = vec![para("hi")];
    ops::split_block_at_caret(&mut blocks, 0, 999);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("hi"));
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

// ── merge_with_previous ─────────────────────────────────────────────────────

#[test]
fn merge_appends_text_to_previous() {
    let mut blocks = vec![para("foo"), para("bar")];
    ops::merge_with_previous(&mut blocks, 1);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text.as_deref(), Some("foobar"));
}

#[test]
fn merge_at_zero_is_noop() {
    let mut blocks = vec![para("only")];
    ops::merge_with_previous(&mut blocks, 0);
    assert_eq!(blocks.len(), 1);
}

#[test]
fn merge_previous_type_wins() {
    let mut blocks = vec![heading(1, "Title"), para("body")];
    ops::merge_with_previous(&mut blocks, 1);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(blocks[0].text.as_deref(), Some("Titlebody"));
}

// ── change_block_type ───────────────────────────────────────────────────────

#[test]
fn change_paragraph_to_heading() {
    let mut blocks = vec![para("text")];
    ops::change_block_type(&mut blocks, 0, "heading", Some(3));
    assert_eq!(blocks[0].r#type, "heading");
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(3)
    );
    assert_eq!(blocks[0].text.as_deref(), Some("text"));
}

#[test]
fn change_heading_to_paragraph_clears_attrs() {
    let mut blocks = vec![heading(2, "hi")];
    ops::change_block_type(&mut blocks, 0, "paragraph", None);
    assert_eq!(blocks[0].r#type, "paragraph");
    assert!(blocks[0].attrs.as_object().is_some_and(|m| m.is_empty()));
}

#[test]
fn change_to_unknown_type_is_noop() {
    let mut blocks = vec![para("text")];
    ops::change_block_type(&mut blocks, 0, "code_block", None);
    assert_eq!(blocks[0].r#type, "paragraph");
}

#[test]
fn heading_level_clamped_to_1_6() {
    let mut blocks = vec![para("t")];
    ops::change_block_type(&mut blocks, 0, "heading", Some(0));
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(1)
    );
    ops::change_block_type(&mut blocks, 0, "heading", Some(9));
    assert_eq!(
        blocks[0].attrs.get("level").and_then(|v| v.as_u64()),
        Some(6)
    );
}

// ── add_paragraph_at_end ────────────────────────────────────────────────────

#[test]
fn add_paragraph_appends() {
    let mut blocks = vec![para("existing")];
    ops::add_paragraph_at_end(&mut blocks);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[1].r#type, "paragraph");
    assert_eq!(blocks[1].text.as_deref(), Some(""));
}

// ── delete_block ────────────────────────────────────────────────────────────

#[test]
fn delete_removes_block() {
    let mut blocks = vec![para("a"), para("b"), para("c")];
    ops::delete_block(&mut blocks, 1);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text.as_deref(), Some("a"));
    assert_eq!(blocks[1].text.as_deref(), Some("c"));
}

#[test]
fn delete_last_block_inserts_empty_paragraph() {
    let mut blocks = vec![para("only")];
    ops::delete_block(&mut blocks, 0);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].r#type, "paragraph");
    assert_eq!(blocks[0].text.as_deref(), Some(""));
}

#[test]
fn delete_out_of_bounds_is_noop() {
    let mut blocks = vec![para("a")];
    ops::delete_block(&mut blocks, 5);
    assert_eq!(blocks.len(), 1);
}

// ── is_editable ─────────────────────────────────────────────────────────────

#[test]
fn paragraph_is_editable() {
    assert!(ops::is_editable("paragraph"));
}

#[test]
fn heading_is_editable() {
    assert!(ops::is_editable("heading"));
}

#[test]
fn code_block_is_editable() {
    assert!(ops::is_editable("code_block"));
}

#[test]
fn list_is_editable() {
    assert!(ops::is_editable("list"));
}

#[test]
fn unknown_type_is_not_editable() {
    assert!(!ops::is_editable("image"));
    assert!(!ops::is_editable(""));
}

// ── list ops ────────────────────────────────────────────────────────────────

fn list_block(ordered: bool, items: &[&str]) -> Block {
    Block {
        r#type: "list".into(),
        attrs: json!({ "ordered": ordered }),
        children: items
            .iter()
            .map(|t| Block {
                r#type: "list_item".into(),
                attrs: json!({}),
                children: vec![Block::paragraph(*t)],
                text: None,
            })
            .collect(),
        text: None,
    }
}

#[test]
fn add_list_item_appends_empty_item() {
    let mut blocks = vec![list_block(false, &["first"])];
    ops::add_list_item(&mut blocks, 0);
    let list = blocks.get(0).unwrap();
    assert_eq!(list.children.len(), 2);
    let new_item = list.children.get(1).unwrap();
    let para = new_item.children.get(0).unwrap();
    assert_eq!(para.text.as_deref(), Some(""));
}

#[test]
fn delete_list_item_removes_item() {
    let mut blocks = vec![list_block(false, &["a", "b", "c"])];
    ops::delete_list_item(&mut blocks, 0, 1);
    let list = blocks.get(0).unwrap();
    assert_eq!(list.children.len(), 2);
    let remaining_para = list.children.get(0).and_then(|i| i.children.get(0)).unwrap();
    assert_eq!(remaining_para.text.as_deref(), Some("a"));
}

#[test]
fn delete_last_list_item_leaves_one_empty() {
    let mut blocks = vec![list_block(false, &["only"])];
    ops::delete_list_item(&mut blocks, 0, 0);
    let list = blocks.get(0).unwrap();
    assert_eq!(list.children.len(), 1);
    let para = list.children.get(0).and_then(|i| i.children.get(0)).unwrap();
    assert_eq!(para.text.as_deref(), Some(""));
}

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use lopress_editor::actions::{apply, BlockAction};
use lopress_editor::model::types::{
    BlockBody, BlockId, EditorBlock, EditorDoc, InlineRun, ListItem,
};

fn item(text: &str) -> ListItem {
    ListItem {
        id: BlockId::new(),
        runs: vec![InlineRun::plain(text)],
    }
}

fn list_doc(items: Vec<ListItem>) -> EditorDoc {
    EditorDoc {
        blocks: vec![EditorBlock::list(false, items)],
        front_matter: lopress_core::FrontMatter::default(),
    }
}

fn items_of(doc: &EditorDoc) -> Vec<String> {
    match &doc.blocks[0].body {
        BlockBody::List(items) => items
            .iter()
            .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect())
            .collect(),
        _ => panic!("not a list"),
    }
}

#[test]
fn edit_list_item_replaces_runs() {
    let it0 = item("old");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::EditListItem {
            block_id,
            item_id,
            new_runs: vec![InlineRun::plain("new")],
        },
    );
    assert_eq!(items_of(&doc), vec!["new"]);
}

#[test]
fn split_list_item_inserts_new_item_after() {
    let it0 = item("hello world");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset: 6,
        },
    );
    assert_eq!(items_of(&doc), vec!["hello ", "world"]);
}

#[test]
fn merge_list_item_with_prev_joins_into_predecessor() {
    let it0 = item("foo");
    let it1 = item("bar");
    let item_id = it1.id;
    let mut doc = list_doc(vec![it0, it1]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::MergeListItemWithPrev { block_id, item_id },
    );
    assert_eq!(items_of(&doc), vec!["foobar"]);
}

#[test]
fn merge_first_list_item_is_a_no_op() {
    let it0 = item("only");
    let item_id = it0.id;
    let mut doc = list_doc(vec![it0]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::MergeListItemWithPrev { block_id, item_id },
    );
    assert_eq!(items_of(&doc), vec!["only"]);
}

#[test]
fn split_on_a_list_block_splits_the_containing_item() {
    let mut doc = list_doc(vec![item("ab"), item("cd")]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::Split {
            block_id,
            byte_offset: 4,
        },
    );
    assert_eq!(items_of(&doc), vec!["ab", "c", "d"]);
}

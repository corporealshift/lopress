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
            new_block_id: None,
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
            new_block_id: None,
        },
    );
    assert_eq!(items_of(&doc), vec!["ab", "c", "d"]);
}

#[test]
fn merge_with_prev_on_a_list_lifts_first_item_and_keeps_the_rest() {
    // Backspace at the start of the first list item emits MergeWithPrev on
    // the list block — it must not delete the whole list.
    let mut doc = EditorDoc {
        blocks: vec![
            EditorBlock::paragraph(vec![InlineRun::plain("before ")]),
            EditorBlock::list(false, vec![item("First"), item("Second")]),
        ],
        front_matter: lopress_core::FrontMatter::default(),
    };
    let list_id = doc.blocks[1].id;
    apply(&mut doc, BlockAction::MergeWithPrev { block_id: list_id });
    assert_eq!(doc.blocks.len(), 2, "list block must survive the merge");
    match &doc.blocks[0].body {
        BlockBody::Inline(runs) => {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(text, "before First");
        }
        _ => panic!("previous block should stay an inline paragraph"),
    }
    match &doc.blocks[1].body {
        BlockBody::List(items) => {
            let texts: Vec<String> = items
                .iter()
                .map(|it| it.runs.iter().map(|r| r.text.as_str()).collect())
                .collect();
            assert_eq!(texts, vec!["Second"]);
        }
        _ => panic!("remaining items should stay a list"),
    }
}

#[test]
fn merge_with_prev_on_a_single_item_list_drops_the_emptied_list() {
    let mut doc = EditorDoc {
        blocks: vec![
            EditorBlock::paragraph(vec![InlineRun::plain("p ")]),
            EditorBlock::list(false, vec![item("only")]),
        ],
        front_matter: lopress_core::FrontMatter::default(),
    };
    let list_id = doc.blocks[1].id;
    apply(&mut doc, BlockAction::MergeWithPrev { block_id: list_id });
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0].body {
        BlockBody::Inline(runs) => {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(text, "p only");
        }
        _ => panic!("expected merged paragraph"),
    }
}

#[test]
fn split_list_item_with_new_item_id_uses_provided_id() {
    let it0 = item("first item");
    let it1 = item("second");
    let item_id = it0.id;
    let target_id = BlockId::new();
    let mut doc = list_doc(vec![it0, it1]);
    let block_id = doc.blocks[0].id;
    apply(
        &mut doc,
        BlockAction::SplitListItem {
            block_id,
            item_id,
            byte_offset: 5,
            new_block_id: Some(target_id),
        },
    );
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!("expected list body");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[1].id, target_id);
}

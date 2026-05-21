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
#[test]
fn split_list_item_round_trip_id_stable() {
    let it0 = item("alpha");
    let it1 = item("beta");
    let item_a = it0.id;
    let mut doc = list_doc(vec![it0, it1]);
    let block_id = doc.blocks[0].id;
    let before = doc.clone();

    let (canonical, inverse) = apply(
        &mut doc,
        BlockAction::SplitListItem {
            block_id,
            item_id: item_a,
            byte_offset: 3,
            new_block_id: None,
        },
    )
    .unwrap();

    let minted_item_id = match &canonical {
        BlockAction::SplitListItem {
            new_block_id: Some(nid),
            ..
        } => *nid,
        _ => panic!("canonical must carry concrete new_block_id"),
    };
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!()
    };
    assert_eq!(items[1].id, minted_item_id);

    // Undo.
    let _ = apply(&mut doc, inverse).unwrap();
    assert_eq!(doc.blocks.len(), before.blocks.len());
    let BlockBody::List(items_after) = &doc.blocks[0].body else {
        panic!()
    };
    let BlockBody::List(items_before) = &before.blocks[0].body else {
        panic!()
    };
    assert_eq!(items_after.len(), items_before.len());
}

#[test]
fn editing_multiple_items_then_splitting_one_preserves_all_edits() {
    // Regression for the uncommitted-list-item-edit-loss bug
    // (docs/superpowers/ideas/2026-05-18-list-item-uncommitted-edit-loss.md).
    //
    // The UI fix in stage 4 task 3 builds a complete new BlockBody::List
    // from every item's live buffer + the structural mutation, and emits a
    // single EditBlockBody. This action-layer test verifies the apply path
    // handles that body shape correctly.
    let it0 = item("item zero original");
    let it1 = item("item one original");
    let it2 = item("item two original");
    let ids = vec![it0.id, it1.id, it2.id];
    let list = EditorBlock::list(false, vec![it0, it1, it2]);
    let block_id = list.id;
    let mut doc = list_doc(vec![]);
    doc.blocks[0] = list;

    // User typed into items 0 and 2 but did not commit. Now they press
    // Enter in item 1 to split it. The UI captures everyone's live buffer
    // plus the split into a single EditBlockBody.
    let new_item_after_split = BlockId::new();
    let new_body = BlockBody::List(vec![
        ListItem {
            id: ids[0],
            runs: vec![InlineRun::plain("item zero edited")],
        },
        ListItem {
            id: ids[1],
            runs: vec![InlineRun::plain("item one ed")],
        },
        ListItem {
            id: new_item_after_split,
            runs: vec![InlineRun::plain("ited")],
        },
        ListItem {
            id: ids[2],
            runs: vec![InlineRun::plain("item two edited")],
        },
    ]);
    let (_canonical, inverse) =
        apply(&mut doc, BlockAction::EditBlockBody { block_id, new_body }).unwrap();

    // All four items present with the right text — no edit was lost.
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!("expected list body");
    };
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].runs[0].text, "item zero edited");
    assert_eq!(items[1].runs[0].text, "item one ed");
    assert_eq!(items[2].runs[0].text, "ited");
    assert_eq!(items[2].id, new_item_after_split);
    assert_eq!(items[3].runs[0].text, "item two edited");

    // Inverse round-trip restores the original three items with original ids.
    let _ = apply(&mut doc, inverse).unwrap();
    let BlockBody::List(items) = &doc.blocks[0].body else {
        panic!();
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items.iter().map(|it| it.id).collect::<Vec<_>>(), ids);
}

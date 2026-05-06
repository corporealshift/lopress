#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::types::{BlockKind, InlineRun};
use lopress_editor::ui::blocks::inline_editor::block_is_empty;
use lopress_editor::ui::slash_menu::slash_menu_items;

#[test]
fn block_is_empty_for_no_runs() {
    let runs: Vec<InlineRun> = Vec::new();
    assert!(block_is_empty(&runs));
}

#[test]
fn block_is_empty_for_runs_with_only_empty_text() {
    let runs = vec![InlineRun::plain(""), InlineRun::plain("")];
    assert!(block_is_empty(&runs));
}

#[test]
fn block_is_not_empty_when_any_run_has_text() {
    let runs = vec![InlineRun::plain(""), InlineRun::plain("x")];
    assert!(!block_is_empty(&runs));
}

#[test]
fn slash_menu_items_match_acceptance_list() {
    let items = slash_menu_items();
    let labels: Vec<&'static str> = items.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        labels,
        vec![
            "Paragraph",
            "Heading 1",
            "Heading 2",
            "Heading 3",
            "Code block",
            "Unordered list",
            "Ordered list",
        ]
    );
    // Spot-check a few kinds — heading levels in particular.
    assert!(matches!(items[1].1, BlockKind::Heading(1)));
    assert!(matches!(items[3].1, BlockKind::Heading(3)));
    assert!(matches!(items[5].1, BlockKind::List { ordered: false }));
    assert!(matches!(items[6].1, BlockKind::List { ordered: true }));
}

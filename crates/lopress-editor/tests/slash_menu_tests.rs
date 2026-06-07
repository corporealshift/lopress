#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::ui::slash_menu::{slash_menu_items, SlashChoice};

#[test]
fn slash_menu_items_match_acceptance_list() {
    let items = slash_menu_items();
    let labels: Vec<String> = items.iter().map(|(l, _)| l.clone()).collect();
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
            "Image",
            "Read more",
            "Separator",
            "Table",
        ]
    );
    // Spot-check a few editors — heading levels in particular.
    assert!(matches!(
        &items[1].1,
        SlashChoice::ChangeType { new_editor, .. } if &**new_editor == "heading"
    ));
    assert!(matches!(
        &items[3].1,
        SlashChoice::ChangeType { new_editor, .. } if &**new_editor == "heading"
    ));
    assert!(matches!(
        &items[5].1,
        SlashChoice::ChangeType { new_editor, .. } if &**new_editor == "list"
    ));
    assert!(matches!(
        &items[6].1,
        SlashChoice::ChangeType { new_editor, .. } if &**new_editor == "list"
    ));
    assert!(matches!(items[7].1, SlashChoice::Image));
    assert!(matches!(items[8].1, SlashChoice::ReadMore));
}

#[test]
fn slash_items_include_image() {
    let items = slash_menu_items();
    assert!(items
        .iter()
        .any(|(label, choice)| *label == "Image" && matches!(choice, SlashChoice::Image)));
}

#[test]
fn slash_items_include_read_more() {
    let items = slash_menu_items();
    assert!(
        items.iter().any(|(label, choice)| *label == "Read more"
            && matches!(choice, SlashChoice::ReadMore)),
        "expected a Read more / SlashChoice::ReadMore entry"
    );
}

#[test]
fn paragraph_entry_is_a_change_type_choice() {
    let items = slash_menu_items();
    assert!(items.iter().any(|(label, choice)| *label == "Paragraph"
        && matches!(choice, SlashChoice::ChangeType { new_editor, .. } if &**new_editor == "paragraph")));
}

#[test]
fn includes_separator_and_table() {
    let items = slash_menu_items();
    assert!(items
        .iter()
        .any(|(l, c)| l == "Separator" && matches!(c, SlashChoice::Separator)));
    assert!(items
        .iter()
        .any(|(l, c)| l == "Table" && matches!(c, SlashChoice::Table)));
}

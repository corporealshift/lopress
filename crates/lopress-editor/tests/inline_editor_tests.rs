#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::types::InlineRun;
use lopress_editor::ui::blocks::inline_editor::{
    backspace, compare, delete, delete_selection, insert_char, move_left, move_right, Caret,
    LocalSelection,
};

fn plain(t: &str) -> InlineRun {
    InlineRun::plain(t)
}

fn bold(t: &str) -> InlineRun {
    InlineRun {
        text: t.into(),
        bold: true,
        ..Default::default()
    }
}

#[test]
fn insert_char_into_empty() {
    let mut runs = Vec::new();
    let c = insert_char(&mut runs, Caret::START, 'h');
    assert_eq!(runs, vec![plain("h")]);
    assert_eq!(c, Caret { run: 0, offset: 1 });
}

#[test]
fn insert_char_into_middle() {
    let mut runs = vec![plain("helo")];
    let c = insert_char(&mut runs, Caret { run: 0, offset: 3 }, 'l');
    assert_eq!(runs[0].text, "hello");
    assert_eq!(c, Caret { run: 0, offset: 4 });
}

#[test]
fn insert_char_at_end_advances_caret() {
    let mut runs = vec![plain("ab")];
    let c = insert_char(&mut runs, Caret { run: 0, offset: 2 }, 'c');
    assert_eq!(runs[0].text, "abc");
    assert_eq!(c, Caret { run: 0, offset: 3 });
}

#[test]
fn insert_char_handles_multibyte() {
    let mut runs = vec![plain("a")];
    let c1 = insert_char(&mut runs, Caret { run: 0, offset: 1 }, 'é');
    assert_eq!(runs[0].text, "aé");
    assert_eq!(c1, Caret { run: 0, offset: 2 });
    let c2 = insert_char(&mut runs, c1, 'b');
    assert_eq!(runs[0].text, "aéb");
    assert_eq!(c2, Caret { run: 0, offset: 3 });
}

#[test]
fn backspace_within_run() {
    let mut runs = vec![plain("hello")];
    let c = backspace(&mut runs, Caret { run: 0, offset: 3 });
    assert_eq!(runs[0].text, "helo");
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn backspace_at_block_start_is_noop() {
    let mut runs = vec![plain("hello")];
    let c = backspace(&mut runs, Caret::START);
    assert_eq!(runs[0].text, "hello");
    assert_eq!(c, Caret::START);
}

#[test]
fn backspace_at_run_boundary_merges_to_prev() {
    let mut runs = vec![plain("hello "), bold("world")];
    let c = backspace(&mut runs, Caret { run: 1, offset: 0 });
    assert_eq!(c.run, 0);
    assert_eq!(runs[0].text, "hello");
    assert_eq!(runs[1].text, "world");
}

#[test]
fn backspace_coalesces_when_styles_match() {
    let mut runs = vec![plain("hi "), plain("there")];
    let c = backspace(&mut runs, Caret { run: 1, offset: 0 });
    // Removed the space at end of run 0; runs now share style → coalesce.
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "hithere");
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn delete_within_run() {
    let mut runs = vec![plain("hello")];
    let c = delete(&mut runs, Caret { run: 0, offset: 1 });
    assert_eq!(runs[0].text, "hllo");
    assert_eq!(c, Caret { run: 0, offset: 1 });
}

#[test]
fn delete_at_block_end_is_noop() {
    let mut runs = vec![plain("hi")];
    let c = delete(&mut runs, Caret { run: 0, offset: 2 });
    assert_eq!(runs[0].text, "hi");
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn delete_forward_across_run_boundary() {
    let mut runs = vec![plain("ab"), plain("cd")];
    let _ = delete(&mut runs, Caret { run: 0, offset: 2 });
    // Forward-deletes the 'c' from run 1; remaining runs ("ab","d") share
    // style → coalesce.
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "abd");
}

#[test]
fn move_left_within_run() {
    let runs = vec![plain("hello")];
    let c = move_left(&runs, Caret { run: 0, offset: 3 });
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn move_left_at_block_start_is_clamped() {
    let runs = vec![plain("hi")];
    let c = move_left(&runs, Caret::START);
    assert_eq!(c, Caret::START);
}

#[test]
fn move_left_crosses_run_boundary() {
    let runs = vec![plain("ab"), plain("cd")];
    let c = move_left(&runs, Caret { run: 1, offset: 0 });
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn move_right_within_run() {
    let runs = vec![plain("hello")];
    let c = move_right(&runs, Caret { run: 0, offset: 0 });
    assert_eq!(c, Caret { run: 0, offset: 1 });
}

#[test]
fn move_right_crosses_run_boundary() {
    let runs = vec![plain("ab"), plain("cd")];
    let c = move_right(&runs, Caret { run: 0, offset: 2 });
    assert_eq!(c, Caret { run: 1, offset: 0 });
}

#[test]
fn move_right_at_block_end_is_clamped() {
    let runs = vec![plain("hi")];
    let c = move_right(&runs, Caret { run: 0, offset: 2 });
    assert_eq!(c, Caret { run: 0, offset: 2 });
}

#[test]
fn caret_end_of_empty_runs_is_start() {
    let runs: Vec<InlineRun> = Vec::new();
    assert_eq!(Caret::end(&runs), Caret::START);
}

#[test]
fn caret_end_of_runs() {
    let runs = vec![plain("ab"), bold("cdé")];
    assert_eq!(Caret::end(&runs), Caret { run: 1, offset: 3 });
}

#[test]
fn compare_orders_by_run_then_offset() {
    use std::cmp::Ordering;
    assert_eq!(
        compare(Caret { run: 0, offset: 1 }, Caret { run: 0, offset: 2 }),
        Ordering::Less
    );
    assert_eq!(
        compare(Caret { run: 1, offset: 0 }, Caret { run: 0, offset: 9 }),
        Ordering::Greater
    );
    assert_eq!(
        compare(Caret { run: 0, offset: 3 }, Caret { run: 0, offset: 3 }),
        Ordering::Equal
    );
}

#[test]
fn local_selection_ordered_swaps_when_head_before_anchor() {
    let sel = LocalSelection {
        anchor: Caret { run: 1, offset: 2 },
        head: Caret { run: 0, offset: 1 },
    };
    let (start, end) = sel.ordered();
    assert_eq!(start, Caret { run: 0, offset: 1 });
    assert_eq!(end, Caret { run: 1, offset: 2 });
}

#[test]
fn delete_selection_collapsed_is_noop() {
    let mut runs = vec![plain("hello")];
    let sel = LocalSelection::caret(Caret { run: 0, offset: 2 });
    let new = delete_selection(&mut runs, sel);
    assert_eq!(runs[0].text, "hello");
    assert_eq!(new, sel);
}

#[test]
fn delete_selection_within_run() {
    let mut runs = vec![plain("hello world")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 5 },
        head: Caret { run: 0, offset: 11 },
    };
    let new = delete_selection(&mut runs, sel);
    assert_eq!(runs[0].text, "hello");
    assert_eq!(new, LocalSelection::caret(Caret { run: 0, offset: 5 }));
}

#[test]
fn delete_selection_handles_reversed_anchor_head() {
    let mut runs = vec![plain("abcdef")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 5 },
        head: Caret { run: 0, offset: 1 },
    };
    let new = delete_selection(&mut runs, sel);
    assert_eq!(runs[0].text, "af");
    assert_eq!(new, LocalSelection::caret(Caret { run: 0, offset: 1 }));
}

#[test]
fn delete_selection_across_runs_preserves_remaining_styles() {
    let mut runs = vec![plain("hello "), bold("world")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 3 },
        head: Caret { run: 1, offset: 3 },
    };
    let _ = delete_selection(&mut runs, sel);
    // "hel" (plain) + "ld" (bold) — different styles, should not coalesce.
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "hel");
    assert!(!runs[0].bold);
    assert_eq!(runs[1].text, "ld");
    assert!(runs[1].bold);
}

#[test]
fn delete_selection_drops_runs_fully_inside_range() {
    let mut runs = vec![plain("a"), bold("BBB"), plain("c")];
    let sel = LocalSelection {
        anchor: Caret { run: 0, offset: 1 },
        head: Caret { run: 2, offset: 0 },
    };
    let new = delete_selection(&mut runs, sel);
    // Runs 0 & 2 untouched at their boundaries; run 1 deleted entirely.
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "ac");
    assert_eq!(new, LocalSelection::caret(Caret { run: 0, offset: 1 }));
}

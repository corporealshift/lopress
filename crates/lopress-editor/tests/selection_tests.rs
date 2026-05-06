#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::model::types::{BlockId, EditorBlock, EditorDoc, InlineRun};
use lopress_editor::selection::{
    compare_positions, doc_end_position, doc_start_position, project, BlockSelection,
    DocPosition, DocSelection, GeometryCache, APPROX_CHAR_RATIO,
};
use lopress_editor::ui::blocks::inline_editor::{Caret, LocalSelection};

fn doc_of(texts: &[&str]) -> (EditorDoc, Vec<BlockId>) {
    let blocks: Vec<EditorBlock> = texts
        .iter()
        .map(|t| EditorBlock::paragraph(vec![InlineRun::plain(*t)]))
        .collect();
    let ids: Vec<BlockId> = blocks.iter().map(|b| b.id).collect();
    let doc = EditorDoc {
        blocks,
        front_matter: lopress_core::FrontMatter::default(),
    };
    (doc, ids)
}

#[test]
fn caret_constructor_collapses() {
    let p = DocPosition::new(BlockId::new(), 0, 0);
    let s = DocSelection::caret(p);
    assert!(s.is_collapsed());
}

#[test]
fn compare_positions_orders_by_block_then_run_then_offset() {
    let (doc, ids) = doc_of(&["abc", "def"]);
    let a = DocPosition::new(ids[0], 0, 1);
    let b = DocPosition::new(ids[1], 0, 0);
    assert!(compare_positions(a, b, &doc).is_lt());
    assert!(compare_positions(b, a, &doc).is_gt());

    let c = DocPosition::new(ids[0], 0, 2);
    assert!(compare_positions(a, c, &doc).is_lt());
}

#[test]
fn ordered_returns_min_max() {
    let (doc, ids) = doc_of(&["x", "y"]);
    let p1 = DocPosition::new(ids[0], 0, 0);
    let p2 = DocPosition::new(ids[1], 0, 0);
    let sel = DocSelection { anchor: p2, head: p1 };
    let (lo, hi) = sel.ordered(&doc);
    assert_eq!(lo, p1);
    assert_eq!(hi, p2);
}

#[test]
fn project_within_single_block_returns_local() {
    let (doc, ids) = doc_of(&["hello world"]);
    let sel = DocSelection {
        anchor: DocPosition::new(ids[0], 0, 0),
        head: DocPosition::new(ids[0], 0, 5),
    };
    let block_view = project(sel, &doc.blocks[0], &doc);
    assert_eq!(
        block_view,
        BlockSelection::Local {
            local: LocalSelection {
                anchor: Caret { run: 0, offset: 0 },
                head: Caret { run: 0, offset: 5 },
            },
            holds_head: true,
        }
    );
}

#[test]
fn project_into_outside_block_is_none() {
    let (doc, ids) = doc_of(&["abc", "def"]);
    let sel = DocSelection::caret(DocPosition::new(ids[0], 0, 1));
    let block_view = project(sel, &doc.blocks[1], &doc);
    assert_eq!(block_view, BlockSelection::None);
}

#[test]
fn project_three_block_selection() {
    let (doc, ids) = doc_of(&["aaa", "bbb", "ccc"]);
    // Anchor in block 0 at offset 1; head in block 2 at offset 2.
    let sel = DocSelection {
        anchor: DocPosition::new(ids[0], 0, 1),
        head: DocPosition::new(ids[2], 0, 2),
    };
    // Block 0 trails out the bottom with anchor inside.
    let v0 = project(sel, &doc.blocks[0], &doc);
    assert_eq!(
        v0,
        BlockSelection::Trailing {
            start: Caret { run: 0, offset: 1 },
            holds_head: false,
        }
    );
    // Block 1 is fully selected.
    let v1 = project(sel, &doc.blocks[1], &doc);
    assert_eq!(v1, BlockSelection::Full);
    // Block 2 leads in from the top with head inside.
    let v2 = project(sel, &doc.blocks[2], &doc);
    assert_eq!(
        v2,
        BlockSelection::Leading {
            end: Caret { run: 0, offset: 2 },
            holds_head: true,
        }
    );
}

#[test]
fn step_right_advances_within_block_then_hops() {
    let (doc, ids) = doc_of(&["ab", "cd"]);
    let p = DocPosition::new(ids[0], 0, 0);
    let p = p.step_right(&doc);
    assert_eq!(p, DocPosition::new(ids[0], 0, 1));
    let p = p.step_right(&doc);
    assert_eq!(p, DocPosition::new(ids[0], 0, 2));
    // At end of block 0; next step hops to start of block 1.
    let p = p.step_right(&doc);
    assert_eq!(p, DocPosition::new(ids[1], 0, 0));
}

#[test]
fn step_left_retreats_within_block_then_hops() {
    let (doc, ids) = doc_of(&["ab", "cd"]);
    let p = DocPosition::new(ids[1], 0, 0);
    // Hops to end of block 0.
    let p = p.step_left(&doc);
    assert_eq!(p, DocPosition::new(ids[0], 0, 2));
    let p = p.step_left(&doc);
    assert_eq!(p, DocPosition::new(ids[0], 0, 1));
}

#[test]
fn doc_start_and_end_positions() {
    let (doc, ids) = doc_of(&["one", "two", "three"]);
    let s = doc_start_position(&doc);
    assert_eq!(s, DocPosition::new(ids[0], 0, 0));
    let e = doc_end_position(&doc);
    assert_eq!(e, DocPosition::new(ids[2], 0, 5));
}

#[test]
fn nearest_offset_finds_closest() {
    let id = BlockId::new();
    let mut cache = GeometryCache::default();
    cache.put(id, vec![0.0, 10.0, 20.0, 30.0]);
    assert_eq!(cache.nearest_offset(id, 11.0), Some(1));
    assert_eq!(cache.nearest_offset(id, 25.0), Some(2));
    assert_eq!(cache.nearest_offset(id, -5.0), Some(0));
    assert_eq!(cache.nearest_offset(id, 100.0), Some(3));
}

#[test]
fn x_at_clamps_offset() {
    let id = BlockId::new();
    let mut cache = GeometryCache::default();
    cache.put(id, vec![0.0, 7.0, 14.0]);
    assert_eq!(cache.x_at(id, 0), Some(0.0));
    assert_eq!(cache.x_at(id, 1), Some(7.0));
    assert_eq!(cache.x_at(id, 50), Some(14.0));
    assert_eq!(cache.x_at(BlockId::new(), 0), None);
}

#[test]
fn approximate_for_produces_n_plus_one_entries() {
    let xs = GeometryCache::approximate_for("hello", 16.0);
    assert_eq!(xs.len(), 6);
    assert_eq!(xs[0], 0.0);
    assert!((xs[1] - 16.0 * APPROX_CHAR_RATIO).abs() < 1e-3);
    assert!((xs[5] - 16.0 * APPROX_CHAR_RATIO * 5.0).abs() < 1e-3);
}

//! Document-level selection, owned by `editor_pane`.
//!
//! The pane owns one `RwSignal<DocSelection>`. Each per-block widget reads its
//! slice of that selection (which it converts to local caret/range geometry
//! for painting) and writes back via callbacks routed through the pane.
//!
//! The `GeometryCache` records, for each block, the x-position (in the
//! block's local coordinate system) of every character offset that the block
//! can hold. Vertical-arrow navigation across blocks consults the source
//! block's cache to read the current x, then asks the target block for the
//! offset whose cached x is closest.

use crate::model::types::{BlockBody, BlockId, EditorBlock, EditorDoc};
use crate::ui::blocks::inline_editor::{Caret, LocalSelection};
use std::cmp::Ordering;
use std::collections::HashMap;

/// A position within a specific block. `run` and `offset` follow the same
/// convention as `Caret`: `run` indexes into `block.body.runs`, `offset`
/// counts characters within `runs[run].text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocPosition {
    pub block: BlockId,
    pub run: usize,
    pub offset: usize,
}

impl DocPosition {
    pub fn new(block: BlockId, run: usize, offset: usize) -> Self {
        Self { block, run, offset }
    }

    /// `(block, START_CARET)` — useful for "doc start" or "block start".
    pub fn block_start(block: BlockId) -> Self {
        Self {
            block,
            run: 0,
            offset: 0,
        }
    }
}

/// Doc-level selection. `anchor` is fixed; `head` is what moves under
/// shift-extension. A collapsed selection (`anchor == head`) is the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocSelection {
    pub anchor: DocPosition,
    pub head: DocPosition,
}

impl DocSelection {
    pub fn caret(p: DocPosition) -> Self {
        Self { anchor: p, head: p }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// `(min, max)` in document order, given the block ordering from `doc`.
    /// If a position references a block that's no longer in the doc, treats
    /// that position as "after end" (a degenerate but defensive choice).
    pub fn ordered(&self, doc: &EditorDoc) -> (DocPosition, DocPosition) {
        if compare_positions(self.anchor, self.head, doc).is_le() {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// Document order on (block_index, run, offset). Blocks not present in `doc`
/// sort after all present blocks (and against each other by raw `BlockId`,
/// which is stable but arbitrary).
pub fn compare_positions(a: DocPosition, b: DocPosition, doc: &EditorDoc) -> Ordering {
    let ai = block_index(doc, a.block);
    let bi = block_index(doc, b.block);
    match (ai, bi) {
        (Some(ai), Some(bi)) => ai
            .cmp(&bi)
            .then(a.run.cmp(&b.run))
            .then(a.offset.cmp(&b.offset)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a
            .block
            .raw()
            .cmp(&b.block.raw())
            .then(a.run.cmp(&b.run))
            .then(a.offset.cmp(&b.offset)),
    }
}

fn block_index(doc: &EditorDoc, id: BlockId) -> Option<usize> {
    doc.blocks.iter().position(|b| b.id == id)
}

/// What part of `doc_sel` falls inside `block` — used by per-block widgets
/// to decide what to paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockSelection {
    /// This block is outside the selection; paint nothing extra.
    None,
    /// This block contains a caret/range bounded by `local`. The block also
    /// holds `head` iff `holds_head` is true (caret painted only then).
    Local {
        local: LocalSelection,
        holds_head: bool,
    },
    /// The selection enters this block from the start and exits at `end`.
    /// `holds_head` indicates whether the head endpoint is the one inside.
    Leading { end: Caret, holds_head: bool },
    /// The selection enters at `start` and continues out the end of the
    /// block. `holds_head` indicates whether the head endpoint is the one
    /// inside this block.
    Trailing { start: Caret, holds_head: bool },
    /// Block sits strictly between anchor and head; entirely selected, no
    /// caret here.
    Full,
}

/// Project `doc_sel` into the slice that lives inside `block` (in `doc`).
pub fn project(doc_sel: DocSelection, block: &EditorBlock, doc: &EditorDoc) -> BlockSelection {
    let (start, end) = doc_sel.ordered(doc);
    let here = block.id;
    let here_idx = match block_index(doc, here) {
        Some(i) => i,
        None => return BlockSelection::None,
    };
    let start_idx = block_index(doc, start.block);
    let end_idx = block_index(doc, end.block);

    let head_here = doc_sel.head.block == here;

    match (start_idx, end_idx) {
        (Some(si), Some(ei)) if si == ei && si == here_idx => {
            // Both endpoints in this block.
            let local = LocalSelection {
                anchor: caret_at(start),
                head: caret_at(end),
            };
            // Restore the user's anchor/head ordering (start == ordered min).
            let local = if compare_positions(doc_sel.anchor, doc_sel.head, doc).is_le() {
                local
            } else {
                LocalSelection {
                    anchor: local.head,
                    head: local.anchor,
                }
            };
            BlockSelection::Local {
                local,
                holds_head: head_here,
            }
        }
        (Some(si), Some(_ei)) if here_idx == si => BlockSelection::Trailing {
            start: caret_at(start),
            holds_head: head_here,
        },
        (Some(_si), Some(ei)) if here_idx == ei => BlockSelection::Leading {
            end: caret_at(end),
            holds_head: head_here,
        },
        (Some(si), Some(ei)) if here_idx > si && here_idx < ei => BlockSelection::Full,
        _ => BlockSelection::None,
    }
}

fn caret_at(p: DocPosition) -> Caret {
    Caret {
        run: p.run,
        offset: p.offset,
    }
}

/// Per-block geometry cache for cross-block vertical-arrow navigation.
/// Each entry is `(BlockId, Vec<f32>)` — char-x positions in the block's
/// local coordinate system (excluding line-wrap; this is a first-cut that
/// treats each block as a single line for navigation purposes).
#[derive(Default, Debug, Clone)]
pub struct GeometryCache {
    map: HashMap<BlockId, Vec<f32>>,
}

impl GeometryCache {
    pub fn put(&mut self, id: BlockId, xs: Vec<f32>) {
        self.map.insert(id, xs);
    }

    pub fn get(&self, id: BlockId) -> Option<&[f32]> {
        self.map.get(&id).map(|v| v.as_slice())
    }

    /// Find the offset whose cached x is closest to `target_x`. Returns
    /// `None` if no entry exists for `id`.
    pub fn nearest_offset(&self, id: BlockId, target_x: f32) -> Option<usize> {
        let xs = self.get(id)?;
        let Some(&first) = xs.first() else {
            return Some(0);
        };
        let mut best_i = 0;
        let mut best_d = (first - target_x).abs();
        for (i, x) in xs.iter().enumerate().skip(1) {
            let d = (*x - target_x).abs();
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        Some(best_i)
    }

    /// Return the cached x for `(id, offset)`, clamped to the available
    /// entries. Returns `None` when no cache entry exists for `id`.
    pub fn x_at(&self, id: BlockId, offset: usize) -> Option<f32> {
        let xs = self.get(id)?;
        if xs.is_empty() {
            return Some(0.0);
        }
        let i = offset.min(xs.len() - 1);
        xs.get(i).copied()
    }

    /// Synthesize an x-position table by approximating each character as
    /// `font_size * APPROX_CHAR_RATIO` wide. Used until per-glyph layout
    /// hooks are wired (see selection module docs in editor_pane.rs).
    pub fn approximate_for(runs_text: &str, font_size: f32) -> Vec<f32> {
        let n = runs_text.chars().count();
        let w = font_size * APPROX_CHAR_RATIO;
        (0..=n)
            .map(|i| f32::from(u16::try_from(i).unwrap_or(u16::MAX)) * w)
            .collect()
    }
}

/// Width approximation per character (Latin-only fallback). 0.55 of the font
/// size sits between proportional widths for most sans/serif faces and is
/// good enough that vertical-arrow navigation lands on the right offset
/// within ±1 char for unstyled paragraph text.
pub const APPROX_CHAR_RATIO: f32 = 0.55;

/// Helpers that operate on `EditorDoc` given a `DocPosition`.
impl DocPosition {
    /// Doc-order successor of this position one character to the right,
    /// crossing block boundaries (for non-Inline blocks, hops to start of
    /// next block).
    pub fn step_right(self, doc: &EditorDoc) -> Self {
        let Some(idx) = block_index(doc, self.block) else {
            return self;
        };
        let Some(block) = doc.blocks.get(idx) else {
            return self;
        };
        if let BlockBody::Inline(runs) = &block.body {
            let Caret { run, offset } = step_caret_right(
                runs,
                Caret {
                    run: self.run,
                    offset: self.offset,
                },
            );
            // If we didn't advance, hop to the next block's start.
            if run == self.run && offset == self.offset {
                if let Some(next) = doc.blocks.get(idx + 1) {
                    return DocPosition::block_start(next.id);
                }
                return self;
            }
            return DocPosition::new(self.block, run, offset);
        }
        // Non-inline: jump straight to next block.
        if let Some(next) = doc.blocks.get(idx + 1) {
            return DocPosition::block_start(next.id);
        }
        self
    }

    /// Doc-order predecessor one character to the left, crossing blocks.
    pub fn step_left(self, doc: &EditorDoc) -> Self {
        let Some(idx) = block_index(doc, self.block) else {
            return self;
        };
        let Some(block) = doc.blocks.get(idx) else {
            return self;
        };
        if let BlockBody::Inline(runs) = &block.body {
            if self.run > 0 || self.offset > 0 {
                let Caret { run, offset } = step_caret_left(
                    runs,
                    Caret {
                        run: self.run,
                        offset: self.offset,
                    },
                );
                return DocPosition::new(self.block, run, offset);
            }
        }
        // At block start: hop to end of previous block.
        let Some(prev) = idx.checked_sub(1).and_then(|i| doc.blocks.get(i)) else {
            return self;
        };
        if let BlockBody::Inline(runs) = &prev.body {
            let end = Caret::end(runs);
            return DocPosition::new(prev.id, end.run, end.offset);
        }
        DocPosition::block_start(prev.id)
    }
}

fn step_caret_right(runs: &[crate::model::types::InlineRun], c: Caret) -> Caret {
    let Some(r) = runs.get(c.run) else { return c };
    let len = r.text.chars().count();
    if c.offset < len {
        Caret {
            run: c.run,
            offset: c.offset + 1,
        }
    } else if c.run + 1 < runs.len() {
        Caret {
            run: c.run + 1,
            offset: 0,
        }
    } else {
        c
    }
}

fn step_caret_left(runs: &[crate::model::types::InlineRun], c: Caret) -> Caret {
    if c.offset > 0 {
        return Caret {
            run: c.run,
            offset: c.offset - 1,
        };
    }
    if c.run > 0 {
        let prev_len = runs
            .get(c.run - 1)
            .map(|r| r.text.chars().count())
            .unwrap_or(0);
        return Caret {
            run: c.run - 1,
            offset: prev_len,
        };
    }
    c
}

/// Return the position at the very end of `doc` (used for Cmd-A's `head`).
pub fn doc_end_position(doc: &EditorDoc) -> DocPosition {
    let Some(last) = doc.blocks.last() else {
        // Empty doc: no real position; synthesize.
        return DocPosition::new(BlockId::new(), 0, 0);
    };
    if let BlockBody::Inline(runs) = &last.body {
        let end = Caret::end(runs);
        return DocPosition::new(last.id, end.run, end.offset);
    }
    DocPosition::block_start(last.id)
}

/// Return the position at the very start of `doc` (used for Cmd-A's `anchor`).
pub fn doc_start_position(doc: &EditorDoc) -> DocPosition {
    let Some(first) = doc.blocks.first() else {
        return DocPosition::new(BlockId::new(), 0, 0);
    };
    DocPosition::block_start(first.id)
}

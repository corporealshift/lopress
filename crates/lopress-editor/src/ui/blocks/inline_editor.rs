//! Editable inline-runs widget.
//!
//! The pure-data helpers in this module manipulate a `Vec<InlineRun>` and a
//! `Caret` (a `(run_index, char_offset)` pair). All edits go through these
//! helpers so they can be unit-tested independent of any UI framework. The
//! Floem widget itself lives below the helpers.

use crate::actions::BlockAction;
use crate::model::types::{BlockId, InlineRun};
use std::rc::Rc;

/// Callback used by editable widgets to push every block-tree mutation
/// through the `actions::apply` chokepoint owned by the editor pane.
pub type ActionSink = Rc<dyn Fn(BlockAction)>;

/// Pane-level slot that the focused editable widget publishes to so the
/// toolbar can reach the focused block's runs. The selection itself lives
/// in the pane-owned `SelectionContext::doc_selection`; the toolbar reads
/// from there and projects onto the focused block.
#[derive(Clone, Copy)]
pub struct FocusPublisher {
    pub block: RwSignal<Option<BlockId>>,
    pub runs: RwSignal<Option<RwSignal<Vec<InlineRun>>>>,
}

/// Position within a `Vec<InlineRun>`. Offsets are *character* counts within
/// `run.text`, not byte indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    pub run: usize,
    pub offset: usize,
}

impl Caret {
    pub const START: Self = Caret { run: 0, offset: 0 };

    pub fn end(runs: &[InlineRun]) -> Self {
        match runs.last() {
            Some(last) => Caret {
                run: runs.len() - 1,
                offset: last.text.chars().count(),
            },
            None => Caret::START,
        }
    }
}

/// Insert one character at `caret`. Returns the caret advanced by one.
pub fn insert_char(runs: &mut Vec<InlineRun>, caret: Caret, ch: char) -> Caret {
    if runs.is_empty() {
        runs.push(InlineRun::plain(ch.to_string()));
        return Caret { run: 0, offset: 1 };
    }
    let Some(run) = runs.get_mut(caret.run) else {
        return caret;
    };
    let byte = char_to_byte(&run.text, caret.offset);
    run.text.insert(byte, ch);
    Caret {
        run: caret.run,
        offset: caret.offset + 1,
    }
}

/// Delete the char immediately before `caret`. At block start: no-op.
/// At a run boundary: hops to the end of the previous run, then deletes.
pub fn backspace(runs: &mut Vec<InlineRun>, caret: Caret) -> Caret {
    if caret.run == 0 && caret.offset == 0 {
        return caret;
    }
    let mut c = caret;
    if c.offset == 0 {
        if c.run == 0 {
            return caret;
        }
        c.run -= 1;
        c.offset = runs.get(c.run).map(|r| r.text.chars().count()).unwrap_or(0);
    }
    let Some(run) = runs.get_mut(c.run) else {
        return caret;
    };
    let byte_end = char_to_byte(&run.text, c.offset);
    let byte_start = char_to_byte(&run.text, c.offset - 1);
    run.text.replace_range(byte_start..byte_end, "");
    let new_caret = Caret {
        run: c.run,
        offset: c.offset - 1,
    };
    coalesce_around(runs, new_caret.run);
    new_caret
}

/// Delete the char immediately after `caret`. Caret unchanged. At end of a
/// run hops into the next run.
pub fn delete(runs: &mut Vec<InlineRun>, caret: Caret) -> Caret {
    let Some(run) = runs.get(caret.run) else {
        return caret;
    };
    let len = run.text.chars().count();
    if caret.offset >= len {
        if caret.run + 1 >= runs.len() {
            return caret;
        }
        let Some(next) = runs.get_mut(caret.run + 1) else {
            return caret;
        };
        if next.text.is_empty() {
            runs.remove(caret.run + 1);
            return caret;
        }
        let byte_end = char_to_byte(&next.text, 1);
        next.text.replace_range(0..byte_end, "");
        coalesce_around(runs, caret.run);
        return caret;
    }
    let Some(run) = runs.get_mut(caret.run) else {
        return caret;
    };
    let byte_start = char_to_byte(&run.text, caret.offset);
    let byte_end = char_to_byte(&run.text, caret.offset + 1);
    run.text.replace_range(byte_start..byte_end, "");
    coalesce_around(runs, caret.run);
    caret
}

/// Move caret one character left, crossing run boundaries.
pub fn move_left(runs: &[InlineRun], caret: Caret) -> Caret {
    if caret.offset > 0 {
        return Caret {
            run: caret.run,
            offset: caret.offset - 1,
        };
    }
    if caret.run > 0 {
        let prev_idx = caret.run - 1;
        let prev_len = runs.get(prev_idx).map(|r| r.text.chars().count()).unwrap_or(0);
        return Caret {
            run: prev_idx,
            offset: prev_len,
        };
    }
    caret
}

/// Move caret one character right, crossing run boundaries.
pub fn move_right(runs: &[InlineRun], caret: Caret) -> Caret {
    let Some(run) = runs.get(caret.run) else {
        return caret;
    };
    let len = run.text.chars().count();
    if caret.offset < len {
        return Caret {
            run: caret.run,
            offset: caret.offset + 1,
        };
    }
    if caret.run + 1 < runs.len() {
        return Caret {
            run: caret.run + 1,
            offset: 0,
        };
    }
    caret
}

/// True when `runs` contains no characters — either an empty vector or only
/// runs whose `text` is empty. Used by the slash-menu trigger.
pub fn block_is_empty(runs: &[InlineRun]) -> bool {
    runs.iter().all(|r| r.text.is_empty())
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Merge `runs[idx]` with each neighbor when their styles match.
fn coalesce_around(runs: &mut Vec<InlineRun>, idx: usize) {
    if idx + 1 < runs.len() {
        let same = match (runs.get(idx), runs.get(idx + 1)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if same {
            let next = runs.remove(idx + 1);
            if let Some(cur) = runs.get_mut(idx) {
                cur.text.push_str(&next.text);
            }
        }
    }
    if idx > 0 {
        let same = match (runs.get(idx - 1), runs.get(idx)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if same {
            let cur = runs.remove(idx);
            if let Some(prev) = runs.get_mut(idx - 1) {
                prev.text.push_str(&cur.text);
            }
        }
    }
}

fn same_style(a: &InlineRun, b: &InlineRun) -> bool {
    a.bold == b.bold && a.italic == b.italic && a.code == b.code && a.link == b.link
}

// ── Selection ────────────────────────────────────────────────────────────────

/// Single-block selection. A collapsed selection (`anchor == head`) is the
/// caret. The `head` is what moves when the user extends with Shift+arrow;
/// `anchor` stays put until the next non-extending motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSelection {
    pub anchor: Caret,
    pub head: Caret,
}

impl LocalSelection {
    pub const START: Self = LocalSelection {
        anchor: Caret::START,
        head: Caret::START,
    };

    pub fn caret(c: Caret) -> Self {
        Self { anchor: c, head: c }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// `(min, max)` in document order.
    pub fn ordered(&self) -> (Caret, Caret) {
        if compare(self.anchor, self.head).is_le() {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// Lexicographic order on `(run, offset)`.
pub fn compare(a: Caret, b: Caret) -> std::cmp::Ordering {
    a.run.cmp(&b.run).then(a.offset.cmp(&b.offset))
}

/// Delete the range covered by `sel`, returning a collapsed selection at the
/// start of the deleted range. Coalesces neighbour runs when their styles
/// match.
pub fn delete_selection(runs: &mut Vec<InlineRun>, sel: LocalSelection) -> LocalSelection {
    if sel.is_collapsed() {
        return sel;
    }
    let (start, end) = sel.ordered();
    if start.run == end.run {
        if let Some(run) = runs.get_mut(start.run) {
            let byte_start = char_to_byte(&run.text, start.offset);
            let byte_end = char_to_byte(&run.text, end.offset);
            run.text.replace_range(byte_start..byte_end, "");
        }
    } else {
        // Tail of the start run.
        if let Some(run) = runs.get_mut(start.run) {
            let byte = char_to_byte(&run.text, start.offset);
            run.text.truncate(byte);
        }
        // Head of the end run.
        if let Some(run) = runs.get_mut(end.run) {
            let byte = char_to_byte(&run.text, end.offset);
            run.text.replace_range(0..byte, "");
        }
        // Drain runs strictly between them.
        if end.run > start.run + 1 {
            runs.drain(start.run + 1..end.run);
        }
    }

    // Drop empty runs that the deletion left behind, but keep at least one
    // run (so subsequent edits have somewhere to go).
    let mut i = 0;
    while i < runs.len() {
        if runs[i].text.is_empty() && runs.len() > 1 {
            runs.remove(i);
        } else {
            i += 1;
        }
    }
    coalesce_around(runs, start.run.min(runs.len().saturating_sub(1)));

    LocalSelection::caret(start)
}

/// Inline-style flags toggled by the Bold/Italic/Code/Link shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFlag {
    Bold,
    Italic,
    Code,
    Link,
}

/// Toggle the given inline flag across the selection. Toggle direction:
/// clear if every run inside the selection has the flag set, otherwise set.
/// Collapsed selections are a no-op (cursor-style toggling is a follow-up).
pub fn toggle_inline(
    runs: &mut Vec<InlineRun>,
    sel: LocalSelection,
    flag: InlineFlag,
) -> LocalSelection {
    if sel.is_collapsed() {
        return sel;
    }
    // Translate the selection bounds to absolute character offsets from the
    // start of the block. Run indices are about to shift as we split, so we
    // can't keep using the original `Caret`s.
    let (start, end) = sel.ordered();
    let abs_start = abs_offset(runs, start);
    let abs_end = abs_offset(runs, end);

    // Split the higher position first; the second split doesn't disturb the
    // first's absolute offset (it only inserts a run elsewhere).
    split_at_abs(runs, abs_end);
    split_at_abs(runs, abs_start);

    let start_idx = locate_abs(runs, abs_start);
    let end_idx = locate_abs(runs, abs_end);

    let all_set = (start_idx..end_idx).all(|i| {
        runs.get(i)
            .map(|r| match flag {
                InlineFlag::Bold => r.bold,
                InlineFlag::Italic => r.italic,
                InlineFlag::Code => r.code,
                InlineFlag::Link => r.link.is_some(),
            })
            .unwrap_or(false)
    });
    let new_value = !all_set;

    for i in start_idx..end_idx {
        if let Some(r) = runs.get_mut(i) {
            match flag {
                InlineFlag::Bold => r.bold = new_value,
                InlineFlag::Italic => r.italic = new_value,
                InlineFlag::Code => r.code = new_value,
                InlineFlag::Link => {
                    r.link = if new_value { Some(String::new()) } else { None };
                }
            }
        }
    }

    let lo = start_idx.saturating_sub(1);
    let hi = end_idx.min(runs.len());
    coalesce_range(runs, lo, hi);

    sel
}

/// Sum of `chars().count()` over runs `[0..c.run]`, plus `c.offset`.
fn abs_offset(runs: &[InlineRun], c: Caret) -> usize {
    let mut acc = 0;
    for (i, r) in runs.iter().enumerate() {
        if i == c.run {
            return acc + c.offset;
        }
        acc += r.text.chars().count();
    }
    acc
}

/// Split whichever run contains absolute character offset `abs`. No-op if
/// `abs` lands on a run boundary or past end-of-block.
fn split_at_abs(runs: &mut Vec<InlineRun>, abs: usize) {
    let mut acc = 0;
    let mut i = 0;
    while i < runs.len() {
        let len = runs[i].text.chars().count();
        if abs > acc && abs < acc + len {
            let local_off = abs - acc;
            let byte = char_to_byte(&runs[i].text, local_off);
            let (left, right) = runs[i].text.split_at(byte);
            let left = left.to_string();
            let right = right.to_string();
            let mut left_run = runs[i].clone();
            left_run.text = left;
            let mut right_run = runs[i].clone();
            right_run.text = right;
            runs[i] = left_run;
            runs.insert(i + 1, right_run);
            return;
        }
        acc += len;
        i += 1;
    }
}

/// After splits, `abs` aligns with the start of some run; return that run's
/// index. Returns `runs.len()` for end-of-block.
fn locate_abs(runs: &[InlineRun], abs: usize) -> usize {
    let mut acc = 0;
    for (i, r) in runs.iter().enumerate() {
        if acc == abs {
            return i;
        }
        acc += r.text.chars().count();
    }
    runs.len()
}

/// Coalesce neighbour runs with matching styles in the inclusive range
/// `[lo, hi]` (clamped to the runs vector).
fn coalesce_range(runs: &mut Vec<InlineRun>, lo: usize, hi: usize) {
    let mut i = lo;
    while i + 1 < runs.len() && i <= hi {
        let merge = match (runs.get(i), runs.get(i + 1)) {
            (Some(a), Some(b)) => same_style(a, b),
            _ => false,
        };
        if merge {
            let next = runs.remove(i + 1);
            if let Some(cur) = runs.get_mut(i) {
                cur.text.push_str(&next.text);
            }
        } else {
            i += 1;
        }
    }
}

// ── Floem widget ─────────────────────────────────────────────────────────────
//
// Per-block widget for an Inline-bodied block. Selection lives in the
// pane-owned `SelectionContext::doc_selection`; this widget reads its slice
// of the doc selection (via `project`) and writes back through the same
// signal. The runs themselves are still per-block so that local typing is
// cheap; on FocusLost (and on every BlockAction) the latest local runs are
// committed back to the document via `BlockAction::EditInline`.

use crate::selection::{
    doc_end_position, doc_start_position, project, BlockSelection, DocPosition,
    DocSelection, GeometryCache,
};
use crate::ui::blocks::paragraph::{LINK_COLOR, MONO_FAMILY};
use crate::ui::sel_ctx::SelectionContext;
use floem::event::{Event, EventListener, EventPropagation};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::style::FlexWrap;
use floem::text::Weight;
use floem::views::{empty, h_stack_from_iter, text, Decorators};
use floem::{AnyView, IntoView, View};

const CARET_COLOR: Color = Color::rgb8(40, 40, 40);
const SELECTION_BG: Color = Color::rgb8(180, 210, 255);

/// Build the editable inline-runs widget.
pub fn editable_inline(
    runs: RwSignal<Vec<InlineRun>>,
    font_size: f32,
    force_bold: bool,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    slash_eligible: bool,
    sel_ctx: SelectionContext,
) -> impl IntoView {
    let focused: RwSignal<bool> = RwSignal::new(false);

    // Maintain this block's geometry cache entry: rebuild whenever runs
    // change. Approximate widths are good enough for visually-correct
    // vertical-arrow nav within ±1 char on unstyled paragraph text; a
    // future task can swap in real per-glyph positions from a custom
    // text-layout view.
    let geometry_for_effect = sel_ctx.geometry.clone();
    create_effect(move |_| {
        let runs_v = runs.get();
        let text: String = runs_v.iter().map(|r| r.text.clone()).collect();
        let xs = GeometryCache::approximate_for(&text, font_size);
        geometry_for_effect.borrow_mut().put(block_id, xs);
    });

    // Re-render on runs / doc_selection / focus changes. Project the doc
    // selection onto this block to derive what to paint locally.
    let doc_sel_sig = sel_ctx.doc_selection;
    let current_doc_sig = sel_ctx.current_doc;
    let body = floem::views::dyn_container(
        move || (runs.get(), doc_sel_sig.get(), focused.get()),
        move |(runs_v, doc_sel_v, foc)| {
            let bs = current_doc_sig.with_untracked(|maybe| {
                maybe
                    .as_ref()
                    .and_then(|d| {
                        d.blocks
                            .iter()
                            .find(|b| b.id == block_id)
                            .map(|b| project(doc_sel_v, b, d))
                    })
                    .unwrap_or(BlockSelection::None)
            });
            render_block(&runs_v, bs, foc, font_size, force_bold)
        },
    )
    .style(|s| s.width_full());

    let on_action_for_focus_lost = on_action.clone();
    let on_action_for_keydown = on_action;
    let sel_ctx_for_click = sel_ctx.clone();
    let sel_ctx_for_keydown = sel_ctx.clone();
    let view = body
        .keyboard_navigable()
        .on_click_stop(move |_| {
            // Click anywhere → collapse caret at end-of-block. Real per-
            // character hit-testing remains a follow-up.
            let end = runs.with_untracked(|r| Caret::end(r));
            sel_ctx_for_click
                .doc_selection
                .set(DocSelection::caret(DocPosition::new(
                    block_id, end.run, end.offset,
                )));
        })
        .on_event(EventListener::FocusGained, move |_| {
            focused.set(true);
            focus_pub.block.set(Some(block_id));
            focus_pub.runs.set(Some(runs));
            EventPropagation::Stop
        })
        .on_event(EventListener::FocusLost, move |_| {
            focused.set(false);
            if focus_pub.block.get_untracked() == Some(block_id) {
                focus_pub.block.set(None);
                focus_pub.runs.set(None);
            }
            let current = runs.get_untracked();
            on_action_for_focus_lost(BlockAction::EditInline {
                block_id,
                new_runs: current,
            });
            EventPropagation::Stop
        })
        .on_event(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                if handle_key_down(
                    ke,
                    runs,
                    block_id,
                    &on_action_for_keydown,
                    slash_eligible,
                    &sel_ctx_for_keydown,
                    focus_target,
                ) {
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        })
        .style(move |s| {
            let s = s.padding_vert(2.).padding_horiz(2.).border_radius(2.);
            if focused.get() {
                s.background(Color::rgb8(245, 248, 255))
            } else {
                s
            }
        });

    let view_id = view.id();
    create_effect(move |_| {
        if focus_target.get() == Some(block_id) {
            view_id.request_focus();
            focus_target.set(None);
        }
    });

    view
}

/// Direction for horizontal motion (←/→/Home/End).
#[derive(Clone, Copy)]
enum HMotion {
    Left,
    Right,
    Home,
    End,
}

/// Direction for vertical motion (↑/↓).
#[derive(Clone, Copy)]
enum VMotion {
    Up,
    Down,
}

/// Decide what to do for a single `KeyEvent`. Returns `true` when handled.
fn handle_key_down(
    ke: &floem::keyboard::KeyEvent,
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    slash_eligible: bool,
    sel_ctx: &SelectionContext,
    focus_target: RwSignal<Option<BlockId>>,
) -> bool {
    let extending = ke.modifiers.shift();
    let cmd = ke.modifiers.control() || ke.modifiers.meta();

    // Cmd/Ctrl-modified shortcuts handled first.
    if cmd {
        if let Key::Character(s) = &ke.key.logical_key {
            match s.as_str() {
                "a" | "A" => {
                    do_select_all(runs, block_id, on_action, sel_ctx);
                    return true;
                }
                "c" | "C" => {
                    do_copy(runs, block_id, on_action, sel_ctx);
                    return true;
                }
                "x" | "X" => {
                    do_cut(runs, block_id, on_action, sel_ctx);
                    return true;
                }
                "v" | "V" => {
                    do_paste(runs, block_id, on_action, sel_ctx, focus_target);
                    return true;
                }
                _ => {}
            }
        }
    }

    match ke.key.logical_key.clone() {
        Key::Named(NamedKey::ArrowLeft) => {
            do_horizontal(runs, block_id, extending, HMotion::Left, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            do_horizontal(runs, block_id, extending, HMotion::Right, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::Home) => {
            do_horizontal(runs, block_id, extending, HMotion::Home, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::End) => {
            do_horizontal(runs, block_id, extending, HMotion::End, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::ArrowUp) => {
            do_vertical(runs, block_id, extending, VMotion::Up, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::ArrowDown) => {
            do_vertical(runs, block_id, extending, VMotion::Down, sel_ctx, on_action, focus_target);
            true
        }
        Key::Named(NamedKey::Backspace) => {
            do_backspace(runs, block_id, on_action, sel_ctx);
            true
        }
        Key::Named(NamedKey::Delete) => {
            do_delete(runs, block_id, on_action, sel_ctx);
            true
        }
        Key::Named(NamedKey::Space) => {
            insert_at_doc_caret(runs, block_id, sel_ctx, ' ');
            true
        }
        Key::Named(NamedKey::Enter) => {
            if ke.modifiers.shift() {
                return true;
            }
            // Resolve head's local caret in this block, then split.
            let head = sel_ctx.doc_selection.get_untracked().head;
            if head.block != block_id {
                return false;
            }
            let current = runs.get_untracked();
            on_action(BlockAction::EditInline {
                block_id,
                new_runs: current,
            });
            on_action(BlockAction::Split {
                block_id,
                run: head.run,
                offset: head.offset,
            });
            true
        }
        _ => {
            if cmd {
                if let Some(flag) = inline_flag_for_shortcut(ke) {
                    apply_local_toggle(runs, block_id, sel_ctx, flag, on_action);
                    return true;
                }
                return false;
            }
            let Some(text) = ke.key.text.as_ref() else {
                return false;
            };
            // If the doc selection spans multiple blocks, replace it first
            // so the subsequent char insert lands at a clean caret.
            let doc_sel = sel_ctx.doc_selection.get_untracked();
            let multi_block = doc_sel.anchor.block != doc_sel.head.block;
            if multi_block {
                commit_runs(runs, block_id, on_action);
                delete_range_and_collapse(on_action, sel_ctx);
            }
            for ch in text.chars() {
                if ch.is_control() {
                    continue;
                }
                if ch == '/'
                    && slash_eligible
                    && runs.with_untracked(|r| block_is_empty(r))
                {
                    on_action(BlockAction::OpenSlashMenu { block_id });
                    continue;
                }
                // After a multi-block delete, doc_selection now points at
                // the leading block; the focused widget for the next render
                // will pick this up. For the immediate insert call we still
                // need to address whichever block holds the new caret —
                // route through the action sink for the cross-block case.
                let head_block = sel_ctx.doc_selection.get_untracked().head.block;
                if head_block == block_id {
                    insert_at_doc_caret(runs, block_id, sel_ctx, ch);
                } else {
                    // Insert via EditInline on the new leading block (we
                    // can't reach its local runs signal from here).
                    insert_into_doc(sel_ctx, on_action, ch);
                }
            }
            true
        }
    }
}

/// Map a Ctrl/Cmd-modified key event to the inline flag it toggles.
fn inline_flag_for_shortcut(ke: &floem::keyboard::KeyEvent) -> Option<InlineFlag> {
    if let Key::Character(s) = &ke.key.logical_key {
        match s.as_str() {
            "b" | "B" => Some(InlineFlag::Bold),
            "i" | "I" => Some(InlineFlag::Italic),
            "e" | "E" => Some(InlineFlag::Code),
            "k" | "K" => Some(InlineFlag::Link),
            _ => None,
        }
    } else {
        None
    }
}

/// Apply a B/I/code/link toggle. Single-block selections operate on the
/// local runs in-place; cross-block selections route to
/// `BlockAction::ToggleInlineRange`.
fn apply_local_toggle(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    sel_ctx: &SelectionContext,
    flag: InlineFlag,
    on_action: &ActionSink,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    if doc_sel.anchor.block != block_id || doc_sel.head.block != block_id {
        if doc_sel.is_collapsed() {
            return;
        }
        commit_runs(runs, block_id, on_action);
        on_action(BlockAction::ToggleInlineRange {
            selection: doc_sel,
            flag,
        });
        return;
    }
    let local = LocalSelection {
        anchor: Caret { run: doc_sel.anchor.run, offset: doc_sel.anchor.offset },
        head: Caret { run: doc_sel.head.run, offset: doc_sel.head.offset },
    };
    let mut new_local = local;
    runs.update(|r| {
        new_local = toggle_inline(r, local, flag);
    });
    sel_ctx.doc_selection.set(DocSelection {
        anchor: DocPosition::new(block_id, new_local.anchor.run, new_local.anchor.offset),
        head: DocPosition::new(block_id, new_local.head.run, new_local.head.offset),
    });
}

/// Insert one character at the doc caret. For multi-block selections the
/// caller has already routed through `delete_range_and_collapse`, so by
/// this point the doc selection is collapsed inside the leading block.
fn insert_at_doc_caret(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    sel_ctx: &SelectionContext,
    ch: char,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    if doc_sel.head.block != block_id {
        return;
    }
    // For now only handle within-block selections; multi-block delete-then-
    // insert is a Task-16 concern.
    let local = LocalSelection {
        anchor: Caret {
            run: doc_sel.anchor.run,
            offset: doc_sel.anchor.offset,
        },
        head: Caret {
            run: doc_sel.head.run,
            offset: doc_sel.head.offset,
        },
    };
    let single_block = doc_sel.anchor.block == block_id;
    let mut new_caret = local.head;
    runs.update(|r| {
        let collapsed = if single_block && !local.is_collapsed() {
            delete_selection(r, local)
        } else {
            LocalSelection::caret(local.head)
        };
        new_caret = insert_char(r, collapsed.head, ch);
    });
    sel_ctx
        .doc_selection
        .set(DocSelection::caret(DocPosition::new(
            block_id,
            new_caret.run,
            new_caret.offset,
        )));
}

/// Backspace at the doc caret. Block-start backspace merges with the
/// previous block (existing behavior). Within-block proceeds via the local
/// helpers. Multi-block selection routes to `BlockAction::DeleteRange`.
fn do_backspace(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    let single_block = doc_sel.anchor.block == block_id && doc_sel.head.block == block_id;
    if !doc_sel.is_collapsed() && !single_block {
        commit_runs(runs, block_id, on_action);
        delete_range_and_collapse(on_action, sel_ctx);
        return;
    }
    if doc_sel.head.block != block_id {
        return;
    }
    let local = LocalSelection {
        anchor: Caret { run: doc_sel.anchor.run, offset: doc_sel.anchor.offset },
        head: Caret { run: doc_sel.head.run, offset: doc_sel.head.offset },
    };
    if single_block && local.is_collapsed() && local.head == Caret::START {
        // Merge with previous block.
        let current = runs.get_untracked();
        on_action(BlockAction::EditInline {
            block_id,
            new_runs: current,
        });
        on_action(BlockAction::MergeWithPrev { block_id });
        return;
    }
    if single_block && !local.is_collapsed() {
        let mut new_local = local;
        runs.update(|r| {
            new_local = delete_selection(r, local);
        });
        sel_ctx.doc_selection.set(DocSelection {
            anchor: DocPosition::new(block_id, new_local.anchor.run, new_local.anchor.offset),
            head: DocPosition::new(block_id, new_local.head.run, new_local.head.offset),
        });
        return;
    }
    // Single-caret backspace within this block.
    let mut new_caret = local.head;
    runs.update(|r| {
        new_caret = backspace(r, new_caret);
    });
    sel_ctx
        .doc_selection
        .set(DocSelection::caret(DocPosition::new(
            block_id,
            new_caret.run,
            new_caret.offset,
        )));
}

/// Delete-key handling (delete forward). Multi-block selection routes to
/// `BlockAction::DeleteRange`.
fn do_delete(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    let single_block = doc_sel.anchor.block == block_id && doc_sel.head.block == block_id;
    if !doc_sel.is_collapsed() && !single_block {
        commit_runs(runs, block_id, on_action);
        delete_range_and_collapse(on_action, sel_ctx);
        return;
    }
    if doc_sel.head.block != block_id {
        return;
    }
    let local = LocalSelection {
        anchor: Caret { run: doc_sel.anchor.run, offset: doc_sel.anchor.offset },
        head: Caret { run: doc_sel.head.run, offset: doc_sel.head.offset },
    };
    if single_block && !local.is_collapsed() {
        let mut new_local = local;
        runs.update(|r| {
            new_local = delete_selection(r, local);
        });
        sel_ctx.doc_selection.set(DocSelection {
            anchor: DocPosition::new(block_id, new_local.anchor.run, new_local.anchor.offset),
            head: DocPosition::new(block_id, new_local.head.run, new_local.head.offset),
        });
        return;
    }
    let mut new_caret = local.head;
    runs.update(|r| {
        new_caret = delete(r, new_caret);
    });
    sel_ctx
        .doc_selection
        .set(DocSelection::caret(DocPosition::new(
            block_id,
            new_caret.run,
            new_caret.offset,
        )));
}

/// Cmd/Ctrl-A: select the whole document.
fn do_select_all(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
) {
    // Commit local runs first so doc_end_position sees latest text.
    let current = runs.get_untracked();
    on_action(BlockAction::EditInline {
        block_id,
        new_runs: current,
    });
    sel_ctx.current_doc.with_untracked(|maybe| {
        if let Some(d) = maybe {
            let anchor = doc_start_position(d);
            let head = doc_end_position(d);
            sel_ctx.doc_selection.set(DocSelection { anchor, head });
        }
    });
}

/// Horizontal motion. Prefers local runs for within-block movement to honor
/// uncommitted edits; commits and consults the doc for cross-block hops.
fn do_horizontal(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    extending: bool,
    direction: HMotion,
    sel_ctx: &SelectionContext,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    let head = doc_sel.head;

    // Non-extending arrow on a non-collapsed selection collapses to the
    // corresponding end without further motion.
    if !extending
        && !doc_sel.is_collapsed()
        && matches!(direction, HMotion::Left | HMotion::Right)
    {
        let target = sel_ctx.current_doc.with_untracked(|maybe| {
            let d = maybe.as_ref()?;
            let (lo, hi) = doc_sel.ordered(d);
            Some(match direction {
                HMotion::Left => lo,
                HMotion::Right => hi,
                _ => unreachable!(),
            })
        });
        if let Some(t) = target {
            sel_ctx.doc_selection.set(DocSelection::caret(t));
            if t.block != block_id {
                commit_runs(runs, block_id, on_action);
                focus_target.set(Some(t.block));
            }
        }
        return;
    }

    let new_head = match direction {
        HMotion::Left => {
            if head.block == block_id {
                let runs_v = runs.get_untracked();
                let c = move_left(&runs_v, Caret { run: head.run, offset: head.offset });
                if c.run != head.run || c.offset != head.offset {
                    DocPosition::new(block_id, c.run, c.offset)
                } else {
                    commit_runs(runs, block_id, on_action);
                    sel_ctx
                        .current_doc
                        .with_untracked(|maybe| {
                            maybe.as_ref().map(|d| head.step_left(d)).unwrap_or(head)
                        })
                }
            } else {
                sel_ctx
                    .current_doc
                    .with_untracked(|maybe| {
                        maybe.as_ref().map(|d| head.step_left(d)).unwrap_or(head)
                    })
            }
        }
        HMotion::Right => {
            if head.block == block_id {
                let runs_v = runs.get_untracked();
                let c = move_right(&runs_v, Caret { run: head.run, offset: head.offset });
                if c.run != head.run || c.offset != head.offset {
                    DocPosition::new(block_id, c.run, c.offset)
                } else {
                    commit_runs(runs, block_id, on_action);
                    sel_ctx
                        .current_doc
                        .with_untracked(|maybe| {
                            maybe.as_ref().map(|d| head.step_right(d)).unwrap_or(head)
                        })
                }
            } else {
                sel_ctx
                    .current_doc
                    .with_untracked(|maybe| {
                        maybe.as_ref().map(|d| head.step_right(d)).unwrap_or(head)
                    })
            }
        }
        HMotion::Home => DocPosition::block_start(head.block),
        HMotion::End => {
            if head.block == block_id {
                let end = runs.with_untracked(|r| Caret::end(r));
                DocPosition::new(block_id, end.run, end.offset)
            } else {
                sel_ctx
                    .current_doc
                    .with_untracked(|maybe| end_of_block(maybe.as_ref(), head.block).unwrap_or(head))
            }
        }
    };

    let new_sel = if extending {
        DocSelection {
            anchor: doc_sel.anchor,
            head: new_head,
        }
    } else {
        DocSelection::caret(new_head)
    };
    sel_ctx.doc_selection.set(new_sel);

    if !extending && new_head.block != block_id {
        commit_runs(runs, block_id, on_action);
        focus_target.set(Some(new_head.block));
    }
}

/// Vertical motion across blocks. Reads the source block's x at `head.offset`
/// from the geometry cache; finds the offset whose cached x is nearest in
/// the target block.
fn do_vertical(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    extending: bool,
    direction: VMotion,
    sel_ctx: &SelectionContext,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    let head = doc_sel.head;

    // Source-block absolute char offset → x.
    let src_abs = sel_ctx.current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let b = d.blocks.iter().find(|b| b.id == head.block)?;
        if let crate::model::types::BlockBody::Inline(r) = &b.body {
            Some(abs_offset(r, Caret { run: head.run, offset: head.offset }))
        } else {
            None
        }
    });
    let x = src_abs
        .and_then(|a| sel_ctx.geometry.borrow().x_at(head.block, a))
        .unwrap_or(0.0);

    let target_block_id = sel_ctx.current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == head.block)?;
        match direction {
            VMotion::Up => {
                if i == 0 {
                    None
                } else {
                    Some(d.blocks[i - 1].id)
                }
            }
            VMotion::Down => d.blocks.get(i + 1).map(|b| b.id),
        }
    });

    let new_head = match target_block_id {
        Some(tid) => {
            let abs = sel_ctx
                .geometry
                .borrow()
                .nearest_offset(tid, x)
                .unwrap_or(0);
            sel_ctx.current_doc.with_untracked(|maybe| {
                let d = maybe
                    .as_ref()
                    .and_then(|d| d.blocks.iter().find(|b| b.id == tid));
                match d {
                    Some(b) => match &b.body {
                        crate::model::types::BlockBody::Inline(r) => {
                            let (run, offset) = abs_to_run_offset(r, abs);
                            DocPosition::new(tid, run, offset)
                        }
                        _ => DocPosition::block_start(tid),
                    },
                    None => DocPosition::block_start(tid),
                }
            })
        }
        None => sel_ctx.current_doc.with_untracked(|maybe| {
            maybe
                .as_ref()
                .map(|d| match direction {
                    VMotion::Up => doc_start_position(d),
                    VMotion::Down => doc_end_position(d),
                })
                .unwrap_or(head)
        }),
    };

    let new_sel = if extending {
        DocSelection {
            anchor: doc_sel.anchor,
            head: new_head,
        }
    } else {
        DocSelection::caret(new_head)
    };
    if new_head.block != block_id {
        commit_runs(runs, block_id, on_action);
    }
    sel_ctx.doc_selection.set(new_sel);
    if !extending && new_head.block != block_id {
        focus_target.set(Some(new_head.block));
    }
}

fn commit_runs(runs: RwSignal<Vec<InlineRun>>, block_id: BlockId, on_action: &ActionSink) {
    let current = runs.get_untracked();
    on_action(BlockAction::EditInline {
        block_id,
        new_runs: current,
    });
}

fn end_of_block(doc: Option<&crate::model::types::EditorDoc>, block_id: BlockId) -> Option<DocPosition> {
    let d = doc?;
    let b = d.blocks.iter().find(|b| b.id == block_id)?;
    if let crate::model::types::BlockBody::Inline(r) = &b.body {
        let e = Caret::end(r);
        Some(DocPosition::new(block_id, e.run, e.offset))
    } else {
        Some(DocPosition::block_start(block_id))
    }
}

fn abs_to_run_offset(runs: &[InlineRun], abs: usize) -> (usize, usize) {
    let mut acc = 0;
    for (i, r) in runs.iter().enumerate() {
        let len = r.text.chars().count();
        if abs <= acc + len {
            return (i, abs - acc);
        }
        acc += len;
    }
    let last = runs.len().saturating_sub(1);
    let last_len = runs.last().map(|r| r.text.chars().count()).unwrap_or(0);
    (last, last_len)
}

// ── Multi-block routing helpers ──────────────────────────────────────────────

/// Issue a `DeleteRange` action for the current selection and collapse the
/// doc selection to the start of the deleted range. Caller must already
/// have committed any local runs.
fn delete_range_and_collapse(on_action: &ActionSink, sel_ctx: &SelectionContext) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    let start = sel_ctx
        .current_doc
        .with_untracked(|maybe| maybe.as_ref().map(|d| doc_sel.ordered(d).0))
        .unwrap_or(doc_sel.anchor);
    on_action(BlockAction::DeleteRange { selection: doc_sel });
    sel_ctx.doc_selection.set(DocSelection::caret(start));
}

/// Insert a single character at the doc caret via the action sink — used
/// when the caret moved to a different block (after a multi-block delete)
/// and we don't have direct access to that block's runs signal.
fn insert_into_doc(sel_ctx: &SelectionContext, on_action: &ActionSink, ch: char) {
    let head = sel_ctx.doc_selection.get_untracked().head;
    sel_ctx.current_doc.with_untracked(|maybe| {
        let Some(d) = maybe.as_ref() else { return };
        let Some(block) = d.blocks.iter().find(|b| b.id == head.block) else {
            return;
        };
        let crate::model::types::BlockBody::Inline(runs) = &block.body else {
            return;
        };
        let mut new_runs = runs.clone();
        let new_caret = insert_char(
            &mut new_runs,
            Caret { run: head.run, offset: head.offset },
            ch,
        );
        on_action(BlockAction::EditInline {
            block_id: head.block,
            new_runs,
        });
        sel_ctx
            .doc_selection
            .set(DocSelection::caret(DocPosition::new(
                head.block,
                new_caret.run,
                new_caret.offset,
            )));
    });
}

fn do_copy(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    if doc_sel.is_collapsed() {
        return;
    }
    commit_runs(runs, block_id, on_action);
    sel_ctx.current_doc.with_untracked(|maybe| {
        let Some(d) = maybe.as_ref() else { return };
        let blocks = crate::ui::clipboard::extract_selection_blocks(d, doc_sel);
        let md = crate::ui::clipboard::blocks_to_markdown(&blocks);
        crate::ui::clipboard::write_clipboard(md);
    });
}

fn do_cut(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
) {
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    if doc_sel.is_collapsed() {
        return;
    }
    commit_runs(runs, block_id, on_action);
    sel_ctx.current_doc.with_untracked(|maybe| {
        let Some(d) = maybe.as_ref() else { return };
        let blocks = crate::ui::clipboard::extract_selection_blocks(d, doc_sel);
        let md = crate::ui::clipboard::blocks_to_markdown(&blocks);
        crate::ui::clipboard::write_clipboard(md);
    });
    delete_range_and_collapse(on_action, sel_ctx);
}

fn do_paste(
    runs: RwSignal<Vec<InlineRun>>,
    block_id: BlockId,
    on_action: &ActionSink,
    sel_ctx: &SelectionContext,
    focus_target: RwSignal<Option<BlockId>>,
) {
    let Some(text) = crate::ui::clipboard::read_clipboard() else {
        return;
    };
    let blocks = crate::ui::clipboard::markdown_to_blocks(&text);
    if blocks.is_empty() {
        return;
    }
    commit_runs(runs, block_id, on_action);
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    // Replace any existing selection first.
    if !doc_sel.is_collapsed() {
        delete_range_and_collapse(on_action, sel_ctx);
    }
    let head = sel_ctx.doc_selection.get_untracked().head;
    let last_block_id = blocks.last().map(|b| b.id);
    on_action(BlockAction::PasteBlocks {
        at: head,
        blocks,
    });
    // Focus the last pasted block so the caret lands at its end-ish.
    if let Some(id) = last_block_id {
        focus_target.set(Some(id));
        // Caret at end of that block (best we can do without knowing post-
        // paste runs; the focused widget will collapse to end on click but
        // we set doc_selection to a reasonable position).
        sel_ctx.current_doc.with_untracked(|maybe| {
            if let Some(d) = maybe.as_ref() {
                if let Some(b) = d.blocks.iter().find(|b| b.id == id) {
                    if let crate::model::types::BlockBody::Inline(r) = &b.body {
                        let e = Caret::end(r);
                        sel_ctx
                            .doc_selection
                            .set(DocSelection::caret(DocPosition::new(id, e.run, e.offset)));
                    }
                }
            }
        });
    }
}

/// Render the block given its slice of the doc selection.
///
/// `bs` carries which characters are part of the selection range and whether
/// this block holds the doc selection's `head` (which is what determines
/// whether to paint a caret here). `focused` controls whether we paint the
/// caret as a visible bar — it should only be true on the block that owns
/// Floem's keyboard focus, but selection painting happens regardless.
fn render_block(
    runs: &[InlineRun],
    bs: BlockSelection,
    focused: bool,
    font_size: f32,
    force_bold: bool,
) -> AnyView {
    // Collapse `bs` to a `(range, caret)` pair for the rendering loop.
    let (range, caret) = block_render_inputs(bs, runs);

    if runs.is_empty() {
        // An empty block can still hold the caret; paint it if focused.
        return if focused && caret.is_some() {
            caret_span(font_size).into_any()
        } else {
            empty().into_any()
        };
    }

    let mut elements: Vec<AnyView> = Vec::with_capacity(runs.len() + 4);
    for (i, run) in runs.iter().enumerate() {
        emit_run_segments(
            run,
            i,
            range,
            caret,
            focused,
            font_size,
            force_bold,
            &mut elements,
        );
    }

    h_stack_from_iter(elements)
        .style(|s| s.flex_wrap(FlexWrap::Wrap).width_full())
        .into_any()
}

/// Translate `BlockSelection` into an inclusive range `[lo, hi)` of carets
/// to paint as selection background, and an optional caret position.
fn block_render_inputs(
    bs: BlockSelection,
    runs: &[InlineRun],
) -> (Option<(Caret, Caret)>, Option<Caret>) {
    match bs {
        BlockSelection::None => (None, None),
        BlockSelection::Local { local, holds_head } => {
            let range = if local.is_collapsed() {
                None
            } else {
                Some(local.ordered())
            };
            let caret = if holds_head { Some(local.head) } else { None };
            (range, caret)
        }
        BlockSelection::Leading { end, holds_head } => {
            let range = if end == Caret::START {
                None
            } else {
                Some((Caret::START, end))
            };
            let caret = if holds_head { Some(end) } else { None };
            (range, caret)
        }
        BlockSelection::Trailing { start, holds_head } => {
            let block_end = Caret::end(runs);
            let range = if start == block_end {
                None
            } else {
                Some((start, block_end))
            };
            let caret = if holds_head { Some(start) } else { None };
            (range, caret)
        }
        BlockSelection::Full => {
            let block_end = Caret::end(runs);
            let range = if block_end == Caret::START {
                None
            } else {
                Some((Caret::START, block_end))
            };
            (range, None)
        }
    }
}

/// Slice one run into segments at selection boundaries (and at the caret
/// position when present) and append the corresponding styled spans to `out`.
fn emit_run_segments(
    run: &InlineRun,
    run_idx: usize,
    range: Option<(Caret, Caret)>,
    caret: Option<Caret>,
    focused: bool,
    font_size: f32,
    force_bold: bool,
    out: &mut Vec<AnyView>,
) {
    let chars: Vec<char> = run.text.chars().collect();
    let len = chars.len();

    let sel_lo: Option<usize> = range.map(|(s, _)| {
        if s.run < run_idx {
            0
        } else if s.run == run_idx {
            s.offset.min(len)
        } else {
            len + 1 // sentinel: range begins after this run
        }
    });
    let sel_hi: Option<usize> = range.map(|(_, e)| {
        if e.run > run_idx {
            len
        } else if e.run == run_idx {
            e.offset.min(len)
        } else {
            0 // sentinel: range ended before this run
        }
    });
    let sel_range: Option<(usize, usize)> = match (sel_lo, sel_hi) {
        (Some(a), Some(b)) if a < b && a <= len => Some((a, b)),
        _ => None,
    };

    let caret_off: Option<usize> = if focused {
        caret.and_then(|c| {
            if c.run == run_idx {
                Some(c.offset.min(len))
            } else {
                None
            }
        })
    } else {
        None
    };

    let mut splits: Vec<usize> = vec![0, len];
    if let Some((a, b)) = sel_range {
        splits.push(a);
        splits.push(b);
    }
    if let Some(c) = caret_off {
        splits.push(c);
    }
    splits.sort_unstable();
    splits.dedup();

    if caret_off == Some(0) {
        out.push(caret_span(font_size).into_any());
    }

    for w in splits.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        if lo < hi {
            let segment_text: String = chars[lo..hi].iter().collect();
            let in_sel = sel_range.map(|(a, b)| lo >= a && hi <= b).unwrap_or(false);
            out.push(run_span(run, segment_text, font_size, force_bold, in_sel));
        }
        if Some(hi) == caret_off && hi != 0 {
            out.push(caret_span(font_size).into_any());
        }
    }
}

fn run_span(
    run: &InlineRun,
    txt: String,
    font_size: f32,
    force_bold: bool,
    in_selection: bool,
) -> AnyView {
    let bold = run.bold || force_bold;
    let italic = run.italic;
    let code = run.code;
    let is_link = run.link.is_some();
    text(txt)
        .style(move |mut s| {
            s = s.font_size(font_size);
            if bold {
                s = s.font_weight(Weight::BOLD);
            }
            if italic {
                s = s.font_style(floem::text::Style::Italic);
            }
            if code {
                s = s
                    .font_family(MONO_FAMILY.to_string())
                    .background(Color::rgb8(240, 240, 240))
                    .padding_horiz(3.)
                    .border_radius(3.);
            }
            if is_link {
                s = s.color(LINK_COLOR);
            }
            if in_selection {
                s = s.background(SELECTION_BG);
            }
            s
        })
        .into_any()
}

fn caret_span(font_size: f32) -> impl IntoView {
    // 1 logical-px-wide vertical bar sized to the run height.
    empty().style(move |s| {
        s.width(1.)
            .height(font_size * 1.3)
            .background(CARET_COLOR)
    })
}

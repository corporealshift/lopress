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

/// Pane-level signals an editable widget publishes to so the toolbar (and
/// other surfaces) can read which block is focused and operate on its
/// runs / selection. The widget sets `block` to its own id and `signals`
/// to its `(runs, selection)` pair on `FocusGained`; on `FocusLost` it
/// clears `block` (and `signals` if it still owns the slot).
#[derive(Clone, Copy)]
pub struct FocusPublisher {
    pub block: RwSignal<Option<BlockId>>,
    pub signals: RwSignal<Option<(RwSignal<Vec<InlineRun>>, RwSignal<LocalSelection>)>>,
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
// This first-pass widget (Task 8 / Option A in the design discussion) does
// not yet support per-character click hit-testing. Clicking anywhere in the
// block focuses it and snaps the caret to end-of-block. Arrow keys, Home/End,
// Backspace, Delete, and printable character input all work through the pure
// helpers above. Per-pixel click hit-testing is a follow-up.

use crate::ui::blocks::paragraph::{LINK_COLOR, MONO_FAMILY};
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
///
/// `block_id` identifies the owning block in the document — needed so
/// keyboard handlers can emit `BlockAction`s that target the correct block.
/// `on_action` is the chokepoint callback supplied by the editor pane.
/// `focus_target`, when set to this widget's `block_id`, requests focus on
/// the next reactive tick — used to land the caret in newly-split or merged
/// blocks after a structural action.
pub fn editable_inline(
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    font_size: f32,
    force_bold: bool,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    slash_eligible: bool,
) -> impl IntoView {
    let focused: RwSignal<bool> = RwSignal::new(false);

    // Re-render whenever runs / selection / focus change.
    let body = floem::views::dyn_container(
        move || (runs.get(), selection.get(), focused.get()),
        move |(runs_v, sel_v, foc)| {
            render_with_selection(&runs_v, sel_v, foc, font_size, force_bold)
        },
    )
    .style(|s| s.width_full());

    let on_action_for_focus_lost = on_action.clone();
    let on_action_for_keydown = on_action;
    let view = body
        .keyboard_navigable()
        .on_click_stop(move |_| {
            // Click anywhere → collapse selection at end-of-block. Real
            // per-character hit-testing arrives in a follow-up.
            let end = runs.with_untracked(|r| Caret::end(r));
            selection.set(LocalSelection::caret(end));
        })
        .on_event(EventListener::FocusGained, move |_| {
            focused.set(true);
            focus_pub.block.set(Some(block_id));
            focus_pub.signals.set(Some((runs, selection)));
            EventPropagation::Stop
        })
        .on_event(EventListener::FocusLost, move |_| {
            focused.set(false);
            // Only clear the pane-level focus slot if we still own it; if
            // another block already grabbed focus, leave its publication
            // intact.
            if focus_pub.block.get_untracked() == Some(block_id) {
                focus_pub.block.set(None);
                focus_pub.signals.set(None);
            }
            // Commit any in-progress local edits to the document so other
            // widgets observing the doc see the latest text.
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
                    selection,
                    block_id,
                    &on_action_for_keydown,
                    slash_eligible,
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
            // Clear so subsequent re-renders don't keep stealing focus.
            focus_target.set(None);
        }
    });

    view
}

/// Decide what to do for a single `KeyEvent`. Returns `true` when handled.
///
/// `slash_eligible` enables the slash-command trigger for paragraph blocks:
/// when true and the block is empty, typing `/` opens the slash menu instead
/// of inserting the literal character.
fn handle_key_down(
    ke: &floem::keyboard::KeyEvent,
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    block_id: BlockId,
    on_action: &ActionSink,
    slash_eligible: bool,
) -> bool {
    let extending = ke.modifiers.shift();

    match ke.key.logical_key.clone() {
        Key::Named(NamedKey::ArrowLeft) => {
            move_head(runs, selection, extending, |r, c| move_left(r, c));
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            move_head(runs, selection, extending, |r, c| move_right(r, c));
            true
        }
        Key::Named(NamedKey::Home) => {
            move_head(runs, selection, extending, |_r, _c| Caret::START);
            true
        }
        Key::Named(NamedKey::End) => {
            move_head(runs, selection, extending, |r, _c| Caret::end(r));
            true
        }
        Key::Named(NamedKey::Backspace) => {
            let sel = selection.get_untracked();
            if sel.is_collapsed() {
                if sel.head == Caret::START {
                    // Backspace at block start → merge with previous block.
                    // Commit current local runs first so the merge sees the
                    // latest text.
                    let current = runs.get_untracked();
                    on_action(BlockAction::EditInline {
                        block_id,
                        new_runs: current,
                    });
                    on_action(BlockAction::MergeWithPrev { block_id });
                    return true;
                }
                let mut new_caret = sel.head;
                runs.update(|r| {
                    new_caret = backspace(r, new_caret);
                });
                selection.set(LocalSelection::caret(new_caret));
            } else {
                let mut new_sel = sel;
                runs.update(|r| {
                    new_sel = delete_selection(r, sel);
                });
                selection.set(new_sel);
            }
            true
        }
        Key::Named(NamedKey::Delete) => {
            let sel = selection.get_untracked();
            if sel.is_collapsed() {
                let mut new_caret = sel.head;
                runs.update(|r| {
                    new_caret = delete(r, new_caret);
                });
                selection.set(LocalSelection::caret(new_caret));
            } else {
                let mut new_sel = sel;
                runs.update(|r| {
                    new_sel = delete_selection(r, sel);
                });
                selection.set(new_sel);
            }
            true
        }
        Key::Named(NamedKey::Space) => {
            insert_at_selection(runs, selection, ' ');
            true
        }
        Key::Named(NamedKey::Enter) => {
            if ke.modifiers.shift() {
                // Reserve Shift+Enter for soft-break (later); swallow for now.
                return true;
            }
            // Block split. Commit current local runs first so the split is
            // performed on the latest text, then issue Split with the current
            // caret coordinates. The editor pane's action sink will apply
            // both and steer focus to the new block.
            let sel = selection.get_untracked();
            let head = sel.head;
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
            if ke.modifiers.control() || ke.modifiers.meta() {
                if let Some(flag) = inline_flag_for_shortcut(ke) {
                    apply_toggle(runs, selection, flag);
                    return true;
                }
                return false;
            }
            let Some(text) = ke.key.text.as_ref() else {
                return false;
            };
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
                insert_at_selection(runs, selection, ch);
            }
            true
        }
    }
}

/// Move the selection's `head` using `motion`. When `extending` (Shift held)
/// `anchor` stays put. Otherwise the selection collapses around the new head;
/// if the selection was non-collapsed and the user pressed an arrow without
/// shift, the caret jumps to the appropriate end of the prior selection
/// rather than to head ± 1.
fn move_head<F>(
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    extending: bool,
    motion: F,
) where
    F: FnOnce(&[InlineRun], Caret) -> Caret,
{
    let sel = selection.get_untracked();
    let pivot = if extending || sel.is_collapsed() {
        sel.head
    } else {
        let (start, end) = sel.ordered();
        if compare(sel.head, sel.anchor).is_le() {
            start
        } else {
            end
        }
    };
    // For non-extending arrow on a non-collapsed selection, pressing arrow
    // collapses to the corresponding end without further motion.
    let new_head = if !extending && !sel.is_collapsed() {
        pivot
    } else {
        runs.with_untracked(|r| motion(r, pivot))
    };
    let new_sel = if extending {
        LocalSelection {
            anchor: sel.anchor,
            head: new_head,
        }
    } else {
        LocalSelection::caret(new_head)
    };
    selection.set(new_sel);
}

/// Map a Ctrl/Cmd-modified key event to the inline flag it toggles, or
/// `None` when the shortcut isn't one we handle. Both Ctrl and Meta are
/// accepted on every platform — small loss in pure-Mac purity, big gain in
/// "just works on whatever the user reaches for."
fn inline_flag_for_shortcut(ke: &floem::keyboard::KeyEvent) -> Option<InlineFlag> {
    if !(ke.modifiers.control() || ke.modifiers.meta()) {
        return None;
    }
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

fn apply_toggle(
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    flag: InlineFlag,
) {
    let sel = selection.get_untracked();
    let mut new_sel = sel;
    runs.update(|r| {
        new_sel = toggle_inline(r, sel, flag);
    });
    selection.set(new_sel);
}

fn insert_at_selection(
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    ch: char,
) {
    let sel = selection.get_untracked();
    let mut new_caret = sel.head;
    runs.update(|r| {
        // Replace any non-collapsed selection first.
        let collapsed = if sel.is_collapsed() {
            sel
        } else {
            delete_selection(r, sel)
        };
        new_caret = insert_char(r, collapsed.head, ch);
    });
    selection.set(LocalSelection::caret(new_caret));
}

/// Render the runs as a wrapping flow of styled spans. When focused with a
/// collapsed selection, inserts a caret bar at the head position. With a
/// non-collapsed selection, the selected character range is rendered with a
/// highlight background.
fn render_with_selection(
    runs: &[InlineRun],
    sel: LocalSelection,
    focused: bool,
    font_size: f32,
    force_bold: bool,
) -> AnyView {
    if runs.is_empty() {
        return if focused {
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
            sel,
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

/// Slice one run into segments at selection boundaries (and at the caret
/// position when the selection is collapsed) and append the corresponding
/// styled spans to `out`.
fn emit_run_segments(
    run: &InlineRun,
    run_idx: usize,
    sel: LocalSelection,
    focused: bool,
    font_size: f32,
    force_bold: bool,
    out: &mut Vec<AnyView>,
) {
    let chars: Vec<char> = run.text.chars().collect();
    let len = chars.len();

    // Selection extent in this run, in chars (exclusive upper bound).
    let (start, end) = sel.ordered();
    let sel_lo: Option<usize> = if start.run < run_idx {
        Some(0)
    } else if start.run == run_idx {
        Some(start.offset.min(len))
    } else {
        None
    };
    let sel_hi: Option<usize> = if end.run > run_idx {
        Some(len)
    } else if end.run == run_idx {
        Some(end.offset.min(len))
    } else {
        None
    };
    let sel_range: Option<(usize, usize)> = match (sel_lo, sel_hi) {
        (Some(a), Some(b)) if a < b => Some((a, b)),
        _ => None,
    };

    // Caret position in this run (only when collapsed and head is here).
    let caret_off: Option<usize> = if focused && sel.is_collapsed() && sel.head.run == run_idx {
        Some(sel.head.offset.min(len))
    } else {
        None
    };

    // Build sorted, deduped split points.
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

    // Caret at the very start of the run is special-cased: nothing precedes
    // it in this run, so the windowed loop wouldn't insert it.
    if caret_off == Some(0) {
        out.push(caret_span(font_size).into_any());
    }

    for w in splits.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        if lo < hi {
            let segment_text: String = chars[lo..hi].iter().collect();
            let in_sel = sel_range
                .map(|(a, b)| lo >= a && hi <= b)
                .unwrap_or(false);
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

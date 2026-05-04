//! Editable inline-runs widget.
//!
//! The pure-data helpers in this module manipulate a `Vec<InlineRun>` and a
//! `Caret` (a `(run_index, char_offset)` pair). All edits go through these
//! helpers so they can be unit-tested independent of any UI framework. The
//! Floem widget itself lives below the helpers.

use crate::model::types::InlineRun;

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
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::style::FlexWrap;
use floem::text::Weight;
use floem::views::{empty, h_stack_from_iter, text, Decorators};
use floem::{AnyView, IntoView};

const CARET_COLOR: Color = Color::rgb8(40, 40, 40);
const SELECTION_BG: Color = Color::rgb8(180, 210, 255);

/// Build the editable inline-runs widget.
///
/// `runs` is the run vector (mutated in place by edits). `selection` tracks
/// both the caret position (collapsed selection) and any active selection.
/// `font_size` and `force_bold` allow the same widget to be reused for
/// paragraphs and headings.
pub fn editable_inline(
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
    font_size: f32,
    force_bold: bool,
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

    body.keyboard_navigable()
        .on_click_stop(move |_| {
            // Click anywhere → collapse selection at end-of-block. Real
            // per-character hit-testing arrives in a follow-up.
            let end = runs.with_untracked(|r| Caret::end(r));
            selection.set(LocalSelection::caret(end));
        })
        .on_event(EventListener::FocusGained, move |_| {
            focused.set(true);
            EventPropagation::Stop
        })
        .on_event(EventListener::FocusLost, move |_| {
            focused.set(false);
            EventPropagation::Stop
        })
        .on_event(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                if handle_key_down(ke, runs, selection) {
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
        })
}

/// Decide what to do for a single `KeyEvent`. Returns `true` when handled.
fn handle_key_down(
    ke: &floem::keyboard::KeyEvent,
    runs: RwSignal<Vec<InlineRun>>,
    selection: RwSignal<LocalSelection>,
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
            // Reserved for block-split (Task 11). Swallow the keystroke.
            true
        }
        _ => {
            if ke.modifiers.control() || ke.modifiers.meta() {
                return false;
            }
            let Some(text) = ke.key.text.as_ref() else {
                return false;
            };
            for ch in text.chars() {
                if ch.is_control() {
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

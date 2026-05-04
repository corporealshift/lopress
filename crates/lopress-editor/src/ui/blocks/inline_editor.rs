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

/// Build the editable inline-runs widget.
///
/// `runs` is the run vector (mutated in place by edits). `caret` tracks the
/// edit position. `font_size` and `force_bold` allow the same widget to be
/// reused for paragraphs and headings.
pub fn editable_inline(
    runs: RwSignal<Vec<InlineRun>>,
    caret: RwSignal<Caret>,
    font_size: f32,
    force_bold: bool,
) -> impl IntoView {
    let focused: RwSignal<bool> = RwSignal::new(false);

    // Re-render whenever runs / caret / focus change.
    let body = floem::views::dyn_container(
        move || (runs.get(), caret.get(), focused.get()),
        move |(runs_v, caret_v, foc)| {
            render_with_caret(&runs_v, caret_v, foc, font_size, force_bold)
        },
    )
    .style(|s| s.width_full());

    body.keyboard_navigable()
        .on_click_stop(move |_| {
            // Click anywhere → snap caret to end-of-block. Real per-character
            // hit-testing arrives in a follow-up; see module-level note.
            let end = runs.with_untracked(|r| Caret::end(r));
            caret.set(end);
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
                if handle_key_down(ke, runs, caret) {
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

/// Decide what to do for a single `KeyEvent`. Returns `true` when handled
/// (so the caller can stop propagation).
fn handle_key_down(
    ke: &floem::keyboard::KeyEvent,
    runs: RwSignal<Vec<InlineRun>>,
    caret: RwSignal<Caret>,
) -> bool {
    match ke.key.logical_key.clone() {
        Key::Named(NamedKey::ArrowLeft) => {
            let new_caret = runs.with_untracked(|r| move_left(r, caret.get_untracked()));
            caret.set(new_caret);
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            let new_caret = runs.with_untracked(|r| move_right(r, caret.get_untracked()));
            caret.set(new_caret);
            true
        }
        Key::Named(NamedKey::Home) => {
            caret.set(Caret::START);
            true
        }
        Key::Named(NamedKey::End) => {
            let new_caret = runs.with_untracked(|r| Caret::end(r));
            caret.set(new_caret);
            true
        }
        Key::Named(NamedKey::Backspace) => {
            let mut new_caret = caret.get_untracked();
            runs.update(|r| {
                new_caret = backspace(r, new_caret);
            });
            caret.set(new_caret);
            true
        }
        Key::Named(NamedKey::Delete) => {
            let mut new_caret = caret.get_untracked();
            runs.update(|r| {
                new_caret = delete(r, new_caret);
            });
            caret.set(new_caret);
            true
        }
        Key::Named(NamedKey::Space) => {
            insert_at_caret(runs, caret, ' ');
            true
        }
        Key::Named(NamedKey::Enter) => {
            // Reserved for block-split (Task 11). Swallow the keystroke so the
            // user doesn't see a stray system beep on platforms that beep.
            true
        }
        _ => {
            // Printable text: rely on KeyEvent.key.text. Skip when modifiers
            // suggest a shortcut (Ctrl/Meta combos are handled elsewhere).
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
                insert_at_caret(runs, caret, ch);
            }
            true
        }
    }
}

fn insert_at_caret(runs: RwSignal<Vec<InlineRun>>, caret: RwSignal<Caret>, ch: char) {
    let mut new_caret = caret.get_untracked();
    runs.update(|r| {
        new_caret = insert_char(r, new_caret, ch);
    });
    caret.set(new_caret);
}

/// Render the runs as a wrapping flow of styled spans. When focused, splits
/// the run that contains the caret in two and inserts a thin caret indicator
/// between the halves.
fn render_with_caret(
    runs: &[InlineRun],
    caret: Caret,
    focused: bool,
    font_size: f32,
    force_bold: bool,
) -> AnyView {
    // Empty runs vector — render only the caret (when focused) so the user
    // has somewhere to type into. Otherwise an empty view.
    if runs.is_empty() {
        return if focused {
            caret_span(font_size).into_any()
        } else {
            empty().into_any()
        };
    }

    let caret_run = caret.run.min(runs.len().saturating_sub(1));
    let mut elements: Vec<AnyView> = Vec::with_capacity(runs.len() + 2);

    for (i, run) in runs.iter().enumerate() {
        if focused && i == caret_run {
            let chars: Vec<char> = run.text.chars().collect();
            let off = caret.offset.min(chars.len());
            let before: String = chars[..off].iter().collect();
            let after: String = chars[off..].iter().collect();
            if !before.is_empty() {
                elements.push(run_span(run, before, font_size, force_bold));
            }
            elements.push(caret_span(font_size).into_any());
            if !after.is_empty() {
                elements.push(run_span(run, after, font_size, force_bold));
            }
        } else {
            elements.push(run_span(run, run.text.clone(), font_size, force_bold));
        }
    }

    h_stack_from_iter(elements)
        .style(|s| s.flex_wrap(FlexWrap::Wrap).width_full())
        .into_any()
}

fn run_span(run: &InlineRun, txt: String, font_size: f32, force_bold: bool) -> AnyView {
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

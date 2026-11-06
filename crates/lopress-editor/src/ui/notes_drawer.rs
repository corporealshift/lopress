//! Bottom drawer for the open document's notes.
//!
//! In-flow rather than an overlay, so floem hit-tests it normally. The editor
//! is hidden, not unmounted, when the drawer collapses, so the caret and undo
//! history survive. Floem clears focus on every pointer down, so clicking the
//! header blurs the editor; opening the drawer hands focus back.

use crate::ui::blocks::inline_editor::is_save_key;
use crate::ui::editing::notes::NotesState;
use crate::ui::editing::save_pipeline::SaveSink;
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::keypress::default_key_handler;
use floem::views::{dyn_container, empty, h_stack, label, text_editor_keys, v_stack, Decorators};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::path::PathBuf;
use std::rc::Rc;

const HEADER_HEIGHT: f64 = 28.0;
/// Share of the window the drawer takes when open.
const OPEN_HEIGHT_PCT: f64 = 40.0;
const BG: Color = Color::rgb8(250, 250, 250);
const BORDER: Color = Color::rgb8(220, 220, 220);
const LABEL_FG: Color = Color::rgb8(110, 110, 120);
const ERR_FG: Color = Color::rgb8(170, 40, 40);

/// Build the drawer. Hidden when no document is open.
pub fn notes_drawer_view(
    notes: NotesState,
    current_path: RwSignal<Option<PathBuf>>,
    on_save: SaveSink,
) -> impl IntoView {
    // Keyed on the generation, not `current_path`: a slug rename must not
    // rebuild the editor under someone typing in it.
    let body = dyn_container(
        move || notes.doc_generation.get(),
        move |_| notes_editor(notes, Rc::clone(&on_save)),
    )
    .style(move |s| {
        s.width_full()
            .flex_grow(1.0f32)
            .min_height(0.)
            .apply_if(!notes.open.get(), |s| s.hide())
    });

    v_stack((header(notes), body)).style(move |s| {
        let open = notes.open.get();
        s.width_full()
            .flex_shrink(0.0f32)
            .border_top(1.)
            .border_color(BORDER)
            .background(BG)
            .apply_if(open, |s| s.height_pct(OPEN_HEIGHT_PCT))
            .apply_if(!open, |s| s.height(HEADER_HEIGHT))
            .apply_if(current_path.get().is_none(), |s| s.hide())
    })
}

/// The always-visible bar: toggle, "has notes" marker, and any error.
fn header(notes: NotesState) -> impl IntoView {
    let title = label(move || {
        if notes.open.get() {
            "\u{25be} Notes".to_string()
        } else {
            "\u{25b8} Notes".to_string()
        }
    })
    .style(|s| {
        s.font_size(12.)
            .font_weight(Weight::SEMIBOLD)
            .color(LABEL_FG)
    });

    let marker = label(move || {
        if notes.text.with(|t| t.trim().is_empty()) {
            String::new()
        } else {
            "\u{25cf}".to_string()
        }
    })
    .style(|s| s.font_size(9.).color(LABEL_FG));

    let error = label(move || {
        notes
            .load_error
            .get()
            .or_else(|| notes.save_error.get())
            .unwrap_or_default()
    })
    .style(|s| s.font_size(11.).color(ERR_FG).flex_grow(1.0f32));

    h_stack((title, marker, error))
        .style(|s| {
            s.width_full()
                .height(HEADER_HEIGHT)
                .flex_shrink(0.0f32)
                .items_center()
                .gap(6.)
                .padding_horiz(10.)
        })
        .on_click_stop(move |_| notes.open.update(|open| *open = !*open))
}

/// The text editor for the current document's notes. Nothing is mounted after
/// a load failure, so an empty buffer can't overwrite the real notes.
fn notes_editor(notes: NotesState, on_save: SaveSink) -> AnyView {
    if notes.load_error.with_untracked(Option::is_some) {
        return empty().into_any();
    }

    let initial = notes.text.get_untracked();
    let text_ed = text_editor_keys(Rope::from(initial.as_str()), move |editor_sig, kp, ms| {
        if is_save_key(kp, ms) {
            on_save(None);
            return CommandExecuted::Yes;
        }
        default_key_handler(editor_sig)(kp, ms)
    })
    .placeholder("Notes for this post\u{2026}");
    let editor = text_ed.editor().clone();

    // Mirror every edit into the buffer. `cache_rev` is the same trigger the
    // plugin textarea uses, and `doc().text()` reads untracked. The comparison
    // makes the effect's first run a no-op.
    let doc_rev = text_ed.doc().cache_rev();
    let editor_for_text = editor.clone();
    create_effect(move |_| {
        doc_rev.get();
        let live = String::from(&editor_for_text.doc().text());
        if notes.text.with_untracked(|t| *t != live) {
            notes.text.set(live);
            notes.dirty.set(true);
        }
    });

    // Focus the editor when the drawer opens: the header click cleared focus,
    // so anything typed next would otherwise go nowhere. Skips the first run,
    // so loading another document never steals focus from the article.
    let editor_for_focus = editor.clone();
    create_effect(move |prev: Option<()>| {
        let open = notes.open.get();
        if prev.is_some() && open {
            if let Some(view_id) = editor_for_focus.editor_view_id.get_untracked() {
                view_id.request_focus();
            }
        }
    });

    text_ed
        .editor_style(|s| s.hide_gutter(true))
        .style(|s| s.width_full().height_full().font_size(13.).padding(8.))
        .into_any()
}

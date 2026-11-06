//! Bottom strip: build status / save state / word count / server URL.
//!
//! Reactive inputs:
//! - `build_status` — driven by a 250 ms poll on `Session::build_status()`
//!   set up in `mod.rs`. Background thread writes the mutex; we just read.
//! - `status` and `save_error` — set by the save pipeline.
//! - `current_doc` — for word count.
//! - `serve_status` — driven by a 250 ms poll on `Session::serve_status()`
//!   set up in `mod.rs`. Background thread writes the mutex; we just read.

use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{button, dyn_container, empty, h_stack, label, Decorators};
use floem::{AnyView, Clipboard, IntoView};
use lopress_gui_host::{BuildStatus, DiskState, ServeStatus};

use crate::model::types::{BlockBody, EditorBlock, EditorDoc};
use crate::ui::editing::notes::NotesState;
use crate::ui::editing::save_pipeline::{DocStatus, SaveSink};
use crate::ui::icons::dot;
use crate::ui::theme;

const FOOTER_HEIGHT: f64 = 28.0;
const SAVE_BUTTON_HEIGHT: f64 = 22.0;

/// Build the footer view.
pub fn footer_view(
    build_status: RwSignal<BuildStatus>,
    status: DocStatus,
    save_error: RwSignal<Option<String>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    serve_status: RwSignal<ServeStatus>,
    notes: NotesState,
    on_save: SaveSink,
) -> impl IntoView {
    let dirty = status.dirty;
    let build_label = dyn_container(
        move || build_status.get(),
        move |status| build_status_view(&status).into_any(),
    );

    let save_label = dyn_container(
        move || (dirty.get() || notes.dirty.get(), save_error.get()),
        move |(d, err)| save_state_view(d, err).into_any(),
    );

    let save_button = button(label(|| "Save".to_string()))
        .action(move || {
            on_save(None);
        })
        .disabled(move || {
            !(dirty.get() || notes.dirty.get() || status.disk.get() == DiskState::Missing)
        })
        .style(|s| {
            s.font_size(theme::UI_SMALL)
                .height(SAVE_BUTTON_HEIGHT)
                .padding_horiz(theme::SPACE_8)
                .padding_vert(0.)
        });

    let word_label = label(move || {
        let n = current_doc.with(|maybe| maybe.as_ref().map(word_count).unwrap_or(0));
        format!("{n} words")
    })
    .style(muted_text);

    let url_view = dyn_container(
        move || serve_status.get(),
        move |status| serve_status_view(&status).into_any(),
    );

    h_stack((
        build_label,
        h_stack((save_button, save_label)).style(|s| s.gap(theme::SPACE_6).items_center()),
        word_label,
        empty().style(|s| s.flex_grow(1.0f32)),
        url_view,
    ))
    .style(|s| {
        // Without `flex_shrink(0)`, a long document squeezes the footer: the
        // columns above overflow and every sibling gives up a share.
        s.width_full()
            .height(FOOTER_HEIGHT)
            .flex_shrink(0.0f32)
            .padding_horiz(theme::SPACE_16)
            .gap(theme::SPACE_16)
            .background(theme::CHROME_BG)
            .border_top(1.)
            .border_color(theme::RULE)
            .items_center()
    })
}

fn muted_text(s: floem::style::Style) -> floem::style::Style {
    s.color(theme::TEXT_MUTED).font_size(theme::UI_SMALL)
}

fn build_status_view(status: &BuildStatus) -> AnyView {
    let (text, color) = match status {
        BuildStatus::Idle => ("idle".to_string(), theme::TEXT_MUTED),
        BuildStatus::Building => ("building…".to_string(), theme::WARN),
        BuildStatus::Ok {
            pages_rendered,
            pages_skipped,
            duration_ms,
        } => (
            format!("ok · {pages_rendered}+{pages_skipped} pages · {duration_ms} ms"),
            theme::OK,
        ),
        BuildStatus::Failed { message } => (format!("build failed: {message}"), theme::ACCENT),
    };
    let failed = matches!(status, BuildStatus::Failed { .. });
    h_stack((
        dot(color),
        label(move || text.clone())
            .style(move |s| muted_text(s).apply_if(failed, |s| s.color(theme::ACCENT))),
    ))
    .style(|s| s.gap(theme::SPACE_6).items_center())
    .into_any()
}

fn save_state_view(dirty: bool, error: Option<String>) -> AnyView {
    if let Some(msg) = error {
        return label(move || format!("save error: {msg}"))
            .style(|s| s.color(theme::ACCENT).font_size(theme::UI_SMALL))
            .into_any();
    }
    if dirty {
        return label(|| "Unsaved".to_string())
            .style(|s| s.color(theme::WARN).font_size(theme::UI_SMALL))
            .into_any();
    }
    label(|| "Saved".to_string()).style(muted_text).into_any()
}

fn serve_status_view(status: &ServeStatus) -> AnyView {
    match status {
        ServeStatus::Starting => label(|| "starting preview…".to_string())
            .style(muted_text)
            .into_any(),
        ServeStatus::Listening { url } => {
            let url_for_label = url.clone();
            let url_for_click = url.clone();
            label(move || url_for_label.clone())
                .on_click_stop(move |_| {
                    let _ = Clipboard::set_contents(url_for_click.clone());
                })
                .style(|s| {
                    muted_text(s)
                        .cursor(floem::style::CursorStyle::Pointer)
                        .hover(|s| s.color(theme::TEXT))
                })
                .into_any()
        }
        ServeStatus::Unavailable { .. } => label(|| "no preview".to_string())
            .style(muted_text)
            .into_any(),
    }
}

/// Whitespace-split word count over inline-run text and code body text.
/// List items contribute their inline runs.
pub fn word_count(doc: &EditorDoc) -> usize {
    doc.blocks.iter().map(block_word_count).sum()
}

fn block_word_count(b: &EditorBlock) -> usize {
    match &b.body {
        BlockBody::Inline(runs) => runs.iter().map(|r| r.text.split_whitespace().count()).sum(),
        BlockBody::Code(text) => text.split_whitespace().count(),
        BlockBody::List(items) => items
            .iter()
            .flat_map(|it| it.runs.iter())
            .map(|r| r.text.split_whitespace().count())
            .sum(),
        BlockBody::Table(data) => data
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| cell.runs.iter())
            .map(|r| r.text.split_whitespace().count())
            .sum(),
        BlockBody::Opaque(_) => 0,
    }
}

/// Initial poll loop — call once at editing-view setup. Re-schedules itself
/// every 250 ms; cheap enough since `BuildStatus` is just a small enum read
/// behind a mutex.
pub fn start_build_status_poll(
    session: std::rc::Rc<dyn Fn() -> BuildStatus>,
    sink: RwSignal<BuildStatus>,
) {
    fn schedule(session: std::rc::Rc<dyn Fn() -> BuildStatus>, sink: RwSignal<BuildStatus>) {
        floem::action::exec_after(std::time::Duration::from_millis(250), move |_| {
            sink.set((session)());
            schedule(session, sink);
        });
    }
    sink.set((session)());
    schedule(session, sink);
}

/// Initial poll loop for `ServeStatus`. Mirrors `start_build_status_poll`.
/// Re-schedules itself every 250 ms.
pub fn start_serve_status_poll(
    session: std::rc::Rc<dyn Fn() -> ServeStatus>,
    sink: RwSignal<ServeStatus>,
) {
    fn schedule(session: std::rc::Rc<dyn Fn() -> ServeStatus>, sink: RwSignal<ServeStatus>) {
        floem::action::exec_after(std::time::Duration::from_millis(250), move |_| {
            sink.set((session)());
            schedule(session, sink);
        });
    }
    sink.set((session)());
    schedule(session, sink);
}

/// Resolve the server URL string, if the server is up. Used by the footer's
/// click-to-copy element.
pub fn serve_url(status: &ServeStatus) -> Option<String> {
    match status {
        ServeStatus::Listening { url } => Some(url.clone()),
        ServeStatus::Unavailable { .. } | ServeStatus::Starting => None,
    }
}

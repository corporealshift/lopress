//! Bottom strip: build status / save state / word count / server URL.
//!
//! Reactive inputs:
//! - `build_status` — driven by a 250 ms poll on `Session::build_status()`
//!   set up in `mod.rs`. Background thread writes the mutex; we just read.
//! - `dirty` and `save_error` — set by the save-debounce machinery in Task 21.
//! - `current_doc` — for word count.
//! - `serve_url` — captured once at footer-build time from `Session::serve_status()`.

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{dyn_container, empty, h_stack, label, Decorators};
use floem::{AnyView, Clipboard, IntoView};
use lopress_gui_host::{BuildStatus, ServeStatus};

use crate::model::types::{BlockBody, EditorBlock, EditorDoc};

const FOOTER_HEIGHT: f64 = 28.0;
const BG: Color = Color::rgb8(245, 245, 245);
const BORDER: Color = Color::rgb8(220, 220, 220);
const FG: Color = Color::rgb8(60, 60, 70);
const MUTED: Color = Color::rgb8(120, 120, 130);
const OK: Color = Color::rgb8(40, 130, 60);
const WARN: Color = Color::rgb8(180, 100, 0);
const ERR: Color = Color::rgb8(170, 40, 40);

/// Build the footer view.
pub fn footer_view(
    build_status: RwSignal<BuildStatus>,
    dirty: RwSignal<bool>,
    save_error: RwSignal<Option<String>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    serve_url: Option<String>,
) -> impl IntoView {
    let build_label = dyn_container(
        move || build_status.get(),
        move |status| build_status_view(&status).into_any(),
    );

    let save_label = dyn_container(
        move || (dirty.get(), save_error.get()),
        move |(d, err)| save_state_view(d, err).into_any(),
    );

    let word_label = label(move || {
        let n = current_doc.with(|maybe| maybe.as_ref().map(word_count).unwrap_or(0));
        format!("{n} words")
    })
    .style(|s| s.color(MUTED).font_size(12.));

    let url_view: AnyView = match serve_url {
        Some(url) => {
            let url_for_click = url.clone();
            label(move || url.clone())
                .on_click_stop(move |_| {
                    let _ = Clipboard::set_contents(url_for_click.clone());
                })
                .style(|s| {
                    s.color(MUTED)
                        .font_size(12.)
                        .cursor(floem::style::CursorStyle::Pointer)
                        .hover(|s| s.color(FG))
                })
                .into_any()
        }
        None => label(|| "no preview".to_string())
            .style(|s| s.color(MUTED).font_size(12.))
            .into_any(),
    };

    h_stack((
        build_label.style(|s| s.padding_horiz(10.)),
        sep(),
        save_label.style(|s| s.padding_horiz(10.)),
        sep(),
        word_label.style(|s| s.padding_horiz(10.)),
        empty().style(|s| s.flex_grow(1.)),
        url_view.style(|s| s.padding_horiz(10.)),
    ))
    .style(|s| {
        s.width_full()
            .height(FOOTER_HEIGHT)
            .background(BG)
            .border_top(1.)
            .border_color(BORDER)
            .items_center()
    })
}

fn sep() -> AnyView {
    empty()
        .style(|s| s.width(1.).height(14.).background(BORDER))
        .into_any()
}

fn build_status_view(status: &BuildStatus) -> AnyView {
    let (text, color) = match status {
        BuildStatus::Idle => ("idle".to_string(), MUTED),
        BuildStatus::Building => ("building…".to_string(), WARN),
        BuildStatus::Ok {
            pages_rendered,
            pages_skipped,
            duration_ms,
        } => (
            format!("ok · {pages_rendered}+{pages_skipped} pages · {duration_ms} ms"),
            OK,
        ),
        BuildStatus::Failed { message } => (format!("build failed: {message}"), ERR),
    };
    label(move || text.clone())
        .style(move |s| s.color(color).font_size(12.))
        .into_any()
}

fn save_state_view(dirty: bool, error: Option<String>) -> AnyView {
    if let Some(msg) = error {
        return label(move || format!("save error: {msg}"))
            .style(|s| s.color(ERR).font_size(12.))
            .into_any();
    }
    if dirty {
        return label(|| "unsaved".to_string())
            .style(|s| s.color(WARN).font_size(12.))
            .into_any();
    }
    label(|| "saved".to_string())
        .style(|s| s.color(MUTED).font_size(12.))
        .into_any()
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

/// Resolve the server URL string, if the server is up. Used by the footer's
/// click-to-copy element.
pub fn serve_url(status: &ServeStatus) -> Option<String> {
    match status {
        ServeStatus::Listening { url } => Some(url.clone()),
        ServeStatus::Unavailable { .. } | ServeStatus::Starting => None,
    }
}

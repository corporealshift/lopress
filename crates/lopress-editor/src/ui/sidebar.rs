//! Left-pinned sidebar listing posts and pages.
//!
//! Reads `WorkspaceSummary` from a parent-owned signal so external refreshes
//! (after creating a file, after the watcher rescans on save) flow through
//! reactively. Clicking a row asks the parent to open that document via the
//! `on_open` callback. The parent also tells us which path is currently
//! active (`current_path`), so we can highlight that row.

use chrono::Local;
use floem::reactive::{RwSignal, SignalGet};
use floem::text::Weight;
use floem::views::{
    button, dyn_container, h_stack, h_stack_from_iter, label, scroll, v_stack, v_stack_from_iter,
    Decorators,
};
use floem::{AnyView, IntoView};
use lopress_gui_host::{DocumentRef, WorkspaceSummary};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ui::icons::{self, icon};
use crate::ui::theme;

const SIDEBAR_WIDTH: f64 = 220.0;
const PRIMARY_BUTTON_HEIGHT: f64 = 34.0;
const QUIET_BUTTON_HEIGHT: f64 = 30.0;
const ICON_SIZE: f32 = 14.0;
const ROW_PADDING_VERT: f64 = 7.0;

/// Build the sidebar view.
///
/// `workspace` is reactive — replaced when the user creates a new doc or the
/// watcher rescans. `current_path` is the path of the currently open
/// document (used for active-row highlight). `on_open` and the `on_new_*`
/// callbacks are how the sidebar talks to the rest of the app.
pub fn sidebar_view(
    workspace: RwSignal<WorkspaceSummary>,
    current_path: RwSignal<Option<PathBuf>>,
    on_open: Rc<dyn Fn(DocumentRef)>,
    on_new_post: Rc<dyn Fn()>,
    on_new_page: Rc<dyn Fn()>,
    on_site_settings: Rc<dyn Fn()>,
) -> impl IntoView {
    // Re-render the lists when `workspace` or `current_path` changes.
    let on_open_for_lists = on_open;
    let lists = dyn_container(
        move || (workspace.get(), current_path.get()),
        move |(ws, current)| {
            let posts_section = group(
                "Posts",
                ws.posts.clone(),
                current.clone(),
                on_open_for_lists.clone(),
            );
            let pages_section = group(
                "Pages",
                ws.pages.clone(),
                current,
                on_open_for_lists.clone(),
            );
            v_stack_from_iter(vec![posts_section.into_any(), pages_section.into_any()])
                .style(|s| s.gap(theme::SPACE_18).width_full())
                .into_any()
        },
    )
    .style(|s| s.width_full().flex_grow(1.0f32));

    let on_new_post_btn = on_new_post.clone();
    let new_post_btn = button(
        h_stack((
            icon(icons::PLUS, ICON_SIZE, theme::ON_ACCENT),
            label(|| "New post".to_string()),
        ))
        .style(|s| s.gap(theme::SPACE_6).items_center()),
    )
    .action(move || (on_new_post_btn)())
    .style(|s| {
        theme::primary_button(s)
            .width_full()
            .height(PRIMARY_BUTTON_HEIGHT)
            .padding_vert(0.)
            .margin_bottom(theme::SPACE_6)
    });
    let on_new_page_btn = on_new_page.clone();
    let new_page_btn = quiet_row_button(icons::PLUS, "New page", move || (on_new_page_btn)());

    let on_site_settings_btn = on_site_settings.clone();
    let site_settings_btn = quiet_row_button(icons::SLIDERS, "Site settings", move || {
        (on_site_settings_btn)()
    });

    let footer =
        v_stack((new_post_btn, new_page_btn, site_settings_btn)).style(|s| s.gap(theme::SPACE_2));

    v_stack((
        scroll(lists).style(|s| s.flex_grow(1.0f32).min_height(0.).width_full()),
        footer,
    ))
    .style(|s| {
        s.flex_col()
            .width(SIDEBAR_WIDTH)
            .flex_shrink(0.0f32)
            .height_full()
            .gap(theme::SPACE_18)
            .padding_top(theme::SPACE_18)
            .padding_horiz(theme::SPACE_10)
            .padding_bottom(theme::SPACE_12)
            .background(theme::CHROME_BG)
    })
}

/// One of the sidebar's left-aligned quiet buttons.
fn quiet_row_button(
    markup: &'static str,
    text: &'static str,
    action: impl Fn() + 'static,
) -> impl IntoView {
    button(
        h_stack((
            icon(markup, ICON_SIZE, theme::TEXT_SOFT),
            label(move || text.to_string()),
        ))
        .style(|s| s.gap(theme::SPACE_8).items_center()),
    )
    .action(action)
    .style(|s| {
        s.width_full()
            .height(QUIET_BUTTON_HEIGHT)
            .padding_vert(0.)
            .justify_start()
    })
}

fn group(
    title: &'static str,
    items: Vec<DocumentRef>,
    current: Option<PathBuf>,
    on_open: Rc<dyn Fn(DocumentRef)>,
) -> AnyView {
    let header = label(move || title.to_string()).style(|s| {
        s.padding_horiz(theme::SPACE_10)
            .padding_bottom(theme::SPACE_6)
            .color(theme::TEXT_MUTED)
            .font_size(theme::UI_SMALL)
            .font_weight(Weight::SEMIBOLD)
    });
    let mut rows: Vec<AnyView> = Vec::with_capacity(items.len() + 1);
    rows.push(header.into_any());
    if items.is_empty() {
        rows.push(
            label(|| "(none)".to_string())
                .style(|s| {
                    s.padding_horiz(theme::SPACE_10)
                        .color(theme::PLACEHOLDER)
                        .font_size(theme::UI_SMALL)
                })
                .into_any(),
        );
    } else {
        for item in items {
            let active = current.as_deref() == Some(item.path.as_path());
            rows.push(row(item, active, on_open.clone()));
        }
    }
    v_stack_from_iter(rows)
        .style(|s| s.width_full().gap(theme::SPACE_2))
        .into_any()
}

fn row(item: DocumentRef, active: bool, on_open: Rc<dyn Fn(DocumentRef)>) -> AnyView {
    let title = item.title.clone();
    let title_view = label(move || title.clone()).style(move |s| {
        s.flex_grow(1.0f32)
            .flex_basis(0.)
            .min_width(0.)
            .text_ellipsis()
            .font_size(theme::UI)
            .color(theme::TEXT)
            .apply_if(active, |s| s.font_weight(Weight::MEDIUM))
    });

    let mut elements: Vec<AnyView> = vec![title_view.into_any()];
    if item.is_draft {
        elements.push(marker("Draft").into_any());
    }
    if item.has_parse_error {
        elements.push(marker("Error").into_any());
    }

    let inner =
        h_stack_from_iter(elements).style(|s| s.gap(theme::SPACE_8).items_center().width_full());

    let item_for_click = item;
    inner
        .on_click_stop(move |_| (on_open)(item_for_click.clone()))
        .style(move |s| {
            let s = s
                .padding_horiz(theme::SPACE_10)
                .padding_vert(ROW_PADDING_VERT)
                .width_full()
                .border_radius(theme::CORNER_MD)
                .cursor(floem::style::CursorStyle::Pointer);
            if active {
                s.background(theme::ROW_ACTIVE_BG)
            } else {
                s.hover(|s| s.background(theme::ROW_HOVER_BG))
            }
        })
        .into_any()
}

/// Small red text after a row's title.
fn marker(text: &'static str) -> impl IntoView {
    label(move || text.to_string()).style(|s| {
        s.flex_shrink(0.0f32)
            .color(theme::ACCENT)
            .font_size(theme::UI_TINY)
            .font_weight(Weight::SEMIBOLD)
    })
}

// ── New-document stub generation ──────────────────────────────────────────

/// Pick a unique `{base}.md` filename inside `dir`, where `base` is an
/// already-slugified stem (e.g. the slugified default title). Falls back to
/// `"untitled"` if `base` is empty.
pub fn unique_doc_path(dir: &Path, base: &str) -> PathBuf {
    let base = if base.is_empty() { "untitled" } else { base };
    crate::ui::editing::filename_sync::unique_stem(dir, base, None, &|p: &Path| p.exists())
}

/// Default stub markdown for a new post or page.
///
/// Includes an `# {title}` H1 so the document opens with an editable block —
/// without one the editor pane is blank and there is no way to add the first
/// block.
pub fn new_doc_stub(title: &str) -> String {
    let date = Local::now().format("%Y-%m-%d");
    format!(
        "---\ntitle: {title}\ndate: {date}\ndraft: true\n---\n\n# {title}\n",
        title = title,
        date = date
    )
}

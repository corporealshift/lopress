//! Left-pinned sidebar listing posts and pages.
//!
//! Reads `WorkspaceSummary` from a parent-owned signal so external refreshes
//! (after creating a file, after the watcher rescans on save) flow through
//! reactively. Clicking a row asks the parent to open that document via the
//! `on_open` callback. The parent also tells us which path is currently
//! active (`current_path`), so we can highlight that row.

use chrono::Local;
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet};
use floem::text::Weight;
use floem::views::{
    button, dyn_container, h_stack_from_iter, label, scroll, v_stack, v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use lopress_gui_host::{DocumentRef, WorkspaceSummary};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const SIDEBAR_WIDTH: f64 = 220.0;
const BG: Color = Color::rgb8(248, 248, 248);
const BORDER: Color = Color::rgb8(220, 220, 220);
const ACTIVE_BG: Color = Color::rgb8(220, 230, 250);
const HOVER_BG: Color = Color::rgb8(238, 238, 240);
const GROUP_FG: Color = Color::rgb8(110, 110, 120);
const PILL_DRAFT_BG: Color = Color::rgb8(255, 240, 200);
const PILL_DRAFT_FG: Color = Color::rgb8(140, 100, 0);
const PILL_ERR_BG: Color = Color::rgb8(255, 220, 220);
const PILL_ERR_FG: Color = Color::rgb8(170, 40, 40);

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
                .style(|s| s.gap(8.).width_full())
                .into_any()
        },
    )
    .style(|s| s.width_full().flex_grow(1.));

    let on_new_post_btn = on_new_post.clone();
    let new_post_btn = button(label(|| "+ New post".to_string()))
        .action(move || (on_new_post_btn)())
        .style(|s| s.width_full().padding_vert(4.));
    let on_new_page_btn = on_new_page.clone();
    let new_page_btn = button(label(|| "+ New page".to_string()))
        .action(move || (on_new_page_btn)())
        .style(|s| s.width_full().padding_vert(4.));

    let footer = v_stack((new_post_btn, new_page_btn))
        .style(|s| s.gap(4.).padding(8.).border_top(1.).border_color(BORDER));

    v_stack((scroll(lists).style(|s| s.flex_grow(1.)), footer)).style(|s| {
        s.flex_col()
            .width(SIDEBAR_WIDTH)
            .height_full()
            .background(BG)
            .border_right(1.)
            .border_color(BORDER)
    })
}

fn group(
    title: &'static str,
    items: Vec<DocumentRef>,
    current: Option<PathBuf>,
    on_open: Rc<dyn Fn(DocumentRef)>,
) -> AnyView {
    let header = label(move || title.to_string()).style(|s| {
        s.padding_horiz(8.)
            .padding_top(8.)
            .padding_bottom(4.)
            .color(GROUP_FG)
            .font_size(11.)
            .font_weight(Weight::SEMIBOLD)
    });
    let mut rows: Vec<AnyView> = Vec::with_capacity(items.len() + 1);
    rows.push(header.into_any());
    if items.is_empty() {
        rows.push(
            label(|| "(none)".to_string())
                .style(|s| {
                    s.padding_horiz(12.)
                        .color(Color::rgb8(160, 160, 170))
                        .font_size(12.)
                })
                .into_any(),
        );
    } else {
        for item in items {
            let active = current.as_deref() == Some(item.path.as_path());
            rows.push(row(item, active, on_open.clone()));
        }
    }
    v_stack_from_iter(rows).style(|s| s.width_full()).into_any()
}

fn row(item: DocumentRef, active: bool, on_open: Rc<dyn Fn(DocumentRef)>) -> AnyView {
    let title = item.title.clone();
    let title_view = label(move || title.clone()).style(|s| s.flex_grow(1.).font_size(13.));

    let mut elements: Vec<AnyView> = vec![title_view.into_any()];
    if item.is_draft {
        elements.push(pill("draft", PILL_DRAFT_BG, PILL_DRAFT_FG).into_any());
    }
    if item.has_parse_error {
        elements.push(pill("error", PILL_ERR_BG, PILL_ERR_FG).into_any());
    }

    let inner = h_stack_from_iter(elements).style(|s| s.gap(4.).items_center().width_full());

    let item_for_click = item;
    inner
        .on_click_stop(move |_| (on_open)(item_for_click.clone()))
        .style(move |s| {
            let s = s
                .padding_horiz(12.)
                .padding_vert(4.)
                .width_full()
                .cursor(floem::style::CursorStyle::Pointer);
            if active {
                s.background(ACTIVE_BG)
            } else {
                s.hover(|s| s.background(HOVER_BG))
            }
        })
        .into_any()
}

fn pill(text: &'static str, bg: Color, fg: Color) -> impl IntoView {
    label(move || text.to_string()).style(move |s| {
        s.padding_horiz(6.)
            .padding_vert(0.)
            .background(bg)
            .color(fg)
            .border_radius(8.)
            .font_size(10.)
            .font_weight(Weight::SEMIBOLD)
    })
}

// ── New-document stub generation ──────────────────────────────────────────

/// Pick a unique `untitled-N.md` filename inside `dir`.
pub fn unique_untitled_path(dir: &Path) -> PathBuf {
    let base = "untitled";
    let mut n: u32 = 1;
    loop {
        let candidate = dir.join(format!("{base}-{n}.md"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
        if n > 9999 {
            // Defensive: never loop forever.
            return dir.join(format!("{base}-{n}.md"));
        }
    }
}

/// Default stub markdown for a new post or page.
pub fn new_doc_stub(title: &str) -> String {
    let date = Local::now().format("%Y-%m-%d");
    format!(
        "---\ntitle: {title}\ndate: {date}\ndraft: true\n---\n\n",
        title = title,
        date = date
    )
}

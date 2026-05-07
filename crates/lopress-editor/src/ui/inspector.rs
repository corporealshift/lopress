//! Right-pinned inspector pane: front-matter form.
//!
//! Each form field owns a local `RwSignal<String>` (or `<bool>` for draft)
//! seeded from `current_doc.front_matter`. A `create_effect` watches each
//! buffer and writes back into `current_doc` so that edits flow through the
//! same reactive path the editor pane already uses.
//!
//! The form is wrapped in a `dyn_container` keyed on `current_path` so it
//! rebuilds (re-seeds the buffers) only when the user opens a different
//! document, not on every keystroke in the editor pane.

use chrono::NaiveDate;
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{
    dyn_container, empty, label, scroll, text_input, v_stack, Checkbox, Decorators,
};
use floem::{AnyView, IntoView};
use std::path::PathBuf;

use crate::model::types::EditorDoc;

const PANE_WIDTH: f64 = 280.0;
const BG: Color = Color::rgb8(250, 250, 250);
const BORDER: Color = Color::rgb8(220, 220, 220);
const LABEL_FG: Color = Color::rgb8(110, 110, 120);
const ERR_FG: Color = Color::rgb8(170, 40, 40);
const INPUT_BG: Color = Color::rgb8(255, 255, 255);
const INPUT_BORDER: Color = Color::rgb8(210, 210, 215);

/// Build the inspector view. Empty placeholder when no doc is open.
pub fn inspector_view(
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
) -> impl IntoView {
    let body = dyn_container(
        move || current_path.get(),
        move |path| match current_doc.with_untracked(|d| d.clone()) {
            Some(doc) => form(doc, path, current_doc).into_any(),
            None => empty().into_any(),
        },
    )
    .style(|s| s.width_full().flex_grow(1.));

    scroll(body).style(|s| {
        s.width(PANE_WIDTH)
            .height_full()
            .background(BG)
            .border_left(1.)
            .border_color(BORDER)
    })
}

fn form(
    doc: EditorDoc,
    path: Option<PathBuf>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> AnyView {
    let fm = &doc.front_matter;
    let title_buf: RwSignal<String> = RwSignal::new(fm.title.clone().unwrap_or_default());
    let slug_buf: RwSignal<String> = RwSignal::new(fm.slug.clone().unwrap_or_default());
    let date_buf: RwSignal<String> =
        RwSignal::new(fm.date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default());
    let tags_buf: RwSignal<String> = RwSignal::new(fm.tags.join(", "));
    let draft_sig: RwSignal<bool> = RwSignal::new(fm.draft);
    let date_invalid: RwSignal<bool> = RwSignal::new(false);

    // Slug placeholder: file stem of the current path. Avoids forcing
    // authors to type the slug for the common "filename is the slug" case.
    let slug_placeholder = path
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // ── Effects: push buffer changes into current_doc.front_matter ────────
    create_effect(move |_| {
        let new_title = title_buf.get();
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                let next = if new_title.is_empty() { None } else { Some(new_title.clone()) };
                if d.front_matter.title != next {
                    d.front_matter.title = next;
                }
            }
        });
    });
    create_effect(move |_| {
        let new_slug = slug_buf.get();
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                let next = if new_slug.is_empty() { None } else { Some(new_slug.clone()) };
                if d.front_matter.slug != next {
                    d.front_matter.slug = next;
                }
            }
        });
    });
    create_effect(move |_| {
        let raw = date_buf.get();
        if raw.trim().is_empty() {
            date_invalid.set(false);
            current_doc.update(|maybe| {
                if let Some(d) = maybe {
                    if d.front_matter.date.is_some() {
                        d.front_matter.date = None;
                    }
                }
            });
            return;
        }
        match NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
            Ok(d) => {
                date_invalid.set(false);
                current_doc.update(|maybe| {
                    if let Some(doc) = maybe {
                        if doc.front_matter.date != Some(d) {
                            doc.front_matter.date = Some(d);
                        }
                    }
                });
            }
            Err(_) => {
                // Don't write through bad input; surface a hint instead.
                date_invalid.set(true);
            }
        }
    });
    create_effect(move |_| {
        let raw = tags_buf.get();
        let tags: Vec<String> = raw
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                if d.front_matter.tags != tags {
                    d.front_matter.tags = tags.clone();
                }
            }
        });
    });
    create_effect(move |_| {
        let v = draft_sig.get();
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                if d.front_matter.draft != v {
                    d.front_matter.draft = v;
                }
            }
        });
    });

    // ── Field widgets ────────────────────────────────────────────────────
    let title_field = field_row(
        "Title",
        text_input(title_buf).style(input_style).into_any(),
    );
    let slug_field = field_row(
        "Slug",
        text_input(slug_buf)
            .placeholder(slug_placeholder)
            .style(input_style)
            .into_any(),
    );
    let date_input = text_input(date_buf)
        .placeholder("YYYY-MM-DD")
        .style(input_style);
    let date_hint = label(move || {
        if date_invalid.get() {
            "invalid (use YYYY-MM-DD)".to_string()
        } else {
            String::new()
        }
    })
    .style(|s| s.font_size(11.).color(ERR_FG).padding_top(2.));
    let date_field = field_row("Date", v_stack((date_input, date_hint)).into_any());
    let tags_field = field_row(
        "Tags",
        text_input(tags_buf)
            .placeholder("comma, separated")
            .style(input_style)
            .into_any(),
    );
    let draft_field = field_row(
        "Draft",
        Checkbox::new_rw(draft_sig).into_any(),
    );

    v_stack((
        label(|| "Front matter".to_string()).style(|s| {
            s.font_size(12.)
                .font_weight(Weight::SEMIBOLD)
                .color(LABEL_FG)
                .padding_bottom(8.)
        }),
        title_field,
        slug_field,
        date_field,
        tags_field,
        draft_field,
    ))
    .style(|s| s.padding(12.).gap(10.).width_full())
    .into_any()
}

fn field_row(name: &'static str, control: AnyView) -> AnyView {
    v_stack((
        label(move || name.to_string()).style(|s| {
            s.font_size(11.)
                .color(LABEL_FG)
                .font_weight(Weight::SEMIBOLD)
        }),
        control,
    ))
    .style(|s| s.gap(4.).width_full())
    .into_any()
}

fn input_style(s: floem::style::Style) -> floem::style::Style {
    s.width_full()
        .padding_horiz(6.)
        .padding_vert(4.)
        .background(INPUT_BG)
        .border(1.)
        .border_color(INPUT_BORDER)
        .border_radius(3.)
        .font_size(13.)
}

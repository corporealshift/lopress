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
use floem::reactive::{create_effect, create_memo, RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{
    button, dyn_container, empty, h_stack, label, scroll, text_input, v_stack, Checkbox, Decorators,
};
use floem::{AnyView, IntoView};
use std::path::PathBuf;
use std::rc::Rc;

use crate::model::types::EditorDoc;

const PANE_WIDTH: f64 = 280.0;
const BG: Color = Color::rgb8(250, 250, 250);
const BORDER: Color = Color::rgb8(220, 220, 220);
const LABEL_FG: Color = Color::rgb8(110, 110, 120);
const ERR_FG: Color = Color::rgb8(170, 40, 40);
const INPUT_BG: Color = Color::rgb8(255, 255, 255);
const INPUT_BORDER: Color = Color::rgb8(210, 210, 215);

/// Build the inspector view. Empty placeholder when no doc is open.
///
/// `on_action` dispatches front-matter edits through the `BlockAction`
/// chokepoint so they are undoable.
pub fn inspector_view(
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    on_action: crate::ui::blocks::inline_editor::ActionSink,
) -> impl IntoView {
    let body = dyn_container(
        move || current_path.get(),
        move |path| match current_doc.with_untracked(|d| d.clone()) {
            Some(doc) => form(doc, path, current_doc, on_action.clone()).into_any(),
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
    on_action: crate::ui::blocks::inline_editor::ActionSink,
) -> AnyView {
    // Nested Fn trait bounds are inherent to the front-matter mutation API.
    #[allow(clippy::type_complexity)]
    let dispatch_fm_edit: Rc<dyn Fn(&dyn Fn(&mut lopress_core::FrontMatter))> = Rc::new({
        let on_action = on_action.clone();
        move |mutate: &dyn Fn(&mut lopress_core::FrontMatter)| {
            let current =
                current_doc.with_untracked(|d| d.as_ref().map(|doc| doc.front_matter.clone()));
            let Some(mut new_fm) = current else {
                return;
            };
            let before = new_fm.clone();
            mutate(&mut new_fm);
            if new_fm != before {
                on_action(crate::actions::BlockAction::EditFrontMatter {
                    new_front_matter: Box::new(new_fm),
                });
            }
        }
    });
    let fm = &doc.front_matter;
    let title_buf: RwSignal<String> = RwSignal::new(fm.title.clone().unwrap_or_default());
    let slug_buf: RwSignal<String> = RwSignal::new(fm.slug.clone().unwrap_or_default());
    let date_buf: RwSignal<String> = RwSignal::new(
        fm.date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
    );
    let tags_buf: RwSignal<String> = RwSignal::new(fm.tags.join(", "));
    let draft_sig: RwSignal<bool> = RwSignal::new(fm.draft);
    let date_invalid: RwSignal<bool> = RwSignal::new(false);
    let desc_buf: RwSignal<String> = RwSignal::new(fm.description.clone().unwrap_or_default());

    // Slug placeholder: file stem of the current path. Avoids forcing
    // authors to type the slug for the common "filename is the slug" case.
    let slug_placeholder = path
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // ── Effects: push buffer changes through BlockAction for undoability ──
    // Each effect calls `dispatch_fm_edit` which builds a new front matter
    // via a mutator closure and dispatches `EditFrontMatter` only when the
    // result differs from the current model state.
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let new_title = title_buf.get();
        dfe(&|fm| {
            fm.title = if new_title.is_empty() {
                None
            } else {
                Some(new_title.clone())
            };
        });
    });
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let new_slug = slug_buf.get();
        dfe(&|fm| {
            fm.slug = if new_slug.is_empty() {
                None
            } else {
                Some(new_slug.clone())
            };
        });
    });
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let raw = date_buf.get();
        if raw.trim().is_empty() {
            date_invalid.set(false);
            dfe(&|fm| {
                fm.date = None;
            });
            return;
        }
        match NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
            Ok(d) => {
                date_invalid.set(false);
                dfe(&|fm| {
                    fm.date = Some(d);
                });
            }
            Err(_) => {
                date_invalid.set(true);
            }
        }
    });
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let raw = tags_buf.get();
        let tags: Vec<String> = raw
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        dfe(&|fm| {
            fm.tags = tags.clone();
        });
    });
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let v = draft_sig.get();
        dfe(&|fm| {
            fm.draft = v;
        });
    });
    let dfe = Rc::clone(&dispatch_fm_edit);
    create_effect(move |_| {
        let new_desc = desc_buf.get();
        dfe(&|fm| {
            fm.description = if new_desc.is_empty() {
                None
            } else {
                Some(new_desc.clone())
            };
        });
    });

    // ── Field widgets ────────────────────────────────────────────────────
    let title_field = field_row("Title", text_input(title_buf).style(input_style).into_any());
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
    let draft_field = field_row("Draft", Checkbox::new_rw(draft_sig).into_any());

    // ── Title / H1 divergence warning ────────────────────────────────────
    let h1_text = create_memo(move |_| {
        current_doc.with(|maybe| {
            let d = maybe.as_ref()?;
            let h1 = d
                .blocks
                .iter()
                .find(|b| b.kind == crate::model::types::BlockKind::Heading(1))?;
            match &h1.body {
                crate::model::types::BlockBody::Inline(runs) => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                }
                _ => None,
            }
        })
    });
    let title_h1_mismatch = create_memo(move |_| {
        let title = current_doc.with(|d| d.as_ref().and_then(|d| d.front_matter.title.clone()));
        let h1 = h1_text.get();
        matches!((title, h1), (Some(t), Some(h)) if t != h)
    });

    let warning_row = {
        dyn_container(
            move || title_h1_mismatch.get(),
            move |mismatch| {
                if !mismatch {
                    return empty().into_any();
                }
                let dfe_sync = Rc::clone(&dispatch_fm_edit);
                let on_sync = move || {
                    if let Some(text) = h1_text.get_untracked() {
                        title_buf.set(text.clone());
                        dfe_sync(&|fm| {
                            fm.title = Some(text.clone());
                        });
                    }
                };
                h_stack((
                    label(|| "\u{26a0} Title differs from H1".to_string())
                        .style(|s| s.font_size(11.).color(ERR_FG).flex_grow(1.0)),
                    button(label(|| "Sync from H1".to_string()))
                        .action(on_sync)
                        .style(|s| s.font_size(11.).padding_horiz(6.).padding_vert(2.)),
                ))
                .style(|s| s.gap(4.).width_full())
                .into_any()
            },
        )
        .style(|s| s.width_full())
    };

    let desc_field = field_row(
        "Description",
        text_input(desc_buf)
            .placeholder("Short excerpt or summary")
            .style(input_style)
            .into_any(),
    );

    v_stack((
        label(|| "Front matter".to_string()).style(|s| {
            s.font_size(12.)
                .font_weight(Weight::SEMIBOLD)
                .color(LABEL_FG)
                .padding_bottom(8.)
        }),
        title_field,
        warning_row,
        slug_field,
        date_field,
        tags_field,
        draft_field,
        desc_field,
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

//! Navigation editor panel — working model and Floem view.
//!
//! The working model (`NavModel`) is a pure data structure independent of
//! Floem views. It manages a list of `NavRow` items with add/remove/reorder
//! operations. The Floem view (`nav_editor_view`) binds the model to inputs.

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::text::Weight;
use floem::views::{
    button, dyn_container, empty, h_stack, label, scroll, text_input, v_stack, v_stack_from_iter,
    Decorators,
};
use floem::{AnyView, IntoView};
use lopress_build::NavItem;

/// A workspace page offered by the "Link to page" picker.
#[derive(Debug, Clone)]
pub struct PageChoice {
    pub slug: String,
    pub title: String,
}

/// A tag offered by the "Link to tag" picker.
#[derive(Debug, Clone)]
pub struct TagChoice {
    pub name: String,
}

// ── Working model (pure data, testable without Floem) ───────────────────────

/// A single nav row in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavRow {
    pub label: String,
    pub href: String,
}

/// The pure working model for the nav editor panel.
///
/// This is the single source of truth for the panel's state. It is
/// initialized from `session.nav_items()` when the modal opens and is
/// used to build `Vec<NavItem>` on save.
#[derive(Debug, Clone)]
pub struct NavModel {
    pub rows: Vec<NavRow>,
}

impl NavModel {
    /// Create a new model from the current nav items.
    pub fn new(items: Vec<NavItem>) -> Self {
        Self {
            rows: items
                .into_iter()
                .map(|n| NavRow {
                    label: n.label,
                    href: n.href,
                })
                .collect(),
        }
    }

    /// Add an empty row at the end.
    pub fn add_row(&mut self) {
        self.rows.push(NavRow {
            label: String::new(),
            href: String::new(),
        });
    }

    /// Remove the row at the given index. No-op if out of bounds.
    pub fn remove_row(&mut self, index: usize) {
        if index < self.rows.len() {
            self.rows.remove(index);
        }
    }

    /// Move the row at `index` up by one position. No-op at index 0.
    pub fn move_up(&mut self, index: usize) {
        if index > 0 && index < self.rows.len() {
            self.rows.swap(index, index - 1);
        }
    }

    /// Move the row at `index` down by one position. No-op at the last index.
    pub fn move_down(&mut self, index: usize) {
        if index + 1 < self.rows.len() {
            self.rows.swap(index, index + 1);
        }
    }

    /// Update the label of the row at `index`. No-op if out of bounds.
    pub fn set_label(&mut self, index: usize, label: String) {
        if let Some(row) = self.rows.get_mut(index) {
            row.label = label;
        }
    }

    /// Update the href of the row at `index`. No-op if out of bounds.
    pub fn set_href(&mut self, index: usize, href: String) {
        if let Some(row) = self.rows.get_mut(index) {
            row.href = href;
        }
    }

    /// Fill the href (and label if empty) from a page slug. No-op if out of bounds.
    pub fn fill_href_from_page(&mut self, index: usize, slug: &str, title: &str) {
        if let Some(row) = self.rows.get_mut(index) {
            row.href = format!("/{slug}/");
            if row.label.is_empty() {
                row.label = title.to_string();
            }
        }
    }

    /// Fill the href (and label if empty) from a tag name. No-op if out of bounds.
    pub fn fill_href_from_tag(&mut self, index: usize, tag: &str) {
        if let Some(row) = self.rows.get_mut(index) {
            row.href = format!("/tags/{tag}/");
            if row.label.is_empty() {
                row.label = tag.to_string();
            }
        }
    }

    /// Convert the current rows to `NavItem`, dropping empty rows.
    pub fn to_nav_items(&self) -> Vec<NavItem> {
        self.rows
            .iter()
            .filter(|r| !r.label.is_empty() && !r.href.is_empty())
            .map(|r| NavItem {
                label: r.label.clone(),
                href: r.href.clone(),
            })
            .collect()
    }
}

// ── Floem view ──────────────────────────────────────────────────────────────

/// Build the nav-editor panel view.
///
/// `on_save` is called with the collected `Vec<NavItem>`; the caller decides
/// whether to close the modal (it stays open on a save error, which the
/// caller displays — see Task 10).
/// `on_cancel` closes the modal without saving.
///
/// `pages` and `tags` populate the "Link to page" / "Link to tag" pickers;
/// choosing one fills the last row's href (adding a row first if the list is
/// empty), and the row's label when it was blank.
pub fn nav_editor_view(
    model: RwSignal<NavModel>,
    pages: Vec<PageChoice>,
    tags: Vec<TagChoice>,
    on_save: impl Fn(Vec<NavItem>) + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl IntoView {
    let page_picker_open: RwSignal<bool> = RwSignal::new(false);
    let tag_picker_open: RwSignal<bool> = RwSignal::new(false);

    let save_btn = button(label(|| "Save".to_string()))
        .action(move || {
            let items = model.get_untracked().to_nav_items();
            on_save(items);
        })
        .style(|s| {
            s.padding_horiz(16.)
                .padding_vert(6.)
                .font_size(14.)
                .font_weight(Weight::SEMIBOLD)
        });

    let cancel_btn = button(label(|| "Cancel".to_string()))
        .action(on_cancel)
        .style(|s| s.padding_horiz(16.).padding_vert(6.).font_size(14.));

    let add_btn = button(label(|| "+ Add link".to_string()))
        .action(move || model.update(|m| m.add_row()))
        .style(|s| s.padding_vert(4.).font_size(12.));

    let page_btn = button(label(|| "Link to page \u{25be}".to_string())) // ▾
        .action(move || {
            tag_picker_open.set(false);
            page_picker_open.update(|v| *v = !*v);
        })
        .style(|s| s.padding_vert(4.).padding_horiz(8.).font_size(12.));
    let tag_btn = button(label(|| "Link to tag \u{25be}".to_string())) // ▾
        .action(move || {
            page_picker_open.set(false);
            tag_picker_open.update(|v| *v = !*v);
        })
        .style(|s| s.padding_vert(4.).padding_horiz(8.).font_size(12.));

    let top_row = h_stack((add_btn, page_btn, tag_btn)).style(|s| s.gap(6.).items_center());

    let page_popup = dyn_container(
        move || page_picker_open.get(),
        move |open| {
            if !open {
                return empty().into_any();
            }
            picker_popup(
                pages.iter().map(|p| p.title.clone()).collect(),
                pages.iter().map(|p| p.slug.clone()).collect(),
                move |slug, title| {
                    model.update(|m| {
                        if m.rows.is_empty() {
                            m.add_row();
                        }
                        let last = m.rows.len().saturating_sub(1);
                        m.fill_href_from_page(last, &slug, &title);
                    });
                    page_picker_open.set(false);
                },
            )
        },
    );

    let tag_popup = dyn_container(
        move || tag_picker_open.get(),
        move |open| {
            if !open {
                return empty().into_any();
            }
            picker_popup(
                tags.iter().map(|t| t.name.clone()).collect(),
                tags.iter().map(|t| t.name.clone()).collect(),
                move |tag: String, _label: String| {
                    model.update(|m| {
                        if m.rows.is_empty() {
                            m.add_row();
                        }
                        let last = m.rows.len().saturating_sub(1);
                        m.fill_href_from_tag(last, &tag);
                    });
                    tag_picker_open.set(false);
                },
            )
        },
    );

    let footer = h_stack((save_btn, cancel_btn)).style(|s| s.gap(8.).justify_end().padding_top(8.));

    v_stack((
        top_row,
        page_popup,
        tag_popup,
        scroll(
            dyn_container(
                move || model.get(),
                move |m| {
                    let total = m.rows.len();
                    let mut rows: Vec<AnyView> = Vec::with_capacity(total);
                    for (i, row) in m.rows.iter().enumerate() {
                        rows.push(nav_row_view(model, i, total, row));
                    }
                    v_stack_from_iter(rows).style(|s| s.gap(4.))
                },
            )
            .style(|s| s.min_height(150.).max_height(300.)),
        )
        .style(|s| s.flex_grow(1.)),
        footer,
    ))
    .style(|s| s.gap(8.).padding(16.).width(480.))
}

/// A popup list of choices for a picker. Each entry shows `labels[i]` and,
/// when clicked, calls `on_select(values[i], labels[i])`. Renders a "(none)"
/// placeholder when there are no choices.
fn picker_popup(
    labels: Vec<String>,
    values: Vec<String>,
    on_select: impl Fn(String, String) + Clone + 'static,
) -> AnyView {
    let mut entries: Vec<AnyView> = Vec::with_capacity(labels.len());
    for (lbl, val) in labels.into_iter().zip(values) {
        let on_sel = on_select.clone();
        let lbl_for_text = lbl.clone();
        let btn = button(label(move || lbl_for_text.clone()))
            .action(move || on_sel(val.clone(), lbl.clone()))
            .style(|s| {
                s.width_full()
                    .font_size(12.)
                    .padding_vert(2.)
                    .padding_horiz(6.)
            });
        entries.push(btn.into_any());
    }
    if entries.is_empty() {
        entries.push(
            label(|| "(none)".to_string())
                .style(|s| {
                    s.font_size(11.)
                        .padding(4.)
                        .color(Color::rgb8(150, 150, 160))
                })
                .into_any(),
        );
    }
    v_stack_from_iter(entries)
        .style(|s| {
            s.gap(2.)
                .border(1.)
                .border_color(Color::rgb8(210, 210, 210))
                .border_radius(4.)
                .padding(4.)
                .max_height(160.)
        })
        .into_any()
}

/// A single nav row view (label input + href input + reorder/remove controls).
///
/// Edits commit to `model` on focus loss; the reorder/remove buttons mutate
/// `model` directly by index.
fn nav_row_view(model: RwSignal<NavModel>, index: usize, total: usize, row: &NavRow) -> AnyView {
    let label_buf: RwSignal<String> = RwSignal::new(row.label.clone());
    let href_buf: RwSignal<String> = RwSignal::new(row.href.clone());

    let label_input = text_input(label_buf)
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            model.update(|m| m.set_label(index, label_buf.get_untracked()));
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.min_width(120.).font_size(12.));

    let href_input = text_input(href_buf)
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            model.update(|m| m.set_href(index, href_buf.get_untracked()));
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.min_width(120.).font_size(12.));

    let up_btn = button(label(|| "\u{2191}".to_string())) // ↑
        .action(move || model.update(|m| m.move_up(index)))
        .disabled(move || index == 0)
        .style(|s| s.padding(4.).font_size(12.));

    let down_btn = button(label(|| "\u{2193}".to_string())) // ↓
        .action(move || model.update(|m| m.move_down(index)))
        .disabled(move || index + 1 >= total)
        .style(|s| s.padding(4.).font_size(12.));

    let remove_btn = button(label(|| "\u{2715}".to_string())) // ✕
        .action(move || model.update(|m| m.remove_row(index)))
        .style(|s| s.padding(4.).font_size(12.).color(Color::rgb8(200, 60, 60)));

    let controls = h_stack((up_btn, down_btn, remove_btn)).style(|s| s.gap(2.).items_center());

    v_stack((label_input, href_input, controls))
        .style(|s| {
            s.gap(4.)
                .padding(6.)
                .border(1.)
                .border_color(Color::rgb8(220, 220, 220))
                .border_radius(4.)
        })
        .into_any()
}

// ── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a model directly from rows (test convenience; production code
    /// uses `NavModel::new` with persisted `NavItem`s).
    fn model_from_rows(rows: &[(&str, &str)]) -> NavModel {
        NavModel {
            rows: rows
                .iter()
                .map(|(l, h)| NavRow {
                    label: (*l).to_string(),
                    href: (*h).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn add_row_appends_empty_row() {
        let mut model = NavModel::new(vec![]);
        model.add_row();
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.rows[0].label, "");
        assert_eq!(model.rows[0].href, "");
    }

    #[test]
    fn remove_row_at_index() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/")]);
        model.remove_row(0);
        assert_eq!(model.rows.len(), 1);
        assert_eq!(model.rows[0].label, "B");
    }

    #[test]
    fn move_up_at_index_1_moves_to_0() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/"), ("C", "/c/")]);
        model.move_up(2); // C moves up
        assert_eq!(model.rows[1].label, "C");
        assert_eq!(model.rows[2].label, "B");
    }

    #[test]
    fn move_up_at_index_0_does_nothing() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/")]);
        model.move_up(0);
        assert_eq!(model.rows[0].label, "A");
    }

    #[test]
    fn move_down_at_index_0_moves_to_1() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/")]);
        model.move_down(0);
        assert_eq!(model.rows[1].label, "A");
    }

    #[test]
    fn move_down_at_last_index_does_nothing() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/")]);
        model.move_down(1);
        assert_eq!(model.rows[1].label, "B");
    }

    #[test]
    fn to_nav_items_drops_empty_rows() {
        let model = model_from_rows(&[("A", "/a/"), ("", "/empty/"), ("B", "")]);
        let items = model.to_nav_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "A");
    }

    #[test]
    fn fill_href_from_page() {
        let mut model = model_from_rows(&[("", "")]);
        model.fill_href_from_page(0, "about", "About Page");
        assert_eq!(model.rows[0].href, "/about/");
        assert_eq!(model.rows[0].label, "About Page"); // pre-filled because label was empty
    }

    #[test]
    fn fill_href_from_tag() {
        let mut model = model_from_rows(&[("", "")]);
        model.fill_href_from_tag(0, "rust");
        assert_eq!(model.rows[0].href, "/tags/rust/");
        assert_eq!(model.rows[0].label, "rust"); // pre-filled because label was empty
    }

    #[test]
    fn fill_href_from_page_does_not_overwrite_existing_label() {
        let mut model = model_from_rows(&[("Custom", "")]);
        model.fill_href_from_page(0, "about", "About Page");
        assert_eq!(model.rows[0].href, "/about/");
        assert_eq!(model.rows[0].label, "Custom"); // label preserved
    }

    #[test]
    fn fill_href_from_tag_does_not_overwrite_existing_label() {
        let mut model = model_from_rows(&[("My Tags", "")]);
        model.fill_href_from_tag(0, "rust");
        assert_eq!(model.rows[0].href, "/tags/rust/");
        assert_eq!(model.rows[0].label, "My Tags"); // label preserved
    }

    #[test]
    fn new_converts_nav_items_to_rows() {
        let items = vec![
            NavItem {
                label: "Home".into(),
                href: "/".into(),
            },
            NavItem {
                label: "About".into(),
                href: "/about/".into(),
            },
        ];
        let model = NavModel::new(items);
        assert_eq!(model.rows.len(), 2);
        assert_eq!(model.rows[0].label, "Home");
        assert_eq!(model.rows[0].href, "/");
        assert_eq!(model.rows[1].label, "About");
        assert_eq!(model.rows[1].href, "/about/");
    }

    #[test]
    fn to_nav_items_empty_model_returns_empty_vec() {
        let model = NavModel::new(vec![]);
        assert!(model.to_nav_items().is_empty());
    }

    #[test]
    fn to_nav_items_keeps_valid_rows() {
        let model = model_from_rows(&[("Home", "/"), ("About", "/about/")]);
        let items = model.to_nav_items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "Home");
        assert_eq!(items[1].label, "About");
    }

    #[test]
    fn remove_row_removes_middle() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/"), ("C", "/c/")]);
        model.remove_row(1);
        assert_eq!(model.rows.len(), 2);
        assert_eq!(model.rows[0].label, "A");
        assert_eq!(model.rows[1].label, "C");
    }

    #[test]
    fn move_up_and_move_down_restore_order() {
        let mut model = model_from_rows(&[("A", "/a/"), ("B", "/b/"), ("C", "/c/")]);
        model.move_up(2); // C at index 1
        assert_eq!(model.rows[0].label, "A");
        assert_eq!(model.rows[1].label, "C");
        assert_eq!(model.rows[2].label, "B");
        model.move_down(1); // C back to index 2
        assert_eq!(model.rows[0].label, "A");
        assert_eq!(model.rows[1].label, "B");
        assert_eq!(model.rows[2].label, "C");
    }
}

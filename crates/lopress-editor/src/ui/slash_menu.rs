//! Slash command popup. Triggered by `BlockAction::OpenSlashMenu` from the
//! inline editor when `/` is typed in an empty paragraph; lets the user pick
//! a block kind to convert the current block into, or insert a plugin block
//! (e.g. the read-more marker).
//!
//! Typing after `/` fuzzy-filters the list (`fuzzy_filter`); the popup owns the
//! query string and a highlighted-index `RwSignal<usize>`, and handles printable
//! keys (extend the query), Backspace (shrink it / dismiss when empty), Up/Down
//! (move the highlight within the filtered list), Enter (confirm), and Escape
//! (dismiss). Confirmation calls `on_select(choice)`; any close path calls
//! `on_close()` so the editor pane can clear its `slash_menu_open` flag.

use crate::model::descriptor;
use floem::event::{Event, EventListener, EventPropagation};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate};
use floem::views::{dyn_container, label, v_stack, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView, View};
use std::rc::Rc;

const HIGHLIGHT_BG: Color = Color::rgb8(210, 220, 240);
const POPUP_BG: Color = Color::rgb8(252, 252, 254);
const BORDER: Color = Color::rgb8(210, 210, 215);
const MUTED: Color = Color::rgb8(140, 140, 150);

/// A slash-menu selection.
#[derive(Debug, Clone)]
pub enum SlashChoice {
    /// Change the current block to the given editor key with the given attrs.
    ChangeType {
        new_editor: Rc<str>,
        attrs: serde_json::Map<String, serde_json::Value>,
    },
    /// Insert the read-more marker.
    ReadMore,
    /// Insert an image block.
    Image,
    /// Insert a separator block.
    Separator,
    /// Insert a table block.
    Table,
    /// Insert a plugin block.
    Plugin { type_name: Rc<str> },
}

/// The choices offered by the slash menu, in display order.
///
/// # Panics
///
/// Panics if any built-in descriptor's `default_block` produces a block
/// without `PluginMeta` or without `editor: Some` — this is a programming
/// error since all built-in constructors stamp plugin meta.
#[allow(clippy::unwrap_used)] // safe: built-in PluginMeta always has editor: Some
pub fn slash_menu_items() -> Vec<(String, SlashChoice)> {
    let mut items: Vec<(String, SlashChoice)> = descriptor::slash_menu_entries()
        .iter()
        .map(|(label, _default_block_fn)| {
            // Build SlashChoice directly from known descriptor info.
            // Image/Separator/Table insert new blocks rather than changing
            // the current block, so they use dedicated variants.
            let choice = match *label {
                "Paragraph" => {
                    let meta = crate::model::types::PluginMeta::paragraph();
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Heading 1" => {
                    let meta = crate::model::types::PluginMeta::heading(1);
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Heading 2" => {
                    let meta = crate::model::types::PluginMeta::heading(2);
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Heading 3" => {
                    let meta = crate::model::types::PluginMeta::heading(3);
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Code block" => {
                    let meta = crate::model::types::PluginMeta::code("");
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Unordered list" => {
                    let meta = crate::model::types::PluginMeta::list(false);
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Ordered list" => {
                    let meta = crate::model::types::PluginMeta::list(true);
                    SlashChoice::ChangeType {
                        new_editor: meta.editor.unwrap(),
                        attrs: meta.attrs,
                    }
                }
                "Image" => SlashChoice::Image,
                "Separator" => SlashChoice::Separator,
                "Table" => SlashChoice::Table,
                _ => SlashChoice::ChangeType {
                    new_editor: Rc::from(descriptor::EDITOR_PARAGRAPH),
                    attrs: serde_json::Map::new(),
                },
            };
            (label.to_string(), choice)
        })
        .collect();
    // "Read more" is not in the descriptor menu (it's inserted by a dedicated
    // affordance), but it's still part of the slash menu. Insert it after
    // Image (index 7) and before Separator (index 8).
    items.insert(8, ("Read more".to_string(), SlashChoice::ReadMore));
    items
}

/// Fuzzy-match `query` against `label`, returning a relevance score (higher is
/// better) or `None` when the query is not a subsequence of the label.
///
/// Matching is case-insensitive and subsequence-based: every query character
/// must appear in `label` in order. Contiguous matches and a match at the very
/// start of the label score higher, so `"head"` ranks "Heading" above a label
/// where the same letters are scattered.
fn fuzzy_score(label: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    let mut qi = 0usize;
    let mut score = 0i32;
    let mut consecutive = 0i32;
    let mut prev_match = false;
    for (i, raw) in label.chars().enumerate() {
        let Some(&target) = q.get(qi) else {
            break;
        };
        if raw.to_ascii_lowercase() == target {
            score += 1;
            if prev_match {
                consecutive += 1;
                score += consecutive * 2;
            } else {
                consecutive = 0;
            }
            if i == 0 {
                score += 3;
            }
            qi += 1;
            prev_match = true;
        } else {
            prev_match = false;
        }
    }
    if qi == q.len() {
        Some(score)
    } else {
        None
    }
}

/// Filter and rank `items` by fuzzy relevance to `query`. An empty query
/// returns every item in its original order; otherwise only matching items are
/// returned, sorted by descending score (ties keep their original order).
fn fuzzy_filter(items: Vec<(String, SlashChoice)>, query: &str) -> Vec<(String, SlashChoice)> {
    if query.is_empty() {
        return items;
    }
    let mut scored: Vec<(i32, usize, (String, SlashChoice))> = items
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| fuzzy_score(&item.0, query).map(|s| (s, idx, item)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

/// Build the slash menu popup. `items` supplies the choices (enabling
/// per-document filtering such as omitting "Read more" when a marker
/// already exists). `on_select` fires with the chosen `SlashChoice` on
/// Enter or click; `on_close` fires whenever the popup should be dismissed.
pub fn slash_menu<F, C>(
    items: Vec<(String, SlashChoice)>,
    on_select: F,
    on_close: C,
) -> impl IntoView
where
    F: Fn(SlashChoice) + Clone + 'static,
    C: Fn() + Clone + 'static,
{
    let query: RwSignal<String> = RwSignal::new(String::new());
    let highlight: RwSignal<usize> = RwSignal::new(0);

    // A subtle header echoing the typed query, so the user has feedback (the
    // `/` and the characters are consumed by the popup, not shown in the block).
    let query_header = label(move || {
        let q = query.get();
        if q.is_empty() {
            "Type to filter…".to_string()
        } else {
            format!("/{q}")
        }
    })
    .style(|s| {
        s.padding_horiz(8.)
            .padding_vert(4.)
            .font_size(11.)
            .color(MUTED)
    });

    // Rows rebuild whenever the query changes; each row's highlight styling is
    // independently reactive, so Up/Down restyle without a rebuild.
    let items_for_rows = items.clone();
    let on_select_rows = on_select.clone();
    let on_close_rows = on_close.clone();
    let rows_view = dyn_container(
        move || query.get(),
        move |_q| {
            let filtered = fuzzy_filter(items_for_rows.clone(), &query.get_untracked());
            if filtered.is_empty() {
                return label(|| "No matching blocks".to_string())
                    .style(|s| s.padding_horiz(8.).padding_vert(4.).color(MUTED))
                    .into_any();
            }
            let mut rows: Vec<AnyView> = Vec::with_capacity(filtered.len());
            for (i, (lbl, choice)) in filtered.into_iter().enumerate() {
                let lbl_owned = lbl.clone();
                let on_select_for_row = on_select_rows.clone();
                let on_close_for_row = on_close_rows.clone();
                let choice_for_row = choice.clone();
                let row = label(move || lbl_owned.clone())
                    .on_click_stop(move |_| {
                        on_select_for_row(choice_for_row.clone());
                        on_close_for_row();
                    })
                    .on_event(EventListener::PointerEnter, move |_| {
                        highlight.set(i);
                        EventPropagation::Continue
                    })
                    .style(move |s| {
                        let s = s.padding_horiz(8.).padding_vert(4.).width_full();
                        if highlight.get() == i {
                            s.background(HIGHLIGHT_BG)
                        } else {
                            s
                        }
                    });
                rows.push(row.into_any());
            }
            v_stack_from_iter(rows).into_any()
        },
    );

    let items_for_key = items;
    let on_select_for_key = on_select;
    let on_close_for_key = on_close;
    let popup = v_stack((query_header, rows_view))
        .keyboard_navigable()
        .on_event(EventListener::KeyDown, move |e| {
            let Event::KeyDown(ke) = e else {
                return EventPropagation::Continue;
            };
            match &ke.key.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    let n = fuzzy_filter(items_for_key.clone(), &query.get()).len();
                    if n > 0 {
                        highlight.update(|h| *h = (*h + 1) % n);
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let n = fuzzy_filter(items_for_key.clone(), &query.get()).len();
                    if n > 0 {
                        highlight.update(|h| *h = if *h == 0 { n - 1 } else { *h - 1 });
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Enter) => {
                    let filtered = fuzzy_filter(items_for_key.clone(), &query.get());
                    if let Some((_, choice)) = filtered.get(highlight.get()) {
                        on_select_for_key(choice.clone());
                    }
                    on_close_for_key();
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Escape) => {
                    on_close_for_key();
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Backspace) => {
                    if query.get().is_empty() {
                        // Backspacing past the `/` dismisses the menu.
                        on_close_for_key();
                    } else {
                        query.update(|q| {
                            q.pop();
                        });
                        highlight.set(0);
                    }
                    EventPropagation::Stop
                }
                Key::Character(s) => {
                    let mut changed = false;
                    query.update(|q| {
                        for c in s.chars() {
                            if !c.is_control() {
                                q.push(c);
                                changed = true;
                            }
                        }
                    });
                    if changed {
                        highlight.set(0);
                    }
                    EventPropagation::Stop
                }
                _ => EventPropagation::Continue,
            }
        })
        .style(|s| {
            s.background(POPUP_BG)
                .border(1.)
                .border_color(BORDER)
                .border_radius(4.)
                .min_width(200.)
        });

    let popup_id = popup.id();
    create_effect(move |_| {
        popup_id.request_focus();
    });

    popup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(String, SlashChoice)> {
        vec![
            (
                "Paragraph".to_string(),
                SlashChoice::ChangeType {
                    new_editor: Rc::from("paragraph"),
                    attrs: serde_json::Map::new(),
                },
            ),
            (
                "Heading 1".to_string(),
                SlashChoice::ChangeType {
                    new_editor: Rc::from("heading"),
                    attrs: {
                        let mut m = serde_json::Map::new();
                        m.insert("level".into(), 1u64.into());
                        m
                    },
                },
            ),
            (
                "Unordered list".to_string(),
                SlashChoice::ChangeType {
                    new_editor: Rc::from("list"),
                    attrs: {
                        let mut m = serde_json::Map::new();
                        m.insert("ordered".into(), false.into());
                        m
                    },
                },
            ),
            (
                "Ordered list".to_string(),
                SlashChoice::ChangeType {
                    new_editor: Rc::from("list"),
                    attrs: {
                        let mut m = serde_json::Map::new();
                        m.insert("ordered".into(), true.into());
                        m
                    },
                },
            ),
            (
                "Callout".to_string(),
                SlashChoice::Plugin {
                    type_name: Rc::from("lopress:callout"),
                },
            ),
        ]
    }

    fn labels(v: &[(String, SlashChoice)]) -> Vec<&str> {
        v.iter().map(|(l, _)| l.as_str()).collect()
    }

    #[test]
    fn slash_menu_labels_match_hardcoded_order() {
        // The descriptor-projected slash menu must reproduce today's hardcoded
        // label sequence exactly. This pins the menu order so future changes
        // are visible.
        let items = slash_menu_items();
        let labels: Vec<&str> = items.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Paragraph",
                "Heading 1",
                "Heading 2",
                "Heading 3",
                "Code block",
                "Unordered list",
                "Ordered list",
                "Image",
                "Read more",
                "Separator",
                "Table",
            ]
        );
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let out = fuzzy_filter(items(), "");
        assert_eq!(
            labels(&out),
            vec![
                "Paragraph",
                "Heading 1",
                "Unordered list",
                "Ordered list",
                "Callout"
            ]
        );
    }

    #[test]
    fn matches_subsequence_case_insensitive() {
        // "cal" and "CAL" both match "Callout"; "clt" matches as a subsequence.
        for q in ["cal", "CAL", "clt"] {
            let out = fuzzy_filter(items(), q);
            assert_eq!(labels(&out), vec!["Callout"], "query {q:?}");
        }
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(fuzzy_filter(items(), "zzz").is_empty());
    }

    #[test]
    fn start_and_contiguous_rank_higher() {
        // "h" matches "Heading 1" (start) above "Paragraph" (h is mid-word).
        let out = fuzzy_filter(items(), "h");
        let l = labels(&out);
        let h = l.iter().position(|s| *s == "Heading 1");
        let p = l.iter().position(|s| *s == "Paragraph");
        assert!(h.is_some(), "Heading should match 'h': {l:?}");
        assert!(p.is_some(), "Paragraph should match 'h': {l:?}");
        assert!(h < p, "Heading should rank above Paragraph for 'h': {l:?}");
    }

    #[test]
    fn subsequence_matches_both_lists() {
        // "ol" is a subsequence of both list labels; ordered scores higher
        // (its `o` is at the start).
        let out = fuzzy_filter(items(), "ol");
        let l = labels(&out);
        assert!(l.contains(&"Ordered list"), "{l:?}");
        assert!(l.contains(&"Unordered list"), "{l:?}");
        assert_eq!(l.first(), Some(&"Ordered list"), "{l:?}");
    }
}

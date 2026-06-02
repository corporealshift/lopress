//! Slash command popup. Triggered by `BlockAction::OpenSlashMenu` from the
//! inline editor when `/` is typed in an empty paragraph; lets the user pick
//! a block kind to convert the current block into, or insert a plugin block
//! (e.g. the read-more marker).
//!
//! The popup owns a highlighted-index `RwSignal<usize>` and handles Up/Down
//! to move it, Enter to confirm, and Escape to dismiss. Confirmation calls
//! `on_select(choice)`; any close path (Escape, blur, confirmation) calls
//! `on_close()` so the editor pane can clear its `slash_menu_open` flag.

use crate::model::types::BlockKind;
use floem::event::{Event, EventListener, EventPropagation};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate};
use floem::views::{label, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView, View};
use std::rc::Rc;

const HIGHLIGHT_BG: Color = Color::rgb8(210, 220, 240);
const POPUP_BG: Color = Color::rgb8(252, 252, 254);
const BORDER: Color = Color::rgb8(210, 210, 215);

/// A slash-menu selection: either convert the current block to a built-in
/// kind, or insert a plugin block.
#[derive(Debug, Clone, PartialEq)]
pub enum SlashChoice {
    Kind(BlockKind),
    ReadMore,
    Image,
}

/// The choices offered by the slash menu, in display order.
pub fn slash_menu_items() -> Vec<(&'static str, SlashChoice)> {
    vec![
        ("Paragraph", SlashChoice::Kind(BlockKind::Paragraph)),
        ("Heading 1", SlashChoice::Kind(BlockKind::Heading(1))),
        ("Heading 2", SlashChoice::Kind(BlockKind::Heading(2))),
        ("Heading 3", SlashChoice::Kind(BlockKind::Heading(3))),
        (
            "Code block",
            SlashChoice::Kind(BlockKind::Code { lang: Rc::from("") }),
        ),
        (
            "Unordered list",
            SlashChoice::Kind(BlockKind::List { ordered: false }),
        ),
        (
            "Ordered list",
            SlashChoice::Kind(BlockKind::List { ordered: true }),
        ),
        ("Image", SlashChoice::Image),
        ("Read more", SlashChoice::ReadMore),
    ]
}

/// Build the slash menu popup. `items` supplies the choices (enabling
/// per-document filtering such as omitting "Read more" when a marker
/// already exists). `on_select` fires with the chosen `SlashChoice` on
/// Enter or click; `on_close` fires whenever the popup should be dismissed.
pub fn slash_menu<F, C>(
    items: Vec<(&'static str, SlashChoice)>,
    on_select: F,
    on_close: C,
) -> impl IntoView
where
    F: Fn(SlashChoice) + Clone + 'static,
    C: Fn() + Clone + 'static,
{
    let len = items.len();
    let highlight: RwSignal<usize> = RwSignal::new(0);

    let items_for_key: Vec<_> = items.clone();
    let mut rows: Vec<AnyView> = Vec::with_capacity(len);
    for (i, (lbl, choice)) in items.into_iter().enumerate() {
        let lbl_owned = lbl.to_string();
        let on_select_for_row = on_select.clone();
        let on_close_for_row = on_close.clone();
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

    let on_select_for_key = on_select;
    let on_close_for_key = on_close.clone();
    let popup = v_stack_from_iter(rows)
        .keyboard_navigable()
        .on_event(EventListener::KeyDown, move |e| {
            let Event::KeyDown(ke) = e else {
                return EventPropagation::Continue;
            };
            match ke.key.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    highlight.update(|h| *h = (*h + 1) % len);
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowUp) => {
                    highlight.update(|h| *h = if *h == 0 { len - 1 } else { *h - 1 });
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Enter) => {
                    let idx = highlight.get();
                    if let Some((_, choice)) = items_for_key.get(idx) {
                        on_select_for_key(choice.clone());
                    }
                    on_close_for_key();
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Escape) => {
                    on_close_for_key();
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

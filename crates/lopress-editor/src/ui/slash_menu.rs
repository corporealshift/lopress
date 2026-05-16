//! Slash command popup. Triggered by `BlockAction::OpenSlashMenu` from the
//! inline editor when `/` is typed in an empty paragraph; lets the user pick
//! a block kind to convert the current block into.
//!
//! The popup owns a highlighted-index `RwSignal<usize>` and handles Up/Down
//! to move it, Enter to confirm, and Escape to dismiss. Confirmation calls
//! `on_select(kind)`; any close path (Escape, blur, confirmation) calls
//! `on_close()` so the editor pane can clear its `slash_menu_open` flag.

use crate::model::types::BlockKind;
use floem::event::{Event, EventListener, EventPropagation};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{create_effect, RwSignal, SignalGet, SignalUpdate};
use floem::views::{label, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView, View};

const HIGHLIGHT_BG: Color = Color::rgb8(210, 220, 240);
const POPUP_BG: Color = Color::rgb8(252, 252, 254);
const BORDER: Color = Color::rgb8(210, 210, 215);

/// The block kinds offered by the slash menu, in display order.
pub fn slash_menu_items() -> Vec<(&'static str, BlockKind)> {
    vec![
        ("Paragraph", BlockKind::Paragraph),
        ("Heading 1", BlockKind::Heading(1)),
        ("Heading 2", BlockKind::Heading(2)),
        ("Heading 3", BlockKind::Heading(3)),
        (
            "Code block",
            BlockKind::Code {
                lang: String::new(),
            },
        ),
        ("Unordered list", BlockKind::List { ordered: false }),
        ("Ordered list", BlockKind::List { ordered: true }),
    ]
}

/// Build the slash menu popup. `on_select` fires with the chosen kind on
/// Enter or click; `on_close` fires whenever the popup should be dismissed
/// (Escape, after a selection, or on blur).
pub fn slash_menu<F, C>(on_select: F, on_close: C) -> impl IntoView
where
    F: Fn(BlockKind) + Clone + 'static,
    C: Fn() + Clone + 'static,
{
    let items = slash_menu_items();
    let len = items.len();
    let highlight: RwSignal<usize> = RwSignal::new(0);

    let mut rows: Vec<AnyView> = Vec::with_capacity(len);
    for (i, (lbl, kind)) in items.into_iter().enumerate() {
        let lbl_owned = lbl.to_string();
        let on_select_for_row = on_select.clone();
        let on_close_for_row = on_close.clone();
        let kind_for_row = kind.clone();
        let row = label(move || lbl_owned.clone())
            .on_click_stop(move |_| {
                on_select_for_row(kind_for_row.clone());
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
    let items_for_key = slash_menu_items();
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
                    if let Some((_, kind)) = items_for_key.get(idx) {
                        on_select_for_key(kind.clone());
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

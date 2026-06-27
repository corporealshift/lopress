//! Pane-level link-URL editor bar.
//!
//! This lives OUTSIDE the editor pane's rebuilding `dyn_container`, so it
//! survives the pane rebuild that a focus-loss commit triggers. The toolbar
//! Link button and Ctrl+K open it via `inline_editor::open_link_editor`, which
//! captures the selection's byte range up front. The bar then applies (or
//! removes) the link on that captured range in the model via `EditBlockBody`,
//! independent of the live editor's focus/selection state.

use crate::actions::BlockAction;
use crate::model::sync::set_link_on_range;
use crate::model::types::{BlockBody, BlockId, EditorDoc};
use crate::ui::blocks::inline_editor::ActionSink;
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::style::Position;
use floem::views::{button, dyn_container, empty, h_stack, label, text_input, Decorators};
use floem::IntoView;
use std::rc::Rc;

/// Height of the footer the bar sits just above (mirrors `footer::FOOTER_HEIGHT`).
const FOOTER_HEIGHT: f64 = 28.0;
/// Fixed height of the link bar itself.
const BAR_HEIGHT: f64 = 38.0;

/// An in-progress link edit: which block, which byte range within that block's
/// flattened inline text, and the URL buffer seed. Captured at the moment the
/// user invokes "add link" so it survives the editor-pane rebuild.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkEdit {
    pub block_id: BlockId,
    pub start: usize,
    pub end: usize,
    pub url: String,
}

/// The pane-level link bar. Renders nothing when `link_edit` is `None`;
/// otherwise a URL input with Apply / Remove / Cancel. Apply/Remove rewrite the
/// captured range in the model and refocus the block; all three close the bar.
pub fn link_bar_view(
    link_edit: RwSignal<Option<LinkEdit>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
) -> impl IntoView {
    dyn_container(
        move || link_edit.get(),
        move |maybe| match maybe {
            None => empty().into_any(),
            Some(edit) => {
                let url_buf = RwSignal::new(edit.url.clone());

                // Apply `url` (Some = set, None = remove) to the captured range
                // in the model, refocus the block, and close the bar.
                let on_action = on_action.clone();
                let commit: Rc<dyn Fn(Option<String>)> = Rc::new(move |url: Option<String>| {
                    let block_id = edit.block_id;
                    let new_runs = current_doc.with_untracked(|d| {
                        let doc = d.as_ref()?;
                        let block = doc.blocks.iter().find(|b| b.id == block_id)?;
                        match &block.body {
                            BlockBody::Inline(runs) => {
                                Some(set_link_on_range(runs, edit.start, edit.end, url.clone()))
                            }
                            // Links inside list items / other body shapes aren't
                            // supported yet; leave the doc untouched.
                            _ => None,
                        }
                    });
                    if let Some(runs) = new_runs {
                        on_action(BlockAction::EditBlockBody {
                            block_id,
                            new_body: Box::new(BlockBody::Inline(runs)),
                            built_in: true,
                        });
                        focus_target.set(Some(block_id));
                    }
                    link_edit.set(None);
                });

                let commit_apply = Rc::clone(&commit);
                let apply = move || {
                    let u = url_buf.get_untracked();
                    let u = u.trim();
                    commit_apply(if u.is_empty() {
                        None
                    } else {
                        Some(u.to_string())
                    });
                };
                let apply_for_key = apply.clone();
                let commit_remove = Rc::clone(&commit);
                let remove = move || commit_remove(None);
                let cancel = move || link_edit.set(None);

                h_stack((
                    label(|| "Link URL".to_string()).style(|s| s.font_size(13.).margin_right(4.)),
                    text_input(url_buf)
                        .placeholder("https://\u{2026}")
                        .on_event_stop(EventListener::KeyDown, move |e: &Event| {
                            if let Event::KeyDown(k) = e {
                                if matches!(k.key.logical_key, Key::Named(NamedKey::Enter)) {
                                    apply_for_key();
                                }
                            }
                        })
                        // `width_full()` is load-bearing: without an explicit
                        // width, floem's text_input treats the width as `Auto`
                        // and clips the *visible* text to a fixed char-count
                        // target, so a pre-filled URL shows only its first ~20
                        // chars even though the box fills the bar. A percentage
                        // width makes the clip track the real (flex-grown) width.
                        .style(|s| s.flex_grow(1.0).width_full().font_size(13.)),
                    button(label(|| "Apply".to_string()))
                        .on_event_stop(EventListener::PointerDown, move |_| apply()),
                    button(label(|| "Remove".to_string()))
                        .on_event_stop(EventListener::PointerDown, move |_| remove()),
                    button(label(|| "Cancel".to_string()))
                        .on_event_stop(EventListener::PointerDown, move |_| cancel()),
                ))
                .style(|s| {
                    s.width_full()
                        .height(BAR_HEIGHT)
                        .gap(6.)
                        .items_center()
                        .padding_horiz(10.)
                        .background(Color::rgb8(250, 250, 252))
                        .border_top(1.)
                        .border_color(Color::rgb8(220, 220, 226))
                })
                .into_any()
            }
        },
    )
    // Absolutely positioned, pinned just above the footer, so opening/closing
    // the bar never changes the editor pane's height (which would reflow block
    // layout) and the bar can't be squished or pushed off-screen. Mirrors the
    // nav-modal / slash-menu overlay pattern. When `link_edit` is `None` the
    // inner `empty()` collapses to zero size and intercepts nothing.
    .style(|s| {
        s.position(Position::Absolute)
            .inset_left(0.)
            .inset_right(0.)
            .inset_bottom(FOOTER_HEIGHT)
    })
}

//! In-app modal for questions about unsaved work.
//!
//! Drawn over the whole window like the site settings modal. A native dialog
//! would block the event loop and couldn't follow the app's theme.

use crate::ui::theme;
use floem::event::EventListener;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::style::Position;
use floem::unit::PxPctAuto;
use floem::views::{button, dyn_container, empty, h_stack_from_iter, label, v_stack, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// One button in the dialog. The dialog closes before `run` is called.
#[derive(Clone)]
pub struct ConfirmChoice {
    pub label: &'static str,
    pub run: Rc<dyn Fn()>,
}

impl ConfirmChoice {
    pub fn new(label: &'static str, run: impl Fn() + 'static) -> Self {
        Self {
            label,
            run: Rc::new(run),
        }
    }

    /// A button that only closes the dialog.
    pub fn cancel() -> Self {
        Self::new("Cancel", || {})
    }
}

/// A question and its answers.
#[derive(Clone)]
pub struct ConfirmRequest {
    pub message: &'static str,
    pub choices: Vec<ConfirmChoice>,
}

/// The dialog on screen, if any.
pub type ConfirmSlot = RwSignal<Option<ConfirmRequest>>;

/// Build the dialog layer. Mount it last, so it sits on top.
pub fn confirm_dialog_view(slot: ConfirmSlot) -> impl IntoView {
    dyn_container(
        move || slot.get(),
        move |request| match request {
            Some(request) => dialog(slot, request),
            None => empty().into_any(),
        },
    )
    // Full height only while open: the closed layer must not catch clicks.
    .style(move |s| {
        s.position(Position::Absolute)
            .inset_top(0.)
            .inset_left(0.)
            .width_full()
            .apply_if(slot.with(Option::is_some), |s| s.height_full())
    })
}

fn dialog(slot: ConfirmSlot, request: ConfirmRequest) -> AnyView {
    let buttons = request.choices.into_iter().map(|choice| {
        let ConfirmChoice { label: text, run } = choice;
        button(label(move || text.to_string()))
            .action(move || {
                slot.set(None);
                run();
            })
            .into_any()
    });
    let message = request.message;
    let panel = v_stack((
        label(move || message.to_string()).style(|s| s.font_size(theme::UI).color(theme::TEXT)),
        h_stack_from_iter(buttons).style(|s| s.gap(8.).justify_end().width_full()),
    ))
    .style(|s| {
        theme::floating(s)
            .gap(theme::SPACE_16)
            .width(380.)
            .padding(theme::SPACE_16)
            .margin_top(120.)
            .margin_horiz(PxPctAuto::Auto)
    });
    // Everything under the backdrop waits for an answer.
    v_stack((panel,))
        .style(|s| s.width_full().height_full().background(theme::SCRIM))
        .on_event_stop(EventListener::PointerDown, |_| {})
        .into_any()
}

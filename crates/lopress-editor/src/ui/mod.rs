//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod blocks;
pub mod editor_pane;
pub mod welcome;

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{
    button, dyn_container, empty, h_stack, label, stack, v_stack, Decorators,
};
use floem::IntoView;
use std::cell::RefCell;
use std::rc::Rc;

use crate::model::types::EditorDoc;
use crate::settings::{self, Settings};
use crate::state::{AppContext, AppState, EditingState, WelcomeState};
use lopress_gui_host::Session;

/// Maximum number of recent workspaces to retain.
const MAX_RECENTS: usize = 5;

/// Build the root view, consuming the loaded `AppContext`.
///
/// `settings_signal` is pre-created by the caller so the window-close handler
/// in `lib.rs` can read the latest settings (recents + geometry) for
/// persistence.
pub fn root_view(ctx: AppContext, settings_signal: RwSignal<Settings>) -> impl IntoView {
    // Initialise the signal with the loaded settings.
    settings_signal.set(ctx.settings);

    let initial_welcome = match ctx.state {
        AppState::Welcome(w) => w,
        AppState::Editing(_) => WelcomeState::default(),
    };

    let welcome_signal: RwSignal<WelcomeState> = RwSignal::new(initial_welcome);
    let state_tag: RwSignal<StateTag> = RwSignal::new(StateTag::Welcome);

    // Editing-state holder. Shared between the open callback and the editing
    // view. We keep it in an `Rc<RefCell<_>>` because the Floem signal would
    // require Send + Sync on Session, which the underlying watcher/server
    // handles do not provide.
    let editing: Rc<RefCell<Option<EditingState>>> = Rc::new(RefCell::new(None));

    // Reactive view of the currently open document.
    let current_doc: RwSignal<Option<EditorDoc>> = RwSignal::new(None);

    // Callback invoked by the welcome view when the user picks a path.
    let editing_for_open = Rc::clone(&editing);
    let on_open = move |path: std::path::PathBuf| match Session::open(&path) {
        Ok(session) => {
            settings_signal.update(|s| {
                s.recents.retain(|p| p != &path);
                s.recents.insert(0, path.clone());
                s.recents.truncate(MAX_RECENTS);
            });
            if let Some(sp) = settings::default_path() {
                settings_signal.with(|s| {
                    s.save_to(&sp).ok();
                });
            }

            *editing_for_open.borrow_mut() = Some(EditingState::new(session));
            current_doc.set(None);
            state_tag.set(StateTag::Editing);
        }
        Err(e) => {
            welcome_signal.update(|w| {
                w.error = Some(e.to_string());
            });
        }
    };

    let editing_for_view = Rc::clone(&editing);

    dyn_container(
        move || state_tag.get(),
        move |tag| match tag {
            StateTag::Welcome => {
                welcome::welcome_view(welcome_signal, settings_signal, on_open.clone()).into_any()
            }
            StateTag::Editing => {
                editing_view(Rc::clone(&editing_for_view), current_doc).into_any()
            }
        },
    )
    .style(|s| s.width_full().height_full())
}

/// Three-column scaffold: sidebar (left) + editor pane (center) + inspector (right),
/// with a footer pinned at the bottom.
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> impl IntoView {
    let editing_for_btn = Rc::clone(&editing);
    let open_first = button(label(|| "Open first post")).action(move || {
        let mut guard = editing_for_btn.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        let workspace = state.session.workspace();
        if let Some(first) = workspace.posts.first().cloned() {
            state.open_document(&first);
            current_doc.set(state.current_doc.clone());
        }
    });

    let sidebar = v_stack((
        label(|| "Posts").style(|s| s.font_weight(floem::text::Weight::SEMIBOLD).padding(8.)),
        open_first,
    ))
    .style(|s| {
        s.width(220.)
            .height_full()
            .background(Color::rgb8(248, 248, 248))
            .border_right(1.)
            .border_color(Color::rgb8(220, 220, 220))
            .padding(8.)
    });

    let editor = dyn_container(
        move || current_doc.get(),
        move |maybe_doc| match maybe_doc {
            Some(doc) => editor_pane::editor_pane(&doc).into_any(),
            None => label(|| "No document open. Click \"Open first post\" to load one.")
                .style(|s| {
                    s.width_full()
                        .height_full()
                        .items_center()
                        .justify_center()
                        .color(Color::rgb8(140, 140, 140))
                })
                .into_any(),
        },
    )
    .style(|s| s.flex_grow(1.0).height_full());

    let inspector = empty().style(|s| {
        s.width(280.)
            .height_full()
            .background(Color::rgb8(250, 250, 250))
            .border_left(1.)
            .border_color(Color::rgb8(220, 220, 220))
    });

    let footer = empty().style(|s| {
        s.width_full()
            .height(28.)
            .background(Color::rgb8(245, 245, 245))
            .border_top(1.)
            .border_color(Color::rgb8(220, 220, 220))
    });

    let columns = h_stack((sidebar, editor, inspector))
        .style(|s| s.width_full().flex_grow(1.0));

    stack((columns, footer)).style(|s| s.flex_col().width_full().height_full())
}

/// Lightweight discriminant so `dyn_container` can derive equality cheaply.
#[derive(Clone, PartialEq)]
enum StateTag {
    Welcome,
    Editing,
}

//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod blocks;
pub mod dnd;
pub mod editing;
pub mod editor_pane;
pub mod footer;
pub mod inspector;

pub mod sidebar;
pub mod slash_menu;
pub mod toolbar;
pub mod welcome;

use floem::action::debounce_action;
use floem::event::{Event, EventListener};
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{dyn_container, empty, h_stack, label, stack, Decorators};
use floem::IntoView;
use lopress_core::perf;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::actions::{apply, BlockAction};
use crate::ui::editing::focus;
use crate::ui::editing::pane_key;
use crate::ui::editing::new_doc;
use crate::ui::editing::save_pipeline;
use crate::ui::editing::{action_sink, undo_redo};
use crate::model::types::{BlockId, EditorDoc};
use crate::settings::{self, Settings};
use crate::state::{AppContext, AppState, EditingState, WelcomeState};
use crate::ui::blocks::inline_editor::ActionSink;
use crate::ui::dnd::DndState;
use crate::ui::footer::{footer_view, start_build_status_poll, start_serve_status_poll};
use crate::ui::inspector::inspector_view;
use crate::ui::sidebar::{new_doc_stub, sidebar_view, unique_untitled_path};
use lopress_gui_host::{BuildStatus, DocumentRef, ServeStatus, Session, WorkspaceSummary};
use std::path::PathBuf;

/// Maximum number of recent workspaces to retain.
const MAX_RECENTS: usize = 5;

/// Build the root view, consuming the loaded `AppContext`.
///
/// `settings_signal` is pre-created by the caller so the window-close handler
/// in `lib.rs` can read the latest settings (recents + geometry) for
/// persistence.
pub(crate) fn root_view(
    ctx: AppContext,
    settings_signal: RwSignal<Settings>,
    #[cfg(debug_assertions)] ctrl_handle: crate::ctrl::CtrlHandle,
    #[cfg(debug_assertions)] ctrl_action_rx: crossbeam_channel::Receiver<
        crate::ctrl::CtrlActionEnvelope,
    >,
) -> impl IntoView {
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

    #[cfg(debug_assertions)]
    #[allow(clippy::type_complexity)]
    let ctrl_once: Rc<
        std::cell::RefCell<
            Option<(
                crate::ctrl::CtrlHandle,
                crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
            )>,
        >,
    > = Rc::new(std::cell::RefCell::new(Some((ctrl_handle, ctrl_action_rx))));
    #[cfg(debug_assertions)]
    let ctrl_once_for_view = Rc::clone(&ctrl_once);

    dyn_container(
        move || state_tag.get(),
        move |tag| match tag {
            StateTag::Welcome => {
                welcome::welcome_view(welcome_signal, settings_signal, on_open.clone()).into_any()
            }
            StateTag::Editing => {
                #[cfg(debug_assertions)]
                let ctrl = ctrl_once_for_view.borrow_mut().take();
                editing_view(
                    Rc::clone(&editing_for_view),
                    current_doc,
                    #[cfg(debug_assertions)]
                    ctrl,
                )
                .into_any()
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
    #[cfg(debug_assertions)] ctrl: Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlActionEnvelope>,
    )>,
) -> impl IntoView {
    // Snapshot the workspace once at view-build time. Sidebar actions
    // (new post / new page) call `session.rescan()` and then update this
    // signal; clicks just call `open_document` and update `current_path`.
    let initial_ws: WorkspaceSummary = editing
        .borrow()
        .as_ref()
        .map(|s| s.session.workspace())
        .unwrap_or_else(|| WorkspaceSummary {
            root: PathBuf::new(),
            name: String::new(),
            posts: Vec::new(),
            pages: Vec::new(),
        });
    let workspace_signal: RwSignal<WorkspaceSummary> = RwSignal::new(initial_ws);
    let current_path: RwSignal<Option<PathBuf>> = RwSignal::new(None);

    let undo_stack: RwSignal<crate::undo::UndoStack> = RwSignal::new(crate::undo::UndoStack::new());

    let editing_for_open = Rc::clone(&editing);
    let on_open: Rc<dyn Fn(DocumentRef)> = Rc::new(move |doc_ref: DocumentRef| {
        let mut guard = editing_for_open.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        undo_stack.update(|s| *s = crate::undo::UndoStack::new());
    });

    let on_new_post = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Post,
    );
    let on_new_page = new_doc::make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        new_doc::DocKind::Page,
    );

    let sidebar = sidebar_view(
        workspace_signal,
        current_path,
        on_open,
        on_new_post,
        on_new_page,
    );

    let focus_target: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let slash_menu_open: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let dnd = DndState::new();

    // ── Save pipeline ────────────────────────────────────────────────────
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);

    // ── Action sink + undo/redo closures ───────────────────────────────
    let on_action = action_sink::build_action_sink(
        current_doc, focus_target, slash_menu_open, undo_stack, Rc::clone(&save.mark_dirty),
    );
    let on_undo = undo_redo::build_undo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));
    let on_redo = undo_redo::build_redo(undo_stack, current_doc, focus_target, Rc::clone(&save.mark_dirty));

    // Cloned for the debug ctrl wiring near the end of this function;
    // `on_action` itself is moved into the dyn_container view closure.
    #[cfg(debug_assertions)]
    let on_action_for_ctrl = on_action.clone();

    // Key the editor-pane rebuild on the *shape* of the doc — block id
    // sequence + per-block kind tag + plugin presence — not the full
    // content. Within-block text edits (which fire EditInline →
    // current_doc.update) must NOT tear down the per-block widgets,
    // otherwise focus is lost every time the user commits runs (e.g.,
    // arrow-key navigation between blocks calls commit_runs first;
    // rebuilding the pane afterwards would orphan focus on the destination
    // block). The per-block widgets own their own `runs_sig` reactive
    // copies; structural changes (split, delete, insert, reorder) change
    // the id list and trigger a rebuild. Block-kind changes (toolbar
    // P/H1/H2/Code/UL/OL buttons) do too — discriminant comparison covers
    // Heading(1) vs Heading(2), List{ordered:false} vs ordered:true, etc.
    let pane_key = pane_key::build_pane_key(current_doc);
    let editor = dyn_container(pane_key, move |maybe_ids| match maybe_ids {
        Some(_ids) => match current_doc.with_untracked(|d| d.clone()) {
            Some(doc) => editor_pane::editor_pane(
                &doc,
                on_action.clone(),
                focus_target,
                slash_menu_open,
                dnd,
                current_doc,
                on_undo.clone(),
                on_redo.clone(),
            )
            .into_any(),
            None => empty().into_any(),
        },
        None => label(|| "No document open. Pick one from the sidebar.")
            .style(|s| {
                s.width_full()
                    .height_full()
                    .items_center()
                    .justify_center()
                    .color(Color::rgb8(140, 140, 140))
            })
            .into_any(),
    })
    .style(|s| s.flex_grow(1.0).height_full().min_height(0.));

    let inspector = inspector_view(current_doc, current_path, Rc::clone(&save.mark_dirty));

    let footer = footer_view(
        save.build_status_sig,
        save.dirty_sig,
        save.save_error_sig,
        current_doc,
        save.serve_status_sig,
    );

    // ── Debug ctrl wiring ────────────────────────────────────────────────────
    #[cfg(debug_assertions)]
    if let Some((ctrl_handle, ctrl_action_rx)) = ctrl {
        use floem::ext_event::create_signal_from_channel;
        use floem::reactive::create_effect;

        let snap = ctrl_handle.snapshot.clone();
        create_effect(move |_| {
            let json = current_doc.with(|maybe| {
                crate::ctrl::serialize_state(
                    maybe.as_ref(),
                    current_path.get_untracked().as_deref(),
                )
            });
            *snap.lock().unwrap_or_else(|e| e.into_inner()) = json;
        });

        let action_read = create_signal_from_channel(ctrl_action_rx);
        create_effect(move |_| {
            if let Some((ctrl_action, reply_tx)) = action_read.get() {
                let block_id = ctrl_action.block_id();
                // Translate against the current doc. into_block_action's
                // only failure mode is an unknown block id; a missing doc
                // is detected separately so the caller gets a precise
                // result. on_action MUST run outside with_untracked — it
                // calls current_doc.update() and would re-borrow the signal.
                let translated: Result<BlockAction, crate::ctrl::CtrlActionResult> = current_doc
                    .with_untracked(|maybe| match maybe.as_ref() {
                        None => Err(crate::ctrl::CtrlActionResult::NoDocument),
                        Some(doc) => ctrl_action
                            .into_block_action(doc)
                            .ok_or(crate::ctrl::CtrlActionResult::BlockNotFound { block_id }),
                    });
                let result = match translated {
                    Ok(action) => {
                        on_action_for_ctrl(action);
                        crate::ctrl::CtrlActionResult::Dispatched
                    }
                    Err(failure) => failure,
                };
                let _ = reply_tx.send(result);
            }
        });
    }

    // `min_height(0)` lets these flex items shrink below their content height
    // so the editor pane's `scroll` gets a bounded viewport (see editor_pane).
    let columns = h_stack((sidebar, editor, inspector))
        .style(|s| s.width_full().flex_grow(1.0).min_height(0.));

    let editing_for_close = Rc::clone(&editing);
    stack((columns, footer))
        .style(|s| s.flex_col().width_full().height_full())
        .on_event_stop(EventListener::WindowClosed, move |_e: &Event| {
            // Force-flush any unsaved edits before the window dies.
            if !save.dirty_sig.get_untracked() {
                return;
            }
            let doc = match current_doc.with_untracked(|d| d.clone()) {
                Some(d) => d,
                None => return,
            };
            if let Some(state) = editing_for_close.borrow().as_ref() {
                let _ = state.save_doc(&doc);
            }
        })
}

/// Lightweight discriminant so `dyn_container` can derive equality cheaply.
#[derive(Clone, PartialEq)]
enum StateTag {
    Welcome,
    Editing,
}



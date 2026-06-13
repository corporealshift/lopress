//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod blocks;
pub mod dnd;
pub mod editing;
pub mod editor_pane;
pub mod footer;
pub mod inspector;

pub mod nav_editor;
pub mod sidebar;
pub mod slash_menu;
pub mod toolbar;
pub mod welcome;

use floem::event::{Event, EventListener};
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::style::Position;
use floem::text::Weight;
use floem::unit::PxPctAuto;
use floem::views::{dyn_container, empty, h_stack, label, stack, v_stack, Decorators};
use floem::IntoView;
use std::cell::RefCell;
use std::rc::Rc;

use crate::actions::BlockAction;
use crate::model::types::{BlockId, EditorDoc};
use crate::settings::{self, Settings};
use crate::state::{AppContext, AppState, EditingState, WelcomeState};
use crate::ui::dnd::DndState;
#[cfg(debug_assertions)]
use crate::ui::editing::ctrl_wire;
use crate::ui::editing::new_doc;
use crate::ui::editing::save_pipeline;
use crate::ui::editing::{action_sink, undo_redo};
use crate::ui::footer::footer_view;
use crate::ui::inspector::inspector_view;
use crate::ui::nav_editor::{NavModel, PageChoice, TagChoice};
use crate::ui::sidebar::sidebar_view;
use lopress_build::NavItem;
use lopress_gui_host::{DocumentRef, Session, WorkspaceSummary};
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
    #[cfg(debug_assertions)] ctrl_open_rx: crossbeam_channel::Receiver<
        crate::ctrl::CtrlOpenEnvelope,
    >,
    #[cfg(debug_assertions)] ctrl_close_rx: crossbeam_channel::Receiver<
        crate::ctrl::CtrlCloseEnvelope,
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

    // Reactive view of the open document's path. Lifted to root_view (from
    // editing_view) so the ctrl snapshot/open/close effects can read & set it.
    let current_path: RwSignal<Option<PathBuf>> = RwSignal::new(None);

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

    // Wire the always-on ctrl effects (snapshot/open/close) at root scope so
    // `/open` and `/close` work from the welcome screen. Returns the action
    // signal, which `editing_view` wires to the `on_action` sink on mount.
    #[cfg(debug_assertions)]
    let ctrl_action_read = ctrl_wire::wire_ctrl_root(
        ctrl_handle,
        ctrl_action_rx,
        ctrl_open_rx,
        ctrl_close_rx,
        current_doc,
        current_path,
        Rc::clone(&editing),
        state_tag,
    );

    dyn_container(
        move || state_tag.get(),
        move |tag| match tag {
            StateTag::Welcome => {
                welcome::welcome_view(welcome_signal, settings_signal, on_open.clone()).into_any()
            }
            StateTag::Editing => editing_view(
                Rc::clone(&editing_for_view),
                current_doc,
                current_path,
                #[cfg(debug_assertions)]
                ctrl_action_read,
            )
            .into_any(),
        },
    )
    .style(|s| s.width_full().height_full())
}

/// Three-column scaffold: sidebar (left) + editor pane (center) + inspector (right),
/// with a footer pinned at the bottom.
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    #[cfg(debug_assertions)] ctrl_action_read: floem::reactive::ReadSignal<
        Option<crate::ctrl::CtrlActionEnvelope>,
    >,
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
            tags: Vec::new(),
        });
    let workspace_signal: RwSignal<WorkspaceSummary> = RwSignal::new(initial_ws);

    // Compute the plugin inserter list once at view-build time. The registry
    // is stable for a loaded workspace; recomputing per keystroke is wasteful.
    let initial_inserter_items: Rc<[crate::model::inserter::PluginInserterItem]> = Rc::from(
        crate::model::inserter::inserter_items(
            &editing
                .borrow()
                .as_ref()
                .map(|s| s.plugin_registry.clone())
                .unwrap_or_default(),
        )
        .into_boxed_slice(),
    );

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

    // ── Nav editor modal state ───────────────────────────────────────────
    // Opened from the sidebar's "Site settings" button; the modal itself is
    // built near the end of this function and overlaid on the whole view.
    let nav_editor_open: RwSignal<bool> = RwSignal::new(false);
    let nav_save_error: RwSignal<Option<String>> = RwSignal::new(None);
    let on_site_settings: Rc<dyn Fn()> = Rc::new(move || {
        nav_save_error.set(None);
        nav_editor_open.set(true);
    });

    let sidebar = sidebar_view(
        workspace_signal,
        current_path,
        on_open,
        on_new_post,
        on_new_page,
        on_site_settings,
    );

    let focus_target: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let slash_menu_open: RwSignal<Option<BlockId>> = RwSignal::new(None);
    let dnd = DndState::new();

    // ── Save pipeline ────────────────────────────────────────────────────
    let save = save_pipeline::start_save_pipeline(Rc::clone(&editing), current_doc);

    // ── Action sink + undo/redo closures ───────────────────────────────
    let on_action = action_sink::build_action_sink(
        current_doc,
        focus_target,
        slash_menu_open,
        undo_stack,
        Rc::clone(&save.mark_dirty),
    );
    let on_undo = undo_redo::build_undo(
        undo_stack,
        current_doc,
        focus_target,
        Rc::clone(&save.mark_dirty),
    );
    let on_redo = undo_redo::build_redo(
        undo_stack,
        current_doc,
        focus_target,
        Rc::clone(&save.mark_dirty),
    );

    // ── Image import callback ────────────────────────────────────────
    let editing_for_image = Rc::clone(&editing);
    let on_action_for_image = on_action.clone();
    let on_insert_image: Rc<dyn Fn(crate::model::types::BlockId)> =
        Rc::new(move |anchor: crate::model::types::BlockId| {
            // Native file dialog (rfd) — same crate used by the workspace picker.
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                .pick_file()
            else {
                return; // cancelled
            };
            let web = {
                let st = editing_for_image.borrow();
                let Some(state) = st.as_ref() else {
                    return; // no open session to import into
                };
                state.session.import_image(&path)
            };
            match web {
                Ok(src) => {
                    on_action_for_image(BlockAction::InsertAfter {
                        anchor,
                        new_block: Box::new(crate::model::types::EditorBlock::image(&src, "", "")),
                    });
                }
                Err(e) => eprintln!("image import failed: {e}"),
            }
        });

    // Cloned for the debug ctrl wiring near the end of this function;
    // `on_action` itself is moved into the dyn_container view closure.
    #[cfg(debug_assertions)]
    let on_action_for_ctrl = on_action.clone();

    // The editor pane owns its own (stable) `scroll` node and an inner
    // `dyn_container` that rebuilds the block column on every doc mutation, so
    // it is mounted ONCE here rather than wrapped in a pane-rebuild
    // `dyn_container` — that earlier wrapping recreated the `scroll` node on
    // every edit and reset the scroll offset to the top. The "no document
    // open" placeholder is handled inside the pane's inner container.
    let on_insert_image_for_pane = Rc::clone(&on_insert_image);
    let inserter_items_for_pane = Rc::clone(&initial_inserter_items);
    let on_action_for_editor = on_action.clone();
    let editor = editor_pane::editor_pane(
        current_doc,
        on_action_for_editor,
        focus_target,
        slash_menu_open,
        dnd,
        on_undo.clone(),
        on_redo.clone(),
        on_insert_image_for_pane,
        inserter_items_for_pane,
    )
    .style(|s| s.flex_grow(1.0).height_full().min_height(0.));

    let inspector = inspector_view(current_doc, current_path, on_action.clone());

    let footer = footer_view(
        save.build_status_sig,
        save.dirty_sig,
        save.save_error_sig,
        current_doc,
        save.serve_status_sig,
    );

    // ── Debug ctrl wiring ────────────────────────────────────────────────────
    // Snapshot/open/close effects are wired once at root scope; here we only
    // attach the action effect (it needs the `on_action` sink).
    #[cfg(debug_assertions)]
    ctrl_wire::wire_ctrl_action(ctrl_action_read, current_doc, on_action_for_ctrl);

    // `min_height(0)` lets these flex items shrink below their content height
    // so the editor pane's `scroll` gets a bounded viewport (see editor_pane).
    let columns = h_stack((sidebar, editor, inspector))
        .style(|s| s.width_full().flex_grow(1.0).min_height(0.));

    // ── Nav editor modal overlay ─────────────────────────────────────────
    // Absolutely positioned over the whole editing view; `empty` when closed.
    // A fresh working model and picker lists are built from the session each
    // time it opens, so it always reflects the latest saved nav.
    let editing_for_modal = Rc::clone(&editing);
    let nav_modal = dyn_container(
        move || nav_editor_open.get(),
        move |open| {
            if !open {
                return empty().into_any();
            }
            let (model, pages, tags) = {
                let guard = editing_for_modal.borrow();
                let Some(state) = guard.as_ref() else {
                    return empty().into_any();
                };
                let ws = state.session.workspace();
                let pages: Vec<PageChoice> = ws
                    .pages
                    .iter()
                    .map(|p| PageChoice {
                        slug: p.slug.clone(),
                        title: p.title.clone(),
                    })
                    .collect();
                let tags: Vec<TagChoice> = ws
                    .tags
                    .iter()
                    .map(|t| TagChoice { name: t.clone() })
                    .collect();
                (NavModel::new(state.session.nav_items()), pages, tags)
            };
            let model_sig: RwSignal<NavModel> = RwSignal::new(model);

            let editing_for_save = Rc::clone(&editing_for_modal);
            let on_save = move |items: Vec<NavItem>| {
                let guard = editing_for_save.borrow();
                let Some(state) = guard.as_ref() else {
                    return;
                };
                match state.session.update_nav(items) {
                    Ok(()) => {
                        nav_save_error.set(None);
                        nav_editor_open.set(false);
                    }
                    Err(e) => nav_save_error.set(Some(e.to_string())),
                }
            };
            let on_cancel = move || nav_editor_open.set(false);

            let error_line = dyn_container(
                move || nav_save_error.get(),
                move |err| match err {
                    Some(e) => label(move || e.clone())
                        .style(|s| s.color(Color::rgb8(200, 60, 60)).font_size(12.))
                        .into_any(),
                    None => empty().into_any(),
                },
            );

            let panel = v_stack((
                label(|| "Site settings \u{2014} navigation".to_string())
                    .style(|s| s.font_size(15.).font_weight(Weight::SEMIBOLD)),
                error_line,
                nav_editor::nav_editor_view(model_sig, pages, tags, on_save, on_cancel),
            ))
            .style(|s| {
                s.background(Color::rgb8(255, 255, 255))
                    .border(1.)
                    .border_color(Color::rgb8(200, 200, 200))
                    .border_radius(8.)
                    .padding(16.)
                    .margin_top(60.)
                    .margin_horiz(PxPctAuto::Auto)
            })
            // Swallow clicks on the panel so they don't reach the dismiss
            // handler on the backdrop.
            .on_click_stop(move |_| {});

            stack((panel,))
                .style(|s| {
                    s.position(Position::Absolute)
                        .inset_top(0.)
                        .inset_left(0.)
                        .width_full()
                        .height_full()
                        .background(Color::rgba8(0, 0, 0, 110))
                })
                .on_click_stop(move |_| nav_editor_open.set(false))
                .into_any()
        },
    )
    .style(|s| {
        s.position(Position::Absolute)
            .inset_top(0.)
            .inset_left(0.)
            .width_full()
            .height_full()
    });

    let editing_for_close = Rc::clone(&editing);
    stack((columns, footer, nav_modal))
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
pub(crate) enum StateTag {
    Welcome,
    Editing,
}

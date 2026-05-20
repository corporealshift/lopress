//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod blocks;
pub mod dnd;
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
    #[cfg(debug_assertions)] ctrl_action_rx: crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
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
                crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
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

/// The block a just-applied undo/redo action should restore focus to.
fn focus_block_for(action: &BlockAction) -> Option<BlockId> {
    match action {
        BlockAction::EditInline { block_id, .. }
        | BlockAction::EditCode { block_id, .. }
        | BlockAction::Split { block_id, .. }
        | BlockAction::MergeWithPrev { block_id }
        | BlockAction::ChangeType { block_id, .. }
        | BlockAction::EditAttrs { block_id, .. }
        | BlockAction::Move { block_id, .. } => Some(*block_id),
        BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
        BlockAction::Delete { .. } | BlockAction::OpenSlashMenu { .. } => None,
        BlockAction::EditListItem { block_id, .. }
        | BlockAction::SplitListItem { block_id, .. }
        | BlockAction::MergeListItemWithPrev { block_id, .. } => Some(*block_id),
    }
}

/// The id of the item immediately after `item_id` in `block_id`'s list.
fn list_item_after(doc: &EditorDoc, block_id: BlockId, item_id: BlockId) -> Option<BlockId> {
    let block = doc.blocks.iter().find(|b| b.id == block_id)?;
    let crate::model::types::BlockBody::List(items) = &block.body else {
        return None;
    };
    let pos = items.iter().position(|it| it.id == item_id)?;
    items.get(pos + 1).map(|it| it.id)
}

/// Three-column scaffold: sidebar (left) + editor pane (center) + inspector (right),
/// with a footer pinned at the bottom.
fn editing_view(
    editing: Rc<RefCell<Option<EditingState>>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    #[cfg(debug_assertions)] ctrl: Option<(
        crate::ctrl::CtrlHandle,
        crossbeam_channel::Receiver<crate::ctrl::CtrlAction>,
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

    let on_new_post = make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        DocKind::Post,
    );
    let on_new_page = make_new_doc_action(
        Rc::clone(&editing),
        workspace_signal,
        current_doc,
        current_path,
        DocKind::Page,
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

    // ── Save-debounce signals ────────────────────────────────────────────
    // `dirty_counter` bumps on every legitimate edit; `debounce_action`
    // watches it and runs the save closure 500 ms after the last bump.
    // `dirty_sig` / `save_error_sig` drive the footer's status display.
    let build_status_sig: RwSignal<BuildStatus> = RwSignal::new(BuildStatus::Idle);
    let dirty_sig: RwSignal<bool> = RwSignal::new(false);
    let save_error_sig: RwSignal<Option<String>> = RwSignal::new(None);
    let dirty_counter: RwSignal<u64> = RwSignal::new(0);

    let mark_dirty: Rc<dyn Fn()> = Rc::new(move || {
        dirty_sig.set(true);
        dirty_counter.update(|n| *n = n.wrapping_add(1));
    });

    // Chokepoint: every block-tree mutation routes through here. Pre/post
    // lookups derive the block to focus after structural actions.
    let on_action_mark_dirty = Rc::clone(&mark_dirty);
    let on_action: ActionSink = Rc::new(move |action: BlockAction| {
        let _t = perf::span("editor.on_action");
        if let BlockAction::OpenSlashMenu { block_id } = action {
            slash_menu_open.set(Some(block_id));
            return;
        }
        if slash_menu_open.get_untracked().is_some() {
            slash_menu_open.set(None);
        }

        // Pre-focus must read pre-apply state (the block before the one
        // being merged into its predecessor). Capture it before the apply
        // mutates the doc.
        let pre_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::MergeWithPrev { block_id }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .filter(|&i| i > 0)
                .and_then(|i| d.blocks.get(i - 1))
                .map(|b| b.id),
            _ => None,
        });

        // Apply the action; capture the returned (canonical, inverse) pair
        // and push it onto the undo stack. apply returns None for
        // unrecordable cases (UI-only, no-op, or stage-1-unrecordable
        // structural splits / first-block delete).
        let action_for_apply = action.clone();
        let mut recorded: Option<(BlockAction, BlockAction)> = None;
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                recorded = apply(d, action_for_apply);
            }
        });
        if let Some((canonical, inverse)) = recorded {
            undo_stack.update(|s| s.push_after_apply(canonical, inverse));
        }

        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            (
                BlockAction::SplitListItem {
                    block_id, item_id, ..
                },
                Some(d),
            ) => list_item_after(d, *block_id, *item_id),
            _ => None,
        });
        let change_type_focus = match &action {
            BlockAction::ChangeType { block_id, .. } => Some(*block_id),
            _ => None,
        };
        // A freshly inserted block (e.g. the empty-document "add block"
        // button) should take focus so the caret lands in it immediately.
        let insert_focus = match &action {
            BlockAction::InsertAfter { new_block, .. } => Some(new_block.id),
            _ => None,
        };
        if let Some(id) = pre_focus
            .or(post_focus)
            .or(change_type_focus)
            .or(insert_focus)
        {
            floem::action::exec_after(Duration::from_millis(0), move |_| {
                focus_target.set(Some(id));
            });
        }
        on_action_mark_dirty();
    });

    let on_undo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_undo();
            });
            if let Some(action) = popped {
                let focus_id = focus_block_for(&action);
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                // No post-apply id surgery: Split / SplitListItem in stored
                // entries carry new_block_id: Some(...), so re-applying them
                // is id-stable without patching the redo entry.
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };

    let on_redo: Rc<dyn Fn()> = {
        let mark_dirty = Rc::clone(&mark_dirty);
        Rc::new(move || {
            let mut popped = None;
            undo_stack.update(|s| {
                popped = s.pop_redo();
            });
            if let Some(action) = popped {
                let focus_id = focus_block_for(&action);
                let action_for_apply = action.clone();
                current_doc.update(|maybe| {
                    if let Some(d) = maybe {
                        let _ = apply(d, action_for_apply);
                    }
                });
                // No post-apply id surgery for the same reason as on_undo.
                if let Some(id) = focus_id {
                    floem::action::exec_after(Duration::from_millis(0), move |_| {
                        focus_target.set(Some(id));
                    });
                }
                mark_dirty();
            }
        })
    };

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
    let pane_key = move || {
        current_doc.with(|d| {
            d.as_ref().map(|d| {
                d.blocks
                    .iter()
                    .map(|b| (b.id, kind_tag(&b.kind), b.plugin.is_some()))
                    .collect::<Vec<_>>()
            })
        })
    };
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

    let inspector = inspector_view(current_doc, current_path, Rc::clone(&mark_dirty));

    let serve_status_sig: RwSignal<ServeStatus> = RwSignal::new(ServeStatus::Starting);

    {
        let editing_for_poll = Rc::clone(&editing);
        let session_reader: Rc<dyn Fn() -> BuildStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.build_status())
                .unwrap_or(BuildStatus::Idle)
        });
        start_build_status_poll(session_reader, build_status_sig);
    }

    {
        let editing_for_poll = Rc::clone(&editing);
        let serve_reader: Rc<dyn Fn() -> ServeStatus> = Rc::new(move || {
            editing_for_poll
                .borrow()
                .as_ref()
                .map(|s| s.session.serve_status())
                .unwrap_or(ServeStatus::Starting)
        });
        start_serve_status_poll(serve_reader, serve_status_sig);
    }

    // Debounced save+rebuild. `debounce_action` resets its internal timer on
    // every counter bump and fires the closure 500 ms after the last bump.
    {
        let editing_for_save = Rc::clone(&editing);
        debounce_action(dirty_counter, Duration::from_millis(500), move || {
            let doc = match current_doc.with_untracked(|d| d.clone()) {
                Some(d) => d,
                None => return,
            };
            let result = {
                let guard = editing_for_save.borrow();
                match guard.as_ref() {
                    Some(state) => state.save_doc(&doc),
                    None => return,
                }
            };
            match result {
                Ok(()) => {
                    dirty_sig.set(false);
                    save_error_sig.set(None);
                    if let Some(state) = editing_for_save.borrow().as_ref() {
                        state.session.rebuild();
                    }
                }
                Err(msg) => {
                    save_error_sig.set(Some(msg));
                }
            }
        });
    }

    let footer = footer_view(
        build_status_sig,
        dirty_sig,
        save_error_sig,
        current_doc,
        serve_status_sig,
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
            if let Some(ctrl_action) = action_read.get() {
                let block_action =
                    current_doc.with_untracked(|d| ctrl_action.into_block_action(d.as_ref()?));
                if let Some(action) = block_action {
                    on_action_for_ctrl(action);
                }
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
            if !dirty_sig.get_untracked() {
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

/// Compact equality tag for `BlockKind` used by the editor-pane rebuild key.
/// `Eq` is fine; this is just a discriminator (Heading(1) vs Heading(2),
/// List{ordered:false} vs ordered:true, etc.) so we trigger a pane rebuild
/// when the toolbar's type buttons swap a block's kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum KindTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Opaque,
}

fn kind_tag(k: &crate::model::types::BlockKind) -> KindTag {
    use crate::model::types::BlockKind;
    match k {
        BlockKind::Paragraph => KindTag::Paragraph,
        BlockKind::Heading(level) => KindTag::Heading(*level),
        BlockKind::Code { .. } => KindTag::Code,
        BlockKind::List { ordered } => KindTag::List { ordered: *ordered },
        BlockKind::Opaque { .. } => KindTag::Opaque,
    }
}

/// Whether a "+ New …" sidebar action targets the Posts or Pages directory.
#[derive(Clone, Copy)]
enum DocKind {
    Post,
    Page,
}

impl DocKind {
    fn default_title(self) -> &'static str {
        match self {
            DocKind::Post => "New Post",
            DocKind::Page => "New Page",
        }
    }
}

/// Build the closure the sidebar invokes for "+ New post" / "+ New page".
///
/// The closure: picks a fresh `untitled-N.md` filename, writes the stub
/// markdown, rescans the workspace, then opens the new doc through
/// `EditingState::open_document` so the editor pane and current_path signal
/// stay in sync with the sidebar.
fn make_new_doc_action(
    editing: Rc<RefCell<Option<EditingState>>>,
    workspace_signal: RwSignal<WorkspaceSummary>,
    current_doc: RwSignal<Option<EditorDoc>>,
    current_path: RwSignal<Option<PathBuf>>,
    kind: DocKind,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        let mut guard = editing.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        let dir = match kind {
            DocKind::Post => state.session.posts_dir(),
            DocKind::Page => state.session.pages_dir(),
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("failed to create {}: {e}", dir.display());
            return;
        }
        let path = unique_untitled_path(&dir);
        if let Err(e) = std::fs::write(&path, new_doc_stub(kind.default_title())) {
            eprintln!("failed to write {}: {e}", path.display());
            return;
        }

        let summary = state.session.rescan();
        let doc_ref = summary
            .posts
            .iter()
            .chain(summary.pages.iter())
            .find(|d| d.path == path)
            .cloned()
            .unwrap_or_else(|| DocumentRef {
                path: path.clone(),
                title: kind.default_title().to_string(),
                is_draft: true,
                has_parse_error: false,
            });

        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
        workspace_signal.set(summary);
    })
}

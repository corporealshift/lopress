//! Root UI module. Switches between Welcome and Editing views based on `AppState`.

pub mod blocks;
pub mod clipboard;
pub mod dnd;
pub mod editor_pane;
pub mod inspector;
pub mod sel_ctx;
pub mod sidebar;
pub mod slash_menu;
pub mod toolbar;
pub mod welcome;

use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{dyn_container, empty, h_stack, label, stack, Decorators};
use floem::IntoView;
use std::cell::RefCell;
use std::rc::Rc;

use crate::actions::{apply, BlockAction};
use crate::model::types::{BlockId, EditorDoc};
use crate::selection::{DocPosition, DocSelection, GeometryCache};
use crate::settings::{self, Settings};
use crate::state::{AppContext, AppState, EditingState, WelcomeState};
use crate::ui::blocks::inline_editor::ActionSink;
use crate::ui::dnd::DndState;
use crate::ui::sel_ctx::SelectionContext;
use crate::ui::inspector::inspector_view;
use crate::ui::sidebar::{new_doc_stub, sidebar_view, unique_untitled_path};
use lopress_gui_host::{DocumentRef, Session, WorkspaceSummary};
use std::path::PathBuf;

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

    let editing_for_open = Rc::clone(&editing);
    let on_open: Rc<dyn Fn(DocumentRef)> = Rc::new(move |doc_ref: DocumentRef| {
        let mut guard = editing_for_open.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.open_document(&doc_ref);
        current_doc.set(state.current_doc.clone());
        current_path.set(Some(doc_ref.path));
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

    // Doc-level selection lives here. Initial value is a synthetic caret —
    // each block's FocusGained handler resets it to a real position when the
    // user clicks in.
    let doc_selection: RwSignal<DocSelection> = RwSignal::new(DocSelection::caret(
        DocPosition::new(BlockId::new(), 0, 0),
    ));
    let geometry = Rc::new(RefCell::new(GeometryCache::default()));
    let sel_ctx = SelectionContext {
        doc_selection,
        current_doc,
        geometry,
    };

    // Chokepoint: every block-tree mutation routes through here. Pre/post
    // lookups derive the block to focus after structural actions.
    let on_action: ActionSink = Rc::new(move |action: BlockAction| {
        // UI-only action: hand off to the slash-menu signal and skip the
        // model. Doing this before `apply` keeps the chokepoint single-entry
        // for block widgets while letting non-mutating actions piggyback.
        if let BlockAction::OpenSlashMenu { block_id } = action {
            slash_menu_open.set(Some(block_id));
            return;
        }
        // Any block-tree mutation closes an open slash menu.
        if slash_menu_open.get_untracked().is_some() {
            slash_menu_open.set(None);
        }
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
        let action_for_apply = action.clone();
        current_doc.update(|maybe| {
            if let Some(d) = maybe {
                apply(d, action_for_apply);
            }
        });
        let post_focus = current_doc.with_untracked(|maybe| match (&action, maybe) {
            (BlockAction::Split { block_id, .. }, Some(d)) => d
                .blocks
                .iter()
                .position(|b| b.id == *block_id)
                .and_then(|i| d.blocks.get(i + 1))
                .map(|b| b.id),
            _ => None,
        });
        if let Some(id) = pre_focus.or(post_focus) {
            focus_target.set(Some(id));
        }
    });

    let editor = dyn_container(
        move || current_doc.get(),
        move |maybe_doc| match maybe_doc {
            Some(doc) => editor_pane::editor_pane(
                &doc,
                on_action.clone(),
                focus_target,
                slash_menu_open,
                dnd,
                sel_ctx.clone(),
            )
            .into_any(),
            None => label(|| "No document open. Pick one from the sidebar.")
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

    let inspector = inspector_view(current_doc, current_path);

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

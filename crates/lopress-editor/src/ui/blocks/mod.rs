//! Per-block rendering for the editor pane.
//!
//! Paragraph and Heading blocks are dispatched to the editable inline-runs
//! widget, which owns its own `RwSignal<Vec<InlineRun>>` and a caret signal.
//! The signals are created here from the block's initial runs; later tasks
//! will fold edits back into the document model.

pub mod code_editor;
pub mod editor_registry;
pub mod fallback;
pub mod heading;
pub mod inline_editor;
pub mod list;
pub mod opaque;
pub mod paragraph;
pub mod plugin;
pub mod style_span;

use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::ui::dnd::{drag_handle, DndState, HANDLE_WIDTH};
use crate::ui::toolbar::block_toolbar_for;
use floem::event::{EventListener, EventPropagation};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{dyn_container, empty, h_stack, stack, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Border color for the block that currently holds focus.
const FOCUS_BORDER: floem::peniko::Color = floem::peniko::Color::rgb8(150, 180, 230);

/// How far the floating toolbar sits above its block, as a negative top inset.
/// Matches the toolbar's natural rendered height so it clears the block. The
/// toolbar is an absolutely-positioned overlay, so it reserves no layout space
/// and never shifts the document when focus moves between blocks.
const TOOLBAR_HEIGHT_PX: f32 = 36.;

/// Small vertical breathing room between blocks. The toolbar overlay still
/// floats over the block above when focused; this just keeps blocks from
/// sitting flush against each other.
const BLOCK_GAP_PX: f64 = 8.;

/// Background tint for the block under the pointer. Subtle so it reads as a
/// hover hint, not a selection — its main job is making empty blocks (which
/// have no text to see) visible when the mouse is over them.
const HOVER_BG: floem::peniko::Color = floem::peniko::Color::rgb8(244, 244, 246);

/// Dispatch one editor block to its renderer. Inline-bodied blocks
/// (paragraph, heading) become editable widgets backed by reactive signals;
/// other kinds remain read-only for now.
#[allow(clippy::too_many_arguments)]
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    let block_id = block.id;
    let kind = block.kind.clone();

    // Plugin blocks take precedence: header strip + attr form + body editor.
    // Built-in dispatch only runs when the block isn't plugin-flagged.
    if block.plugin.is_some() {
        let plugin_view = plugin::plugin_block_view(
            block,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        );
        return wrap_block(plugin_view, block_id, kind, dnd, focus_pub, on_action);
    }

    let body = match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => code_editor::editable_code_view(
            text,
            lang,
            block.id,
            on_action.clone(),
            focus_target,
            focus_pub,
            current_doc,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        ),
        (BlockKind::Opaque { .. }, BlockBody::Opaque(_)) => {
            // Opaque blocks load from disk with unknown/removed plugin types.
            // Route through the fallback so they're visible and recoverable,
            // not a silent drop or a read-only card with no toolbar.
            fallback::fallback_block_view(block, focus_pub).into_any()
        }
        // Body/kind mismatch — render fallback so content is visible and recoverable.
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] block {:?}: kind/body mismatch ({:?} + {:?})",
                block_id, block.kind, block.body
            );
            fallback::fallback_block_view(block, focus_pub).into_any()
        }
    };

    wrap_block(body, block_id, kind, dnd, focus_pub, on_action)
}

/// Wrap a block's body in the shared chrome: a drag-handle gutter, hover/focus
/// styling, and a floating toolbar that appears above the focused block.
///
/// Both the plugin and built-in render paths funnel through here, so every
/// block — paragraph, code, list, plugin, or fallback — is draggable and gets
/// the same toolbar. The toolbar is absolutely positioned, so it reserves no
/// vertical space (blocks sit flush) and never shifts the document when focus
/// moves between blocks.
fn wrap_block(
    body: AnyView,
    block_id: BlockId,
    kind: BlockKind,
    dnd: DndState,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
) -> AnyView {
    // Hover gutter with the drag handle, left of the body.
    let hover: RwSignal<bool> = RwSignal::new(false);
    let handle = drag_handle(block_id, dnd, hover)
        .style(|s| s.width(HANDLE_WIDTH).flex_shrink(0.).items_center());

    let row = h_stack((handle, body.style(|s| s.flex_grow(1.0))))
        .style(move |s| {
            let s = s.width_full().border_radius(4.);
            if hover.get() {
                s.background(HOVER_BG)
            } else {
                s
            }
        })
        .on_event(EventListener::PointerEnter, move |_| {
            hover.set(true);
            EventPropagation::Continue
        })
        .on_event(EventListener::PointerLeave, move |_| {
            hover.set(false);
            EventPropagation::Continue
        });

    let row_with_border = row.style(move |s| {
        let focused = focus_pub.block.get() == Some(block_id);
        let s = s.width_full().border(1.0).border_radius(4.0);
        if focused {
            s.border_color(FOCUS_BORDER)
        } else {
            s.border_color(floem::peniko::Color::TRANSPARENT)
        }
    });

    // Floating toolbar: mounts only when this block is focused, absolutely
    // positioned just above the block so it reserves no layout space and never
    // shifts the document when focus moves.
    let toolbar_overlay = dyn_container(
        move || focus_pub.block.get() == Some(block_id),
        move |is_focused| {
            if is_focused {
                block_toolbar_for(block_id, kind.clone(), focus_pub, on_action.clone()).into_any()
            } else {
                empty().into_any()
            }
        },
    )
    .style(|s| {
        s.position(floem::style::Position::Absolute)
            .inset_top(-f64::from(TOOLBAR_HEIGHT_PX))
            .inset_left(f64::from(HANDLE_WIDTH))
    });

    stack((row_with_border, toolbar_overlay))
        .style(|s| s.width_full().margin_top(BLOCK_GAP_PX))
        .into_any()
}

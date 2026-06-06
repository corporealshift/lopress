//! Per-block rendering for the editor pane.
//!
//! Paragraph and Heading blocks are dispatched to the editable inline-runs
//! widget, which owns its own `RwSignal<Vec<InlineRun>>` and a caret signal.
//! The signals are created here from the block's initial runs; later tasks
//! will fold edits back into the document model.

pub mod code_editor;
pub mod editor_registry;
pub mod env;
pub mod fallback;
pub mod heading;
pub mod image;
pub mod inline_editor;
pub mod list;
pub mod opaque;
pub mod paragraph;
pub mod plugin;
pub mod read_more;
pub mod separator;
pub mod style_span;
pub mod table;

use crate::model::descriptor;
use crate::model::types::{BlockBody, BlockId, EditorBlock};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::dnd::{drag_handle, DndState, HANDLE_WIDTH};
use crate::ui::toolbar::block_toolbar_for;
use floem::event::{EventListener, EventPropagation};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{dyn_container, empty, h_stack, v_stack, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Border color for the block that currently holds focus.
const FOCUS_BORDER: floem::peniko::Color = floem::peniko::Color::rgb8(150, 180, 230);

/// Fixed height of the in-flow toolbar slot above the focused block. The
/// focused block is pulled up by exactly this amount (see `wrap_block`) so the
/// toolbar visually floats over the block above without shifting the document.
/// A fixed height keeps the upward pull and the slot height in lockstep so
/// there is zero net shift regardless of the toolbar's natural content size.
const TOOLBAR_HEIGHT_PX: f32 = 36.;

/// Small vertical breathing room between blocks, so they don't sit flush
/// against each other.
const BLOCK_GAP_PX: f64 = 8.;

/// Background tint for the block under the pointer. Subtle so it reads as a
/// hover hint, not a selection — its main job is making empty blocks (which
/// have no text to see) visible when the mouse is over them.
const HOVER_BG: floem::peniko::Color = floem::peniko::Color::rgb8(244, 244, 246);

/// Dispatch one editor block to its renderer. Inline-bodied blocks
/// (paragraph, heading) become editable widgets backed by reactive signals;
/// other kinds remain read-only for now.
pub fn block_view(block: &EditorBlock, dnd: DndState, env: &BlockEnv) -> AnyView {
    let block_id = block.id;
    let block_editor = block
        .plugin
        .as_ref()
        .and_then(|m| m.editor.clone())
        .unwrap_or_else(|| Rc::from(descriptor::EDITOR_PARAGRAPH));
    let block_attrs = block
        .plugin
        .as_ref()
        .map(|m| m.attrs.clone())
        .unwrap_or_default();

    // Plugin blocks take precedence: header strip + attr form + body editor.
    // Built-in dispatch only runs when the block isn't plugin-flagged.
    if block.plugin.is_some() {
        let plugin_view = plugin::plugin_block_view(block, env);
        return wrap_block(
            plugin_view,
            block_id,
            dnd,
            env,
            block_editor,
            block_attrs,
        );
    }

    let body = match &block.body {
        BlockBody::Code(text) => {
            let lang = block
                .plugin
                .as_ref()
                .and_then(|m| m.attrs.get("lang"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            code_editor::editable_code_view(text, lang, block_id, env)
        }
        BlockBody::Opaque(_) => {
            // Opaque blocks load from disk with unknown/removed plugin types.
            // Route through the fallback so they're visible and recoverable,
            // not a silent drop or a read-only card with no toolbar.
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
        // Body mismatch — render fallback so content is visible and recoverable.
        _ => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[fallback] block {:?}: body {:?} has no non-plugin renderer",
                block_id, block.body
            );
            fallback::fallback_block_view(block, env.focus_pub).into_any()
        }
    };

    wrap_block(body, block_id, dnd, env, block_editor, block_attrs)
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
    dnd: DndState,
    env: &BlockEnv,
    block_editor: Rc<str>,
    block_attrs: serde_json::Map<String, serde_json::Value>,
) -> AnyView {
    // Capture env fields into owned/copy types so the closures outlive `env`.
    let focus_block = env.focus_pub.block;
    let focus_pub = env.focus_pub;
    let on_action = env.on_action.clone();

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
        let focused = focus_block.get() == Some(block_id);
        let s = s.width_full().border(1.0).border_radius(4.0);
        if focused {
            s.border_color(FOCUS_BORDER)
        } else {
            s.border_color(floem::peniko::Color::TRANSPARENT)
        }
    });

    // Toolbar slot: an in-flow first row that mounts the toolbar only when this
    // block is focused (a fixed-height box then; zero-height otherwise). It must
    // be in-flow — not an absolute overlay — to stay clickable: floem 0.2 gates
    // pointer descent on `layout_rect().with_origin(local_location)`. An
    // absolutely positioned child overflowing *above* its parent (a negative
    // `inset_top`) grows the parent's union height but re-anchors it at the
    // parent's own positive origin, shifting the hit rectangle downward — so the
    // toolbar painted above the block was visible but never hit-tested, and its
    // buttons were dead. Keeping it in flow makes its layout box coincide with
    // where it paints, so clicks land.
    let toolbar_slot = {
        let on_action = on_action.clone();
        dyn_container(
            move || focus_block.get() == Some(block_id),
            move |is_focused| {
                if is_focused {
                    block_toolbar_for(
                        block_id,
                        block_editor.clone(),
                        block_attrs.clone(),
                        focus_pub,
                        on_action.clone(),
                    )
                    .into_any()
                } else {
                    empty().into_any()
                }
            },
        )
        .style(move |s| {
            if focus_block.get() == Some(block_id) {
                s.width_full().height(TOOLBAR_HEIGHT_PX)
            } else {
                s.height(0.)
            }
        })
    };

    v_stack((toolbar_slot, row_with_border))
        .style(move |s| {
            // Keep an 8px gap between blocks. When this block is focused, the
            // in-flow toolbar would push everything below it down by its height;
            // cancel that by pulling the whole block up by exactly that height,
            // so the toolbar floats over the block above and the document never
            // shifts as focus moves between blocks.
            let focused = focus_block.get() == Some(block_id);
            let margin_top = if focused {
                BLOCK_GAP_PX - f64::from(TOOLBAR_HEIGHT_PX)
            } else {
                BLOCK_GAP_PX
            };
            s.width_full().margin_top(margin_top)
        })
        .into_any()
}

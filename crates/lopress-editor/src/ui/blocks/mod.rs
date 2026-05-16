//! Per-block rendering for the editor pane.
//!
//! Paragraph and Heading blocks are dispatched to the editable inline-runs
//! widget, which owns its own `RwSignal<Vec<InlineRun>>` and a caret signal.
//! The signals are created here from the block's initial runs; later tasks
//! will fold edits back into the document model.

pub mod code;
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
use floem::views::{dyn_container, empty, h_stack, v_stack, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Dispatch one editor block to its renderer. Inline-bodied blocks
/// (paragraph, heading) become editable widgets backed by reactive signals;
/// other kinds remain read-only for now.
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
            dnd,
            Rc::clone(&on_undo),
            Rc::clone(&on_redo),
        );
        // The toolbar slot still mounts above plugin blocks so kind / B / I
        // toggles still work on the body editor.
        let toolbar_slot = {
            let on_action = on_action.clone();
            let kind_for_slot = kind.clone();
            dyn_container(
                move || focus_pub.block.get() == Some(block_id),
                move |is_focused| {
                    if is_focused {
                        block_toolbar_for(
                            block_id,
                            kind_for_slot.clone(),
                            focus_pub,
                            on_action.clone(),
                        )
                        .into_any()
                    } else {
                        empty().into_any()
                    }
                },
            )
            .style(|s| s.width_full())
        };
        return v_stack((toolbar_slot, plugin_view))
            .style(|s| s.width_full())
            .into_any();
    }

    let body = match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            paragraph::render_paragraph_editable(
                runs,
                block.id,
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
            )
            .style(|s| s.padding_vert(6.))
            .into_any()
        }
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            heading::render_heading_editable(
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
            .into_any()
        }
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code::render_code(lang, text).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => {
            list::render_list(*ordered, items).into_any()
        }
        (BlockKind::Opaque { type_name }, BlockBody::Opaque(value)) => {
            opaque::render_opaque(type_name, value).into_any()
        }
        // Body/kind mismatch — render nothing.
        _ => empty().into_any(),
    };

    // Anchored toolbar: rendered above this block's body iff this block is
    // the focused one. Uses a `dyn_container` keyed on `focus_pub.block` so
    // it appears/disappears reactively.
    let toolbar_slot = {
        let on_action = on_action.clone();
        let kind_for_slot = kind.clone();
        dyn_container(
            move || focus_pub.block.get() == Some(block_id),
            move |is_focused| {
                if is_focused {
                    block_toolbar_for(
                        block_id,
                        kind_for_slot.clone(),
                        focus_pub,
                        on_action.clone(),
                    )
                    .into_any()
                } else {
                    empty().into_any()
                }
            },
        )
        .style(|s| s.width_full())
    };

    // Hover gutter: shows the drag handle when the user is over this block
    // (or while it's being dragged). PointerEnter/PointerLeave on the
    // h_stack container set the local hover signal.
    let hover: RwSignal<bool> = RwSignal::new(false);
    let handle = drag_handle(block_id, dnd, hover)
        .style(|s| s.width(HANDLE_WIDTH).flex_shrink(0.).items_center());

    let row = h_stack((handle, body.style(|s| s.flex_grow(1.0))))
        .style(|s| s.width_full())
        .on_event(EventListener::PointerEnter, move |_| {
            hover.set(true);
            EventPropagation::Continue
        })
        .on_event(EventListener::PointerLeave, move |_| {
            hover.set(false);
            EventPropagation::Continue
        });

    v_stack((toolbar_slot, row))
        .style(|s| s.width_full())
        .into_any()
}

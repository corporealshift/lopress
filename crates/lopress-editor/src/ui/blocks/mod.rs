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

use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher, LocalSelection};
use crate::ui::toolbar::block_toolbar_for;
use floem::reactive::{RwSignal, SignalGet};
use floem::views::{dyn_container, empty, v_stack, Decorators};
use floem::{AnyView, IntoView};

/// Dispatch one editor block to its renderer. Inline-bodied blocks
/// (paragraph, heading) become editable widgets backed by reactive signals;
/// other kinds remain read-only for now.
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
) -> AnyView {
    let block_id = block.id;
    let kind = block.kind.clone();
    let body = match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            let runs_sig = RwSignal::new(runs.clone());
            let selection_sig = RwSignal::new(LocalSelection::START);
            paragraph::render_paragraph_editable(
                runs_sig,
                selection_sig,
                block.id,
                on_action.clone(),
                focus_target,
                focus_pub,
            )
            .style(|s| s.padding_vert(6.))
            .into_any()
        }
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            let runs_sig = RwSignal::new(runs.clone());
            let selection_sig = RwSignal::new(LocalSelection::START);
            heading::render_heading_editable(
                *level,
                runs_sig,
                selection_sig,
                block.id,
                on_action.clone(),
                focus_target,
                focus_pub,
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

    v_stack((toolbar_slot, body))
        .style(|s| s.width_full())
        .into_any()
}

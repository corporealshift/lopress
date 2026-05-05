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
use crate::ui::blocks::inline_editor::{ActionSink, LocalSelection};
use floem::reactive::RwSignal;
use floem::views::{empty, Decorators};
use floem::{AnyView, IntoView};

/// Dispatch one editor block to its renderer. Inline-bodied blocks
/// (paragraph, heading) become editable widgets backed by reactive signals;
/// other kinds remain read-only for now.
pub fn block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
) -> AnyView {
    match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => {
            let runs_sig = RwSignal::new(runs.clone());
            let selection_sig = RwSignal::new(LocalSelection::START);
            paragraph::render_paragraph_editable(
                runs_sig,
                selection_sig,
                block.id,
                on_action,
                focus_target,
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
                on_action,
                focus_target,
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
    }
}

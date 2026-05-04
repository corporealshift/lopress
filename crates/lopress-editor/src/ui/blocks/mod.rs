//! Read-only block rendering for the editor pane (Task 7).
//!
//! Each `block_view` dispatches an `EditorBlock` to one of the per-kind
//! renderers below. Editing comes in Tasks 8–10.

pub mod code;
pub mod heading;
pub mod inline_editor;
pub mod list;
pub mod opaque;
pub mod paragraph;

use crate::model::types::{BlockBody, BlockKind, EditorBlock};
use floem::views::{empty, Decorators};
use floem::{AnyView, IntoView};

/// Dispatch a read-only block render to the appropriate view.
pub fn block_view(block: &EditorBlock) -> AnyView {
    match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph(runs)
            .style(|s| s.padding_vert(6.))
            .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => {
            heading::render_heading(*level, runs).into_any()
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

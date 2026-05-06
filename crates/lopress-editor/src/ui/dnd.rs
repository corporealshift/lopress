//! Drag-and-drop block reorder.
//!
//! Floem 0.2 ships built-in DnD: a view marked `.draggable()` becomes a drag
//! source on pointer-down + small drift, fires `EventListener::DragStart`
//! when the drag begins, fires `DragOver`/`DragLeave` on hovered targets,
//! and fires `Drop` on whichever view is under the cursor at pointer-up
//! followed by `DragEnd` on the source. Floem doesn't carry a payload with
//! the drag — we identify the dragged block via our own `DndState`.
//!
//! Drop targets are gap strips between blocks (and one above the first /
//! below the last). On a successful drop, the gap fires `BlockAction::Move`
//! with its index; `apply_move` translates that gap index to the right
//! post-removal insert position.

use crate::actions::BlockAction;
use crate::model::types::BlockId;
use crate::ui::blocks::inline_editor::ActionSink;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::style::CursorStyle;
use floem::views::{empty, label, Decorators};
use floem::IntoView;

/// Pane-level reactive state shared by drag handles and gap drop zones.
///
/// `dragging` carries the currently-dragged block id (set on `DragStart`,
/// cleared on `DragEnd` or after a successful drop). `hover_gap` carries
/// the index of the gap currently under the cursor during a drag — used
/// only to render the indicator line.
#[derive(Clone, Copy)]
pub struct DndState {
    pub dragging: RwSignal<Option<BlockId>>,
    pub hover_gap: RwSignal<Option<usize>>,
}

impl DndState {
    pub fn new() -> Self {
        Self {
            dragging: RwSignal::new(None),
            hover_gap: RwSignal::new(None),
        }
    }
}

impl Default for DndState {
    fn default() -> Self {
        Self::new()
    }
}

/// Width reserved for the drag-handle column on the left of every block.
pub const HANDLE_WIDTH: f32 = 20.0;

const HANDLE_COLOR: Color = Color::rgb8(170, 170, 175);
const HANDLE_COLOR_ACTIVE: Color = Color::rgb8(80, 80, 90);
const INDICATOR_COLOR: Color = Color::rgb8(70, 130, 230);

/// The drag handle shown in the left gutter of each block. Visible only when
/// `hover` is true (block is hovered) or this block is currently being
/// dragged. Clicking the handle is treated as a drag start by Floem; on
/// `DragStart` we record `block_id` into `dnd.dragging`.
pub fn drag_handle(block_id: BlockId, dnd: DndState, hover: RwSignal<bool>) -> impl IntoView {
    label(|| "\u{22EE}\u{22EE}".to_string())
        .draggable()
        .on_event(EventListener::DragStart, move |_| {
            dnd.dragging.set(Some(block_id));
            EventPropagation::Continue
        })
        .on_event(EventListener::DragEnd, move |_| {
            // Always clear — drop-outside doesn't fire `Drop`, so the gap
            // handlers wouldn't have cleared it.
            dnd.dragging.set(None);
            dnd.hover_gap.set(None);
            EventPropagation::Continue
        })
        .style(move |s| {
            let visible =
                hover.get() || dnd.dragging.get() == Some(block_id);
            let s = s
                .padding_horiz(4.)
                .font_size(14.)
                .cursor(CursorStyle::Pointer);
            if visible {
                s.color(HANDLE_COLOR)
            } else {
                s.color(Color::TRANSPARENT)
            }
        })
        .dragging_style(|s| s.color(HANDLE_COLOR_ACTIVE))
}

/// A gap drop-target between two blocks (or before the first / after the
/// last). The strip is normally invisible-but-hit-testable (8 logical px
/// tall). During a drag, when the pointer hovers it, a 2 px indicator line
/// appears across the editor column. On `Drop`, emits `BlockAction::Move`
/// with the gap index.
pub fn gap_drop_zone(
    gap_index: usize,
    dnd: DndState,
    on_action: ActionSink,
) -> impl IntoView {
    empty()
        .on_event(EventListener::DragOver, move |_| {
            // Only update when we're actually in a drag and the value would
            // change — DragOver fires on every pointer move while hovered.
            if dnd.dragging.get_untracked().is_some()
                && dnd.hover_gap.get_untracked() != Some(gap_index)
            {
                dnd.hover_gap.set(Some(gap_index));
            }
            EventPropagation::Continue
        })
        .on_event(EventListener::DragLeave, move |_| {
            if dnd.hover_gap.get_untracked() == Some(gap_index) {
                dnd.hover_gap.set(None);
            }
            EventPropagation::Continue
        })
        .on_event(EventListener::Drop, move |_| {
            let Some(block_id) = dnd.dragging.get_untracked() else {
                return EventPropagation::Continue;
            };
            on_action(BlockAction::Move {
                block_id,
                to_index: gap_index,
            });
            dnd.dragging.set(None);
            dnd.hover_gap.set(None);
            EventPropagation::Stop
        })
        .style(move |s| {
            let active = dnd.dragging.get().is_some()
                && dnd.hover_gap.get() == Some(gap_index);
            let s = s.width_full().height(8.);
            if active {
                // Draw the 2 px indicator as a top border, vertically
                // centered by a 3 px top margin.
                s.margin_top(3.).border_top(2.).border_color(INDICATOR_COLOR)
            } else {
                s
            }
        })
}

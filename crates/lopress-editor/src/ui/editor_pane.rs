//! The vertical scrollable editor pane.

use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::block_view;
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use floem::reactive::RwSignal;
use floem::views::{scroll, v_stack_from_iter, Decorators};
use floem::IntoView;

/// Render the editor pane: vertical scroll container, max content width 720
/// logical px, centered, with one block view per `EditorBlock`. `on_action`
/// is the chokepoint that block widgets call for every block-tree mutation;
/// `focus_target`, when set to a block id, hands focus to that block on the
/// next tick.
///
/// Each editable block view also publishes its focus state and signal pair
/// into a pane-level `FocusPublisher`, so a per-block toolbar (Task 12) can
/// render anchored above whichever block currently owns focus.
pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
) -> impl IntoView {
    let focus_pub = FocusPublisher {
        block: RwSignal::new(None),
        signals: RwSignal::new(None),
    };
    let blocks: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| block_view(b, on_action.clone(), focus_target, focus_pub))
        .collect();
    let column = v_stack_from_iter(blocks).style(|s| {
        s.max_width(720.)
            .width_full()
            .margin_horiz(floem::unit::PxPctAuto::Auto)
            .padding(24.)
    });
    scroll(column).style(|s| s.width_full().height_full())
}

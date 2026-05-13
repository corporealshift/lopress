//! The vertical scrollable editor pane.

use crate::actions::BlockAction;
use crate::model::types::{BlockId, EditorDoc};
use crate::ui::blocks::block_view;
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::ui::dnd::{gap_drop_zone, DndState};
use crate::ui::slash_menu::slash_menu;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{dyn_container, empty, scroll, stack, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView};

/// Render the editor pane: vertical scroll container, max content width 720
/// logical px, centered, with one block view per `EditorBlock`. `on_action`
/// is the chokepoint that block widgets call for every block-tree mutation;
/// `focus_target`, when set to a block id, hands focus to that block on the
/// next tick.
///
/// Each editable block view also publishes its focus state and signal pair
/// into a pane-level `FocusPublisher`, so a per-block toolbar (Task 12) can
/// render anchored above whichever block currently owns focus.
///
/// `slash_menu_open` is the pane-level signal that the slash command menu
/// consults; when `Some(block_id)` the menu is rendered as an overlay and a
/// selection emits `BlockAction::ChangeType` against that block.
pub fn editor_pane(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> impl IntoView {
    let focus_pub = FocusPublisher {
        block: RwSignal::new(None),
        editor_and_spans: RwSignal::new(None),
    };
    // Interleave gap drop-zones with block views: gap(0), block(0), gap(1),
    // block(1), …, gap(N). Gap N (after the last block) is the "drop at end"
    // target.
    let mut rows: Vec<AnyView> = Vec::with_capacity(doc.blocks.len() * 2 + 1);
    for (i, b) in doc.blocks.iter().enumerate() {
        rows.push(gap_drop_zone(i, dnd, on_action.clone()).into_any());
        rows.push(block_view(
            b,
            on_action.clone(),
            focus_target,
            focus_pub,
            dnd,
            current_doc,
        ));
    }
    rows.push(gap_drop_zone(doc.blocks.len(), dnd, on_action.clone()).into_any());
    let column = v_stack_from_iter(rows).style(|s| {
        s.max_width(720.)
            .width_full()
            .margin_horiz(floem::unit::PxPctAuto::Auto)
            .padding(24.)
    });
    let scroll_view = scroll(column).style(|s| s.width_full().height_full());

    // Slash menu overlay. Mounts when `slash_menu_open` is `Some(_)`.
    // Anchored placement against a specific block isn't worth the wiring on
    // first cut — a centered overlay is fine for the acceptance criteria.
    let on_action_for_menu = on_action;
    let menu_overlay = dyn_container(
        move || slash_menu_open.get(),
        move |maybe_block| match maybe_block {
            None => empty().into_any(),
            Some(block_id) => {
                let on_action_for_select = on_action_for_menu.clone();
                let on_select = move |new_kind| {
                    on_action_for_select(BlockAction::ChangeType { block_id, new_kind });
                };
                let on_close = move || {
                    slash_menu_open.set(None);
                    focus_target.set(Some(block_id));
                };
                slash_menu(on_select, on_close)
                    .style(|s| s.margin_top(40.).margin_horiz(floem::unit::PxPctAuto::Auto))
                    .into_any()
            }
        },
    )
    .style(|s| {
        s.position(floem::style::Position::Absolute)
            .inset_top(0.)
            .inset_left(0.)
            .width_full()
    });

    stack((scroll_view, menu_overlay)).style(|s| s.width_full().height_full())
}

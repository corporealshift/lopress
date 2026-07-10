//! The vertical scrollable editor pane.

use crate::actions::BlockAction;
use crate::model::types::{BlockId, EditorBlock, EditorDoc, InlineRun};
use crate::ui::blocks::block_view;
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::inline_editor::{ActionSink, ActiveCommitSlot, FocusPublisher};
use crate::ui::dnd::{gap_drop_zone, DndState};
use crate::ui::slash_menu::{slash_menu, SlashChoice};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::views::{
    button, dyn_container, empty, label, scroll, stack, v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use std::rc::Rc;

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
#[allow(clippy::too_many_arguments)]
pub fn editor_pane(
    current_doc: RwSignal<Option<EditorDoc>>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    slash_menu_open: RwSignal<Option<BlockId>>,
    dnd: DndState,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
    on_insert_image: Rc<dyn Fn(BlockId)>,
    inserter_items: Rc<[crate::model::inserter::PluginInserterItem]>,
    link_edit: RwSignal<Option<crate::ui::link_bar::LinkEdit>>,
    active_commit: ActiveCommitSlot,
) -> impl IntoView {
    // The block column is rebuilt on every document mutation: Floem's
    // `create_updater` (behind `dyn_container`) fires on every
    // `current_doc.update()` with no value diffing, so `pane_key` does not
    // actually gate rebuilds — every edit reconstructs the column. The
    // important property here is *structural*: this `dyn_container` is the
    // CHILD of the `scroll` created below, so only the column is destroyed and
    // recreated — the `scroll` node persists and keeps its offset. Previously
    // the whole pane (including `scroll`) was the dyn_container child, so every
    // edit (Enter, ChangeType, table row/col, …) built a fresh scroll node and
    // snapped the viewport back to the top.
    let pane_key = crate::ui::editing::pane_key::build_pane_key(current_doc);
    let on_action_for_col = on_action.clone();
    let column = dyn_container(pane_key, move |maybe_ids| match maybe_ids {
        None => label(|| "No document open. Pick one from the sidebar.".to_string())
            .style(|s| {
                s.width_full()
                    .padding(24.)
                    .color(floem::peniko::Color::rgb8(140, 140, 140))
            })
            .into_any(),
        Some(_) => match current_doc.with_untracked(|d| d.clone()) {
            Some(doc) => block_column(
                &doc,
                on_action_for_col.clone(),
                focus_target,
                dnd,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
                link_edit,
                active_commit,
            ),
            None => empty().into_any(),
        },
    })
    // `width_full()` is load-bearing: this dyn_container sits between the
    // `scroll` and the block column, and a scroll child's width is otherwise
    // indefinite — the column's own `width_full()` (a percentage) then has
    // nothing to resolve against and collapses to min-content, wrapping every
    // block's text at a few characters.
    .style(|s| s.width_full());

    // `min_height(0)` is load-bearing: a flex item's default `min-height:auto`
    // floors it at its content size, so without this the scroll grows to the
    // full document height and never has a viewport smaller than its content
    // — i.e. it can never scroll. See the matching calls up the layout chain
    // in `ui::mod::editing_view`. Created ONCE and stable across rebuilds.
    let scroll_view = scroll(column).style(|s| s.width_full().height_full().min_height(0.));

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
                // Omit "Read more" when the document already has a marker.
                let has_more = current_doc.with_untracked(|d| {
                    d.as_ref().is_some_and(|doc| {
                        doc.blocks
                            .iter()
                            .any(|b| &*b.plugin.block_type_name == "lopress:more")
                    })
                });
                let items: Vec<_> = crate::ui::slash_menu::slash_menu_items()
                    .into_iter()
                    .filter(|(_, choice)| !(has_more && matches!(choice, SlashChoice::ReadMore)))
                    .collect();

                // Append plugin block rows after the built-in entries.
                let mut plugin_rows: Vec<(String, SlashChoice)> = Vec::new();
                for item in inserter_items.iter() {
                    plugin_rows.push((
                        item.title.clone(),
                        SlashChoice::Plugin {
                            type_name: item.type_name.clone(),
                        },
                    ));
                }
                let items: Vec<_> = items.into_iter().chain(plugin_rows).collect();
                let inserter_items_for_select = Rc::clone(&inserter_items);
                let on_insert_image_for_select = on_insert_image.clone();
                let on_select = move |choice: SlashChoice| match choice {
                    SlashChoice::ChangeType { new_editor, attrs } => {
                        on_action_for_select(BlockAction::ChangeType {
                            block_id,
                            new_editor,
                            new_attrs: Box::new(attrs),
                        });
                    }
                    SlashChoice::ReadMore => {
                        on_action_for_select(BlockAction::InsertAfter {
                            anchor: block_id,
                            new_block: Box::new(EditorBlock::read_more()),
                        });
                    }
                    SlashChoice::Image => {
                        on_insert_image_for_select(block_id);
                    }
                    SlashChoice::Separator => {
                        on_action_for_select(BlockAction::InsertAfter {
                            anchor: block_id,
                            new_block: Box::new(EditorBlock::separator()),
                        });
                    }
                    SlashChoice::Table => {
                        on_action_for_select(BlockAction::InsertAfter {
                            anchor: block_id,
                            new_block: Box::new(EditorBlock::table_default()),
                        });
                    }
                    SlashChoice::Plugin { type_name } => {
                        if let Some(item) = inserter_items_for_select
                            .iter()
                            .find(|i| i.type_name.as_ref() == type_name.as_ref())
                        {
                            on_action_for_select(BlockAction::InsertAfter {
                                anchor: block_id,
                                new_block: Box::new(EditorBlock::from_plugin_item(item)),
                            });
                        }
                    }
                };
                let on_close = move || {
                    slash_menu_open.set(None);
                    focus_target.set(Some(block_id));
                };
                slash_menu(items, on_select, on_close)
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

    stack((scroll_view, menu_overlay)).style(|s| s.width_full().height_full().min_height(0.))
}

/// Build the centered column of block views for `doc`. Rebuilt by the pane's
/// inner `dyn_container` on every document mutation; the surrounding `scroll`
/// node is stable, so the scroll offset is preserved across rebuilds.
///
/// Interleaves gap drop-zones with block views: gap(0), block(0), gap(1),
/// block(1), …, gap(N). Gap N (after the last block) is the "drop at end"
/// target. An empty document has no blocks to click into, so it shows a single
/// "add block" button instead.
#[allow(clippy::too_many_arguments)]
fn block_column(
    doc: &EditorDoc,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    dnd: DndState,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
    link_edit: RwSignal<Option<crate::ui::link_bar::LinkEdit>>,
    active_commit: ActiveCommitSlot,
) -> AnyView {
    // `focus_pub` is created per rebuild and shared by this column's block
    // editors (which publish their editor signals into it on focus) and the
    // per-block toolbar (which reads it). It must NOT outlive the column: the
    // editor signals it references live in this same (rebuilt) scope, so a
    // `focus_pub` hoisted to a stable parent would dangle after a rebuild and
    // panic when the toolbar reads the disposed signal.
    let focus_pub = FocusPublisher {
        block: RwSignal::new(None),
        editor_and_spans: RwSignal::new(None),
    };
    let env = BlockEnv {
        on_action: on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
        link_edit,
        active_commit,
    };
    let mut rows: Vec<AnyView> = Vec::with_capacity(doc.blocks.len() * 2 + 1);
    if doc.blocks.is_empty() {
        rows.push(add_block_button(on_action.clone()));
    } else {
        for (i, b) in doc.blocks.iter().enumerate() {
            rows.push(gap_drop_zone(i, dnd, on_action.clone()).into_any());
            rows.push(block_view(b, dnd, &env));
        }
        rows.push(gap_drop_zone(doc.blocks.len(), dnd, on_action.clone()).into_any());
    }
    v_stack_from_iter(rows)
        .style(|s| {
            s.max_width(720.)
                .width_full()
                .margin_horiz(floem::unit::PxPctAuto::Auto)
                .padding(24.)
        })
        .into_any()
}

/// The affordance shown for an empty document: a button that inserts the
/// first paragraph block. `BlockAction::InsertAfter` with an anchor that
/// matches no block appends to the (empty) document — see `apply_insert_after`.
fn add_block_button(on_action: ActionSink) -> AnyView {
    button(label(|| "+ Add a block".to_string()))
        .action(move || {
            on_action(BlockAction::InsertAfter {
                anchor: BlockId::new(),
                new_block: Box::new(EditorBlock::paragraph(vec![InlineRun::plain("")])),
            });
        })
        .style(|s| s.margin_top(8.))
        .into_any()
}

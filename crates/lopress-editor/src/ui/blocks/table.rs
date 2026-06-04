//! Editable GFM table widget (the `editor = "table"` implementation).
//!
//! A `v_stack` of: a contextual control strip (Add/Del Row, Add/Del Column,
//! L/C/R alignment) shown when a cell in this table is focused, then a grid of
//! rows. Each cell is a native `BlockEditorState` mounted via
//! `mount_block_editor` (the same machinery as `list.rs`). A shared
//! `CellHandles` collects every cell's editor signals so any edit rebuilds a
//! fresh `BlockBody::Table` and emits one `EditBlockBody`. Structural changes
//! (rows/columns/alignment) come from the control strip, which dispatches the
//! table `BlockAction`s.

use crate::actions::BlockAction;
use crate::model::style_span::StyleSpan;
use crate::model::sync::{canonicalize_body, rope_and_spans_to_runs};
use crate::model::types::{Align, BlockBody, BlockId, TableCell, TableData, TableRow};
use crate::ui::blocks::editor_registry::EditorContext;
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
    StructuralKey,
};
use crate::ui::blocks::paragraph::BODY_FONT_SIZE;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::Editor;
use floem::views::{
    button, h_stack, h_stack_from_iter, label, v_stack, v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::cell::RefCell;
use std::rc::Rc;

const HEADER_BG: Color = Color::rgb8(238, 238, 244);
const CELL_BORDER: Color = Color::rgb8(214, 214, 222);
const STRIP_BG: Color = Color::rgb8(250, 250, 252);

/// (row, col, editor_sig, spans_sig) for every cell, in row-major order.
type CellHandles = Rc<RefCell<Vec<(usize, usize, RwSignal<Editor>, RwSignal<Vec<StyleSpan>>)>>>;

/// The currently-focused cell within this table, as (row, col). Drives which
/// row/column the control strip operates on.
type FocusedCell = RwSignal<Option<(usize, usize)>>;

/// Rebuild a `TableData` from the live cell buffers, preserving the original
/// `align` and the row/cell ids captured at build time.
fn collect_table(
    handles: &CellHandles,
    align: &[Align],
    row_ids: &[BlockId],
    cell_ids: &[Vec<BlockId>],
) -> TableData {
    let n_rows = row_ids.len();
    let mut rows: Vec<TableRow> = row_ids
        .iter()
        .enumerate()
        .map(|(r, &rid)| TableRow {
            id: rid,
            cells: cell_ids
                .get(r)
                .map(|ids| {
                    ids.iter()
                        .map(|&cid| TableCell {
                            id: cid,
                            runs: vec![],
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    for (r, c, editor_sig, spans_sig) in handles.borrow().iter() {
        if *r >= n_rows {
            continue;
        }
        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
        let spans = spans_sig.get_untracked();
        let rope = Rope::from(text.as_str());
        let runs = rope_and_spans_to_runs(&rope, &spans);
        if let Some(row) = rows.get_mut(*r) {
            if let Some(cell) = row.cells.get_mut(*c) {
                cell.runs = runs;
            }
        }
    }
    TableData {
        align: align.to_vec(),
        rows,
    }
}

/// Build the editable table view.
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
pub fn table_editor_widget(ctx: &EditorContext) -> AnyView {
    let BlockBody::Table(data) = &ctx.block.body else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[fallback] table widget: {:?} has non-table body",
            ctx.block.id
        );
        return crate::ui::blocks::fallback::fallback_block_view(ctx.block, ctx.focus_pub)
            .into_any();
    };
    let block_id = ctx.block.id;
    let on_action = ctx.on_action.clone();
    let focus_target = ctx.focus_target;
    let focus_pub = ctx.focus_pub;
    let current_doc = ctx.current_doc;
    let on_undo = Rc::clone(&ctx.on_undo);
    let on_redo = Rc::clone(&ctx.on_redo);

    let align = data.align.clone();
    let row_ids: Vec<BlockId> = data.rows.iter().map(|r| r.id).collect();
    let cell_ids: Vec<Vec<BlockId>> = data
        .rows
        .iter()
        .map(|r| r.cells.iter().map(|c| c.id).collect())
        .collect();
    let handles: CellHandles = Rc::new(RefCell::new(Vec::new()));
    let focused_cell: FocusedCell = RwSignal::new(None);

    // Shared bits for the commit closure (rebuild whole table body on any cell edit).
    let collect_ctx = (align.clone(), row_ids.clone(), cell_ids.clone());

    let mut row_views: Vec<AnyView> = Vec::with_capacity(data.rows.len());
    for (r, row) in data.rows.iter().enumerate() {
        let mut cell_views: Vec<AnyView> = Vec::with_capacity(row.cells.len());
        for (c, cell) in row.cells.iter().enumerate() {
            let cx = Scope::current();
            // `BODY_FONT_SIZE` is a small positive integer-valued constant,
            // so the cast to `usize` is always safe and never loses data.
            #[allow(clippy::cast_sign_loss)]
            let state = build_block_editor(cx, &cell.runs, BODY_FONT_SIZE as usize);
            let editor_sig = state.editor_sig;
            let spans_sig = state.spans_sig;
            handles.borrow_mut().push((r, c, editor_sig, spans_sig));

            // Track focus → focused_cell, so the strip knows the active row/col.
            let focused_cell_for_cell = focused_cell; // RwSignal is Copy
                                                      // Commit closure: rebuild the full table body from all cells.
            let commit_handles = Rc::clone(&handles);
            let commit_on_action = on_action.clone();
            let (c_align, c_row_ids, c_cell_ids) = collect_ctx.clone();
            let commit: CommitClosure = Rc::new(move || {
                let live = collect_table(&commit_handles, &c_align, &c_row_ids, &c_cell_ids);
                // `BlockBody` derives `PartialEq`; compare whole canonical bodies
                // (mirrors `list.rs::commit_live_if_changed`).
                let live_body = canonicalize_body(&BlockBody::Table(live));
                let differs = current_doc.with_untracked(|maybe| {
                    maybe
                        .as_ref()
                        .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
                        .map(|b| canonicalize_body(&b.body) != live_body)
                        .unwrap_or(false)
                });
                if differs {
                    commit_on_action(BlockAction::EditBlockBody {
                        block_id,
                        new_body: Box::new(live_body),
                        built_in: true,
                    });
                }
            });

            // Structural-key closure: record the focused cell on any keypress
            // and fall through to default. Captured values must be `Clone`
            // so the closure is `Fn` (not just `FnOnce`).
            let sc = focused_cell_for_cell; // RwSignal is Copy
            let sr = r;
            let sc2 = c;
            let structural_key: StructuralKey = Rc::new(move |_kp, _ms| {
                sc.set(Some((sr, sc2)));
                None
            });

            let view = mount_block_editor(
                state,
                cell.id,
                block_id,
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
                commit,
                structural_key,
                /* slash_eligible */ false,
            );

            let is_header = r == 0;
            let focused_cell_for_click = focused_cell;
            let cell_view = view
                .into_any()
                .style(move |s| {
                    let s = s
                        .border(1.)
                        .border_color(CELL_BORDER)
                        .padding_horiz(8.)
                        .padding_vert(8.)
                        // Give rows a comfortable height. The inner editor is a
                        // fixed single-line height (~20px at the 15px body
                        // font); without this floor a cell is only ~28px and
                        // reads as cramped, and an empty cell can collapse
                        // shorter still. `items_center` keeps the one-line text
                        // vertically centered in the taller cell.
                        .min_height(40.)
                        .items_center()
                        .min_width(80.)
                        .flex_grow(1.0);
                    if is_header {
                        s.background(HEADER_BG)
                            .font_weight(floem::text::Weight::SEMIBOLD)
                    } else {
                        s
                    }
                })
                // Record the active cell for the control strip. Do NOT
                // `request_focus` on this wrapper: the inner `editor_view`
                // focuses itself on PointerDown (see `mount_block_editor`), and
                // focusing the wrapper here would steal keyboard focus from the
                // editor — making the cell impossible to type into. This mirrors
                // the list widget, which relies on the same native focus.
                .on_event(EventListener::PointerDown, move |_| {
                    focused_cell_for_click.set(Some((r, c)));
                    EventPropagation::Continue
                });
            cell_views.push(cell_view.into_any());
        }
        row_views.push(
            h_stack_from_iter(cell_views)
                .style(|s| s.width_full())
                .into_any(),
        );
    }

    let grid = v_stack_from_iter(row_views).style(|s| s.width_full());

    let strip = control_strip(block_id, on_action.clone(), focused_cell, focus_pub);

    v_stack((strip, grid))
        .style(|s| s.width_full().padding_vert(4.))
        .into_any()
}

/// A single button view, wrapped in `Rc<dyn Fn() -> AnyView>` so it can be
/// cloned into the rebuild closure of `dyn_container`.
type BtnView = Rc<dyn Fn() -> AnyView>;

/// The in-flow control strip: shown when a cell in this table is focused.
fn control_strip(
    block_id: BlockId,
    on_action: ActionSink,
    focused_cell: FocusedCell,
    focus_pub: FocusPublisher,
) -> AnyView {
    use floem::views::empty;

    let mk = move |lbl: &'static str,
                   make_action: Rc<dyn Fn((usize, usize)) -> Option<BlockAction>>|
          -> BtnView {
        let on_action = on_action.clone();
        let lbl_owned: Rc<String> = Rc::new(lbl.to_string());
        let make_action = Rc::clone(&make_action);
        let focused = focused_cell; // RwSignal is Copy
        Rc::new(move || {
            let lbl = Rc::clone(&lbl_owned);
            let ma = Rc::clone(&make_action);
            let fc = focused; // RwSignal is Copy
            let oa = on_action.clone();
            let lbl_str = lbl.clone();
            button(label(move || lbl_str.clone()))
                .action(move || {
                    if let Some(rc) = fc.get_untracked() {
                        if let Some(act) = ma(rc) {
                            oa(act);
                        }
                    }
                })
                .style(|s| s.padding_horiz(6.).padding_vert(1.).font_size(12.))
                .into_any()
        })
    };

    let add_row = mk(
        "+ Row",
        Rc::new(move |(r, _c)| {
            Some(BlockAction::TableInsertRow {
                block_id,
                at: r + 1,
            })
        }),
    );
    let del_row = mk(
        "− Row",
        Rc::new(move |(r, _c)| Some(BlockAction::TableDeleteRow { block_id, row: r })),
    );
    let add_col = mk(
        "+ Col",
        Rc::new(move |(_r, c)| {
            Some(BlockAction::TableInsertColumn {
                block_id,
                at: c + 1,
            })
        }),
    );
    let del_col = mk(
        "− Col",
        Rc::new(move |(_r, c)| Some(BlockAction::TableDeleteColumn { block_id, col: c })),
    );
    let al_l = mk(
        "L",
        Rc::new(move |(_r, c)| {
            Some(BlockAction::TableSetAlign {
                block_id,
                col: c,
                align: Align::Left,
            })
        }),
    );
    let al_c = mk(
        "C",
        Rc::new(move |(_r, c)| {
            Some(BlockAction::TableSetAlign {
                block_id,
                col: c,
                align: Align::Center,
            })
        }),
    );
    let al_r = mk(
        "R",
        Rc::new(move |(_r, c)| {
            Some(BlockAction::TableSetAlign {
                block_id,
                col: c,
                align: Align::Right,
            })
        }),
    );

    // Gate the strip to only show when this table holds focus.
    floem::views::dyn_container(
        move || focus_pub.block.get() == Some(block_id),
        move |shown| {
            if shown {
                h_stack((
                    (add_row)(),
                    (del_row)(),
                    (add_col)(),
                    (del_col)(),
                    (al_l)(),
                    (al_c)(),
                    (al_r)(),
                ))
                .style(|s| {
                    s.gap(4.)
                        .padding_horiz(6.)
                        .padding_vert(3.)
                        .margin_bottom(4.)
                        .background(STRIP_BG)
                        .border(1.)
                        .border_color(CELL_BORDER)
                        .border_radius(4.)
                })
                .into_any()
            } else {
                empty().into_any()
            }
        },
    )
    .into_any()
}

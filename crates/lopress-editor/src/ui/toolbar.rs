//! Block toolbar — anchored above the focused block.
//!
//! Reads:
//!   - focused block kind (for the type label / cycler)
//!   - the focused block's runs + selection signals (for B/I/code/link
//!     "active" fill states and to mutate via `toggle_inline`)
//!
//! Emits:
//!   - `BlockAction::ChangeType` (via the type-cycler buttons)
//!   - `BlockAction::Delete`
//!   - inline-flag toggles applied directly to the focused block's
//!     `runs`/`selection` signals (matching the inline editor's keyboard
//!     shortcut path)

use crate::actions::BlockAction;
use crate::model::types::{BlockId, BlockKind, InlineRun};
use crate::selection::DocPosition;
use crate::ui::blocks::inline_editor::{
    toggle_inline, ActionSink, Caret, FocusPublisher, InlineFlag, LocalSelection,
};
use crate::ui::sel_ctx::SelectionContext;
use floem::peniko::Color;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{button, h_stack_from_iter, label, Decorators};
use floem::{AnyView, IntoView};

/// Pre-snapshotted view of the toolbar's inputs at one moment in time.
/// Currently only used by tests / external callers — the live toolbar reads
/// directly from the focused widget's signals via `block_toolbar_for`.
pub struct ToolbarState {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub bold_active: bool,
    pub italic_active: bool,
    pub code_active: bool,
    pub link_active: bool,
}

/// Build the toolbar for the currently-focused block. The caller (the
/// per-block `block_view` wrapper) is responsible for only mounting this
/// when its block is in fact the focused one.
///
/// Type selector: rendered as a row of seven small kind buttons (P / H1 /
/// H2 / H3 / Code / UL / OL). The current kind's button is highlighted.
/// Floem 0.2 doesn't ship a stock combobox, and a row of buttons is the
/// simplest interaction that satisfies the acceptance criteria.
pub fn block_toolbar_for(
    block_id: BlockId,
    current_kind: BlockKind,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
    sel_ctx: SelectionContext,
) -> impl IntoView {
    let kinds: Vec<(&'static str, BlockKind)> = vec![
        ("P", BlockKind::Paragraph),
        ("H1", BlockKind::Heading(1)),
        ("H2", BlockKind::Heading(2)),
        ("H3", BlockKind::Heading(3)),
        ("Code", BlockKind::Code { lang: String::new() }),
        ("UL", BlockKind::List { ordered: false }),
        ("OL", BlockKind::List { ordered: true }),
    ];

    let mut buttons: Vec<AnyView> = Vec::with_capacity(kinds.len() + 5);
    for (lbl, kind) in kinds {
        let is_current = same_kind(&current_kind, &kind);
        let lbl_str: String = lbl.to_string();
        let kind_for_action = kind.clone();
        let on_action_for_btn = on_action.clone();
        let btn = button(label(move || lbl_str.clone()))
            .action(move || {
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_kind: kind_for_action.clone(),
                });
            })
            .style(move |s| {
                let s = s.padding_horiz(6.).padding_vert(2.);
                if is_current {
                    s.background(Color::rgb8(210, 220, 240))
                        .font_weight(Weight::SEMIBOLD)
                } else {
                    s
                }
            });
        buttons.push(btn.into_any());
    }

    // Separator between the type selector and the inline-flag toggles.
    buttons.push(separator().into_any());

    // Inline-flag toggle buttons.
    for (lbl, flag) in [
        ("B", InlineFlag::Bold),
        ("I", InlineFlag::Italic),
        ("</>", InlineFlag::Code),
        ("Link", InlineFlag::Link),
    ] {
        buttons.push(toggle_button(lbl, flag, focus_pub, sel_ctx.clone()).into_any());
    }

    buttons.push(separator().into_any());

    // Delete.
    let on_action_for_del = on_action.clone();
    let del_btn = button(label(|| "x".to_string()))
        .action(move || {
            on_action_for_del(BlockAction::Delete { block_id });
        })
        .style(|s| {
            s.padding_horiz(6.)
                .padding_vert(2.)
                .color(Color::rgb8(180, 60, 60))
        });
    buttons.push(del_btn.into_any());

    h_stack_from_iter(buttons).style(|s| {
        s.padding_horiz(6.)
            .padding_vert(4.)
            .gap(4.)
            .background(Color::rgb8(245, 245, 248))
            .border(1.)
            .border_color(Color::rgb8(220, 220, 226))
            .border_radius(4.)
            .margin_bottom(4.)
    })
}

/// One inline-flag toggle button. Active when the (single-block) doc
/// selection inside the focused block has `flag` set on every overlapping
/// run; clicking toggles it. No-op when the selection is collapsed or the
/// selection spans multiple blocks (Task 16 will handle multi-block).
fn toggle_button(
    lbl: &'static str,
    flag: InlineFlag,
    focus_pub: FocusPublisher,
    sel_ctx: SelectionContext,
) -> impl IntoView {
    let lbl_owned = lbl.to_string();

    let sel_ctx_for_label = sel_ctx.clone();
    let lbl_view = label(move || lbl_owned.clone()).style(move |s| {
        let active = flag_active(focus_pub, flag, &sel_ctx_for_label);
        if active {
            s.background(Color::rgb8(210, 220, 240))
                .font_weight(Weight::BOLD)
        } else {
            s
        }
    });

    let sel_ctx_for_action = sel_ctx;
    button(lbl_view)
        .action(move || {
            let Some(runs) = focus_pub.runs.get_untracked() else {
                return;
            };
            let Some(block_id) = focus_pub.block.get_untracked() else {
                return;
            };
            let local = match focused_local_selection(focus_pub, &sel_ctx_for_action) {
                Some(l) => l,
                None => return,
            };
            let mut new_local = local;
            runs.update(|r| {
                new_local = toggle_inline(r, local, flag);
            });
            sel_ctx_for_action.doc_selection.set(crate::selection::DocSelection {
                anchor: DocPosition::new(block_id, new_local.anchor.run, new_local.anchor.offset),
                head: DocPosition::new(block_id, new_local.head.run, new_local.head.offset),
            });
        })
        .style(|s| s.padding_horiz(6.).padding_vert(2.))
}

/// Reads the doc selection and returns it projected as a `LocalSelection` —
/// but only when both endpoints are inside the focused block. Cross-block
/// selections currently fall through to `None` (toolbar shortcuts no-op).
fn focused_local_selection(focus_pub: FocusPublisher, sel_ctx: &SelectionContext) -> Option<LocalSelection> {
    let block_id = focus_pub.block.get_untracked()?;
    let doc_sel = sel_ctx.doc_selection.get_untracked();
    if doc_sel.anchor.block != block_id || doc_sel.head.block != block_id {
        return None;
    }
    Some(LocalSelection {
        anchor: Caret { run: doc_sel.anchor.run, offset: doc_sel.anchor.offset },
        head: Caret { run: doc_sel.head.run, offset: doc_sel.head.offset },
    })
}

/// True if every run inside the focused block's selection has `flag` set.
/// Returns false when nothing is focused, selection is collapsed, or the
/// selection spans multiple blocks.
fn flag_active(focus_pub: FocusPublisher, flag: InlineFlag, sel_ctx: &SelectionContext) -> bool {
    // Track signals for reactivity.
    let _ = sel_ctx.doc_selection.get();
    let runs_opt = focus_pub.runs.get();
    let sel = match focused_local_selection(focus_pub, sel_ctx) {
        Some(s) => s,
        None => return false,
    };
    if sel.is_collapsed() {
        return false;
    }
    let runs = match runs_opt {
        Some(r) => r,
        None => return false,
    };
    runs.with(|runs| every_run_has_flag(runs, sel, flag))
}

/// Walk `runs` from the start of the block to the end, tracking the absolute
/// character span covered by each run, and check whether the portion that
/// overlaps `sel` has `flag` set on every contributing run. The selection
/// must be non-empty.
fn every_run_has_flag(runs: &[InlineRun], sel: LocalSelection, flag: InlineFlag) -> bool {
    let (start, end) = sel.ordered();
    let mut acc = 0usize;
    let mut sel_lo = 0usize;
    let mut sel_hi = 0usize;
    let mut saw_any = false;
    for (i, r) in runs.iter().enumerate() {
        let len = r.text.chars().count();
        if i == start.run {
            sel_lo = acc + start.offset.min(len);
        }
        if i == end.run {
            sel_hi = acc + end.offset.min(len);
        }
        acc += len;
    }
    if sel_lo >= sel_hi {
        return false;
    }
    let mut acc = 0usize;
    for r in runs.iter() {
        let len = r.text.chars().count();
        let run_lo = acc;
        let run_hi = acc + len;
        acc = run_hi;
        // Does this run overlap the selection?
        let overlap_lo = run_lo.max(sel_lo);
        let overlap_hi = run_hi.min(sel_hi);
        if overlap_lo >= overlap_hi {
            continue;
        }
        saw_any = true;
        let has = match flag {
            InlineFlag::Bold => r.bold,
            InlineFlag::Italic => r.italic,
            InlineFlag::Code => r.code,
            InlineFlag::Link => r.link.is_some(),
        };
        if !has {
            return false;
        }
    }
    saw_any
}

fn same_kind(a: &BlockKind, b: &BlockKind) -> bool {
    match (a, b) {
        (BlockKind::Paragraph, BlockKind::Paragraph) => true,
        (BlockKind::Heading(la), BlockKind::Heading(lb)) => la == lb,
        (BlockKind::Code { .. }, BlockKind::Code { .. }) => true,
        (BlockKind::List { ordered: oa }, BlockKind::List { ordered: ob }) => oa == ob,
        (BlockKind::Opaque { type_name: ta }, BlockKind::Opaque { type_name: tb }) => ta == tb,
        _ => false,
    }
}

fn separator() -> impl IntoView {
    use floem::views::empty;
    empty().style(|s| {
        s.width(1.)
            .height(16.)
            .margin_horiz(4.)
            .background(Color::rgb8(210, 210, 215))
    })
}

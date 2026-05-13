//! Block toolbar — anchored above the focused block.
//!
//! Reads:
//!   - focused block kind (for the type label / cycler)
//!   - the focused block's editor + style-span signals (for B/I/code/link
//!     "active" fill states and to mutate via `apply_style_toggle`)
//!
//! Emits:
//!   - `BlockAction::ChangeType` (via the type-cycler buttons)
//!   - `BlockAction::Delete`
//!   - inline-flag toggles applied via `apply_style_toggle`

use crate::actions::BlockAction;
use crate::model::style_span::InlineFlag;
use crate::model::types::{BlockId, BlockKind};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use floem::peniko::Color;
use floem::reactive::{SignalGet, SignalWith};
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
) -> impl IntoView {
    let kinds: Vec<(&'static str, BlockKind)> = vec![
        ("P", BlockKind::Paragraph),
        ("H1", BlockKind::Heading(1)),
        ("H2", BlockKind::Heading(2)),
        ("H3", BlockKind::Heading(3)),
        (
            "Code",
            BlockKind::Code {
                lang: String::new(),
            },
        ),
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
                // Commit current editor text before changing kind.
                if let Some((editor_sig, spans_sig, _)) = focus_pub.editor_and_spans.get_untracked() {
                    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                    let spans = spans_sig.get_untracked();
                    let rope = lapce_xi_rope::Rope::from(text.as_str());
                    let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                    on_action_for_btn(BlockAction::EditInline {
                        block_id,
                        new_runs: new_runs,
                    });
                }
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
        buttons.push(toggle_button(lbl, flag, focus_pub).into_any());
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

/// One inline-flag toggle button. Active when the current editor selection
/// has `flag` set on every overlapping style span; clicking toggles it.
fn toggle_button(
    lbl: &'static str,
    flag: InlineFlag,
    focus_pub: FocusPublisher,
) -> impl IntoView {
    let lbl_owned = lbl.to_string();

    let lbl_view = label(move || lbl_owned.clone()).style(move |s| {
        let active = flag_active(focus_pub, flag);
        if active {
            s.background(Color::rgb8(210, 220, 240))
                .font_weight(Weight::BOLD)
        } else {
            s
        }
    });

    button(lbl_view)
        .action(move || {
            if let Some((editor_sig, spans_sig, style_rev)) =
                focus_pub.editor_and_spans.get_untracked()
            {
                crate::ui::blocks::inline_editor::apply_style_toggle(
                    editor_sig, spans_sig, style_rev, flag,
                );
            }
        })
        .style(|s| s.padding_horiz(6.).padding_vert(2.))
}

/// True if every style span inside the current editor selection has `flag` set.
/// Returns false when nothing is focused or the selection is collapsed.
fn flag_active(focus_pub: FocusPublisher, flag: InlineFlag) -> bool {
    use floem::views::editor::core::cursor::CursorMode;
    // Reactive read so the label updates when selection or spans change
    let Some((editor_sig, spans_sig, _)) = focus_pub.editor_and_spans.get() else {
        return false;
    };
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(offset) => (*offset, *offset),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    if sel_start >= sel_end {
        return false;
    }
    let spans = spans_sig.get_untracked();
    let mut saw_any = false;
    for span in &spans {
        let lo = span.start.max(sel_start);
        let hi = span.end.min(sel_end);
        if lo >= hi {
            continue;
        }
        saw_any = true;
        let has = match flag {
            InlineFlag::Bold => span.bold,
            InlineFlag::Italic => span.italic,
            InlineFlag::Code => span.code,
            InlineFlag::Link => span.link.is_some(),
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

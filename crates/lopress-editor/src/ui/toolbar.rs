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
use crate::model::descriptor;
use crate::model::style_span::InlineFlag;
use crate::model::types::{BlockId, EditorBlock};
use crate::ui::blocks::inline_editor::{
    ActionSink, FocusPublisher, SelectionRestore, SelectionRestoreSlot,
};
use crate::ui::icons::{self, icon};
use crate::ui::link_bar::LinkEdit;
use crate::ui::theme;
use floem::event::EventListener;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{button, h_stack_from_iter, label, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Pre-snapshotted view of the toolbar's inputs at one moment in time.
const BUTTON_HEIGHT: f64 = 26.0;
const BUTTON_MIN_WIDTH: f64 = 28.0;
const DELETE_ICON_SIZE: f32 = 12.0;
const SEPARATOR_HEIGHT: f64 = 16.0;
const SEPARATOR_MARGIN: f64 = 5.0;

/// Currently only used by tests / external callers — the live toolbar reads
/// directly from the focused widget's signals via `block_toolbar_for`.
pub struct ToolbarState {
    pub block_id: BlockId,
    pub editor: Rc<str>,
    pub bold_active: bool,
    pub italic_active: bool,
    pub code_active: bool,
    pub link_active: bool,
}

/// Build the toolbar for the currently-focused block. The caller (the
/// per-block `block_view` wrapper) is responsible for only mounting this
/// when its block is in fact the focused one.
///
/// Type selector: rendered as a row of buttons projected from the
/// descriptor table. The current kind's button is highlighted.
///
/// # Panics
///
/// Panics if any descriptor's `default_block` produces a block without
/// `PluginMeta` or without `editor: Some` — this is a programming error
/// since all built-in constructors stamp plugin meta.
#[allow(clippy::unwrap_used)] // safe: built-in PluginMeta always has editor: Some
#[allow(clippy::type_complexity)] // builder threads the attrs map plus the focus/action sinks
pub fn block_toolbar_for(
    block_id: BlockId,
    current_editor: Rc<str>,
    current_attrs: serde_json::Map<String, serde_json::Value>,
    focus_pub: FocusPublisher,
    on_action: ActionSink,
    link_edit: RwSignal<Option<LinkEdit>>,
    restore_selection: SelectionRestoreSlot,
) -> impl IntoView {
    let entries: Vec<(&'static str, fn() -> EditorBlock)> =
        descriptor::toolbar_menu_entries().to_vec();

    let mut buttons: Vec<AnyView> = Vec::with_capacity(entries.len() + 5);
    for (lbl, default_block_fn) in entries {
        let block = default_block_fn();
        let meta = &block.plugin;
        let entry_editor = meta.editor.as_ref().unwrap().clone();
        let entry_attrs = meta.attrs.clone();
        let is_current =
            button_matches_current(&entry_editor, &entry_attrs, &current_editor, &current_attrs);
        let is_inline = is_inline_editor(&entry_editor);
        let lbl_str: String = lbl.to_string();
        let on_action_for_btn = on_action.clone();
        let btn = button(label(move || lbl_str.clone()))
            .on_event_stop(EventListener::PointerDown, move |_| {
                // Pre-commit the current block's body shape before changing kind.
                // Only inline-bodied blocks (Paragraph/Heading) have an active
                // editor_and_spans to read; for non-inline kinds (Code/List) the
                // editor_and_spans is None (or belongs to a different block), so
                // we skip the pre-commit — the body shape already matches what
                // ChangeType expects.
                if is_inline {
                    if let Some((editor_sig, spans_sig, _, _)) =
                        focus_pub.editor_and_spans.get_untracked()
                    {
                        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                        let spans = spans_sig.get_untracked();
                        let rope = lapce_xi_rope::Rope::from(text.as_str());
                        let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                        on_action_for_btn(BlockAction::EditBlockBody {
                            block_id,
                            new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                            built_in: true, // Built-in toolbar type-selector pre-commit.
                        });
                    }
                }
                on_action_for_btn(BlockAction::ChangeType {
                    block_id,
                    new_editor: entry_editor.clone(),
                    new_attrs: Box::new(entry_attrs.clone()),
                });
            })
            .style(move |s| {
                let s = toolbar_button(s);
                if is_current {
                    pressed(s).font_weight(Weight::SEMIBOLD)
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
        buttons.push(
            toggle_button(lbl, flag, focus_pub, block_id, link_edit, restore_selection).into_any(),
        );
    }

    buttons.push(separator().into_any());

    // Table insert button — distinct from the kind-cycler: it inserts a fresh
    // table after the focused block rather than converting the block.
    let on_action_for_table = on_action.clone();
    let table_btn = button(label(|| "Table".to_string()))
        .on_event_stop(EventListener::PointerDown, move |_| {
            on_action_for_table(table_insert_action(block_id));
        })
        .style(toolbar_button);
    buttons.push(table_btn.into_any());

    // Delete.
    let on_action_for_del = on_action.clone();
    let del_btn = button(icon(icons::CLOSE, DELETE_ICON_SIZE, theme::ACCENT))
        .on_event_stop(EventListener::PointerDown, move |_| {
            on_action_for_del(BlockAction::Delete { block_id });
        })
        .style(toolbar_button);
    buttons.push(del_btn.into_any());

    let button_row = h_stack_from_iter(buttons).style(|s| {
        s.padding(theme::SPACE_4)
            .gap(1.)
            .items_center()
            .apply(theme::floating(floem::style::Style::new()))
    });

    // The URL entry UI is the pane-level `link_bar` (see `ui::link_bar`); it
    // lives outside this rebuilding toolbar so it survives the focus-loss commit
    // that clicking a toolbar button triggers. The "Link" button below only
    // opens that bar with the captured selection.
    button_row
}

/// One inline-flag toggle button. Active when the current editor selection
/// has `flag` set on every overlapping style span; clicking toggles it.
fn toggle_button(
    lbl: &'static str,
    flag: InlineFlag,
    focus_pub: FocusPublisher,
    block_id: BlockId,
    link_edit: RwSignal<Option<LinkEdit>>,
    restore_selection: SelectionRestoreSlot,
) -> impl IntoView {
    let lbl_owned = lbl.to_string();

    let lbl_view = label(move || lbl_owned.clone());

    button(lbl_view)
        .on_event_stop(EventListener::PointerDown, move |_| {
            if let Some((editor_sig, spans_sig, style_rev, editor_block)) =
                focus_pub.editor_and_spans.get_untracked()
            {
                // The Link button opens the pane-level URL bar (which survives
                // the focus-loss commit/rebuild this click triggers); other
                // flags toggle the span style directly.
                if flag == InlineFlag::Link {
                    crate::ui::blocks::inline_editor::open_link_editor(
                        editor_sig, spans_sig, block_id, link_edit,
                    );
                } else if let Some((start, end)) =
                    crate::ui::blocks::inline_editor::apply_style_toggle(
                        editor_sig, spans_sig, style_rev, flag,
                    )
                {
                    // This click blurs the editor and its commit rebuilds the
                    // column; have the rebuilt editor take the selection back.
                    restore_selection.set(Some(SelectionRestore {
                        block_id: editor_block,
                        start,
                        end,
                    }));
                }
            }
        })
        .style(move |s| {
            let s = toolbar_button(s);
            // Instrument Sans has no semibold italic, so a pressed I keeps its
            // weight. Asking for that face would fall back to a system font.
            let s = match flag {
                InlineFlag::Bold => s.font_weight(Weight::BOLD),
                InlineFlag::Italic => s.font_style(floem::text::Style::Italic),
                InlineFlag::Code | InlineFlag::Link => s,
            };
            if flag_active(focus_pub, flag) {
                pressed(s)
            } else {
                s
            }
        })
}

/// A toolbar button at rest. Colours and hover come from the root style.
fn toolbar_button(s: floem::style::Style) -> floem::style::Style {
    s.height(BUTTON_HEIGHT)
        .min_width(BUTTON_MIN_WIDTH)
        .padding_horiz(theme::SPACE_6)
        .padding_vert(0.)
        .border_radius(theme::CORNER_SM)
        .font_size(theme::UI_SMALL)
}

/// The look of the current block type and of an active inline style.
fn pressed(s: floem::style::Style) -> floem::style::Style {
    s.background(theme::ACCENT_TINT)
        .color(theme::ACCENT)
        .hover(|s| s.background(theme::ACCENT_TINT))
        .focus(|s| s.hover(|s| s.background(theme::ACCENT_TINT)))
}

/// True if every style span inside the current editor selection has `flag` set.
/// Returns false when nothing is focused or the selection is collapsed.
fn flag_active(focus_pub: FocusPublisher, flag: InlineFlag) -> bool {
    use floem::views::editor::core::cursor::CursorMode;
    // Reactive read so the label updates when selection or spans change
    let Some((editor_sig, spans_sig, _, _)) = focus_pub.editor_and_spans.get() else {
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

/// True when the button's default block matches the current block's identity.
fn button_matches_current(
    entry_editor: &str,
    entry_attrs: &serde_json::Map<String, serde_json::Value>,
    current_editor: &str,
    current_attrs: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    entry_editor == current_editor && entry_attrs == current_attrs
}

/// True when the editor key corresponds to an inline-bodied block.
fn is_inline_editor(editor: &str) -> bool {
    matches!(
        editor,
        descriptor::EDITOR_PARAGRAPH | descriptor::EDITOR_HEADING
    )
}

/// The action the toolbar's Table button dispatches: insert a fresh default
/// table immediately after `block_id`. Extracted so it can be unit-tested
/// without driving the UI.
fn table_insert_action(block_id: BlockId) -> BlockAction {
    BlockAction::InsertAfter {
        anchor: block_id,
        new_block: Box::new(crate::model::types::EditorBlock::table_default()),
    }
}

fn separator() -> impl IntoView {
    use floem::views::empty;
    empty().style(|s| {
        s.width(1.)
            .height(SEPARATOR_HEIGHT)
            .margin_horiz(SEPARATOR_MARGIN)
            .background(theme::RULE_SOFT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inline_editor_paragraph_and_heading() {
        assert!(is_inline_editor(descriptor::EDITOR_PARAGRAPH));
        assert!(is_inline_editor(descriptor::EDITOR_HEADING));
        assert!(!is_inline_editor(descriptor::EDITOR_CODE));
        assert!(!is_inline_editor(descriptor::EDITOR_LIST));
    }

    #[test]
    fn toolbar_entries_match_expected_order() {
        let entries = descriptor::toolbar_menu_entries();
        let labels: Vec<&str> = entries.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec!["P", "H1", "H2", "H3", "H4", "H5", "H6", "Code", "UL", "OL"]
        );
    }

    #[test]
    fn table_button_inserts_a_table_after_block() {
        let id = BlockId::new();
        let action = table_insert_action(id);
        match action {
            BlockAction::InsertAfter { anchor, new_block } => {
                assert_eq!(anchor, id);
                let meta = &new_block.plugin;
                assert_eq!(&*meta.block_type_name, "table");
            }
            _ => panic!("expected InsertAfter"),
        }
    }
}

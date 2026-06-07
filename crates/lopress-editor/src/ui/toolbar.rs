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
use crate::model::style_span::{InlineFlag, StyleSpan};
use crate::model::types::{BlockId, EditorBlock};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::editor::Editor;
use floem::views::{
    button, dyn_container, empty, h_stack, h_stack_from_iter, label, text_input, v_stack,
    Decorators,
};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Pre-snapshotted view of the toolbar's inputs at one moment in time.
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

    // Table insert button — distinct from the kind-cycler: it inserts a fresh
    // table after the focused block rather than converting the block.
    let on_action_for_table = on_action.clone();
    let table_btn = button(label(|| "Table".to_string()))
        .on_event_stop(EventListener::PointerDown, move |_| {
            on_action_for_table(table_insert_action(block_id));
        })
        .style(|s| s.padding_horiz(6.).padding_vert(2.));
    buttons.push(table_btn.into_any());
    buttons.push(separator().into_any());

    // Delete.
    let on_action_for_del = on_action.clone();
    let del_btn = button(label(|| "x".to_string()))
        .on_event_stop(EventListener::PointerDown, move |_| {
            on_action_for_del(BlockAction::Delete { block_id });
        })
        .style(|s| {
            s.padding_horiz(6.)
                .padding_vert(2.)
                .color(Color::rgb8(180, 60, 60))
        });
    buttons.push(del_btn.into_any());

    let button_row = h_stack_from_iter(buttons).style(|s| {
        s.padding_horiz(8.)
            .padding_vert(4.)
            .gap(4.)
            .background(Color::rgb8(252, 252, 254))
            .border(1.)
            .border_color(Color::rgb8(220, 220, 226))
            .border_radius(6.)
            .margin_bottom(6.)
    });

    let on_action_for_url = on_action.clone();
    let url_row = dyn_container(
        move || {
            focus_pub
                .editor_and_spans
                .get()
                .and_then(|(_, _, _, url)| url.get())
        },
        move |maybe_url| match maybe_url {
            None => empty().into_any(),
            Some(current_url) => {
                let url_buf: RwSignal<String> = RwSignal::new(current_url);
                let on_action_commit = on_action_for_url.clone();
                let commit = move || {
                    if let Some((editor_sig, spans_sig, _, url_sig)) =
                        focus_pub.editor_and_spans.get_untracked()
                    {
                        let url = url_buf.get_untracked();
                        write_url_to_selection(editor_sig, spans_sig, &url);
                        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                        let spans = spans_sig.get_untracked();
                        let rope = lapce_xi_rope::Rope::from(text.as_str());
                        let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                        on_action_commit(BlockAction::EditBlockBody {
                            block_id,
                            new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                            built_in: true, // Built-in toolbar URL commit.
                        });
                        url_sig.set(None);
                    }
                };
                let commit_for_key = commit.clone();
                let on_action_remove = on_action_for_url.clone();
                let remove = move || {
                    if let Some((editor_sig, spans_sig, style_rev, url_sig)) =
                        focus_pub.editor_and_spans.get_untracked()
                    {
                        crate::ui::blocks::inline_editor::apply_style_toggle(
                            editor_sig,
                            spans_sig,
                            style_rev,
                            InlineFlag::Link,
                        );
                        url_sig.set(None);
                        let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
                        let spans = spans_sig.get_untracked();
                        let rope = lapce_xi_rope::Rope::from(text.as_str());
                        let new_runs = crate::model::sync::rope_and_spans_to_runs(&rope, &spans);
                        on_action_remove(BlockAction::EditBlockBody {
                            block_id,
                            new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
                            built_in: true, // Built-in toolbar URL remove.
                        });
                    }
                };
                h_stack((
                    text_input(url_buf)
                        .placeholder("https://…")
                        .on_event_stop(EventListener::KeyDown, move |e: &Event| {
                            if let Event::KeyDown(k) = e {
                                if matches!(k.key.logical_key, Key::Named(NamedKey::Enter)) {
                                    commit_for_key();
                                }
                            }
                        })
                        .style(|s| s.flex_grow(1.0).font_size(13.)),
                    button(label(|| "Remove".to_string())).on_event_stop(
                        EventListener::PointerDown,
                        move |_| {
                            remove();
                        },
                    ),
                ))
                .style(|s| s.gap(4.).width_full().padding_horiz(6.).padding_vert(4.))
                .into_any()
            }
        },
    )
    .style(|s| s.width_full());

    v_stack((button_row, url_row)).style(|s| s.width_full())
}

/// One inline-flag toggle button. Active when the current editor selection
/// has `flag` set on every overlapping style span; clicking toggles it.
fn toggle_button(lbl: &'static str, flag: InlineFlag, focus_pub: FocusPublisher) -> impl IntoView {
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
        .on_event_stop(EventListener::PointerDown, move |_| {
            if let Some((editor_sig, spans_sig, style_rev, _)) =
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
            .height(16.)
            .margin_horiz(4.)
            .background(Color::rgb8(210, 210, 215))
    })
}

/// Write `url` into every link-bearing style span that overlaps the editor's
/// current selection.
fn write_url_to_selection(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    url: &str,
) {
    use floem::views::editor::core::cursor::CursorMode;
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(o) => (*o, *o),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    let url_owned = url.to_owned();
    spans_sig.update(|spans| {
        for span in spans.iter_mut() {
            let lo = span.start.max(sel_start);
            let hi = span.end.min(sel_end);
            if lo < hi && span.link.is_some() {
                span.link = Some(url_owned.clone());
            }
        }
    });
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

//! Editable list rendering — the canonical `editor = "list"` implementation.
//!
//! Each `ListItem` gets its own native `BlockEditorState`. The view is a
//! `v_stack` of `[bullet/number] [item editor]` rows. Per-item keys handle
//! splitting, merging, and cross-item / cross-block navigation.

use crate::actions::BlockAction;
use crate::model::sync::rope_and_spans_to_runs;
use crate::model::types::{BlockId, EditorDoc, ListItem};
use crate::ui::blocks::inline_editor::{build_block_editor, ActionSink, FocusPublisher};
use crate::ui::blocks::paragraph::BODY_FONT_SIZE;
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::default_key_handler;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::view::editor_container_view;
use floem::views::editor::Editor;
use floem::views::{h_stack, stack, text, v_stack_from_iter, Decorators};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::rc::Rc;

/// Build the editable list view for a list block.
///
/// `on_undo` / `on_redo` are passed in so list items inherit Ctrl+Z/Y from
/// the shared editor mount in stage 4 task 3. Stage 4 task 2 only plumbs
/// them through; the per-item editor doesn't read them yet.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    unused_variables
)]
pub fn editable_list_view(
    items: &[ListItem],
    block_id: BlockId,
    ordered: bool,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    let item_ids: Rc<Vec<BlockId>> = Rc::new(items.iter().map(|it| it.id).collect());
    let count = items.len();
    let rows: Vec<AnyView> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let prefix = if ordered {
                format!("{}.", idx + 1)
            } else {
                "•".to_string()
            };
            let editor = list_item_editor(
                &item.runs,
                block_id,
                item.id,
                idx,
                count,
                Rc::clone(&item_ids),
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
            );
            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                editor.style(|s| s.flex_grow(1.0)),
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
        })
        .collect();
    v_stack_from_iter(rows)
        .style(|s| s.padding_vert(4.).padding_left(8.).width_full())
        .into_any()
}

/// One list item's native editor: a `BlockEditorState` plus a list-specific
/// key handler for splitting, merging, and navigation.
// `BODY_FONT_SIZE` is a small positive integer-valued constant, so the
// f32->usize conversion is exact; line counts are tiny so usize->f32 is too.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn list_item_editor(
    runs: &[crate::model::types::InlineRun],
    block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> AnyView {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, BODY_FONT_SIZE as usize);
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let style_rev = state.style_rev;
    let text_sig = state.text_sig;
    let link_url_sig = state.link_url_sig;

    let default_kp_handler = default_key_handler(editor_sig);
    let on_action_for_key = on_action;

    let view = editor_container_view(
        editor_sig,
        move |_| editor_sig.with_untracked(|ed| ed.active.get()),
        move |kp, ms| {
            let result = handle_list_item_key(
                kp,
                ms,
                editor_sig,
                spans_sig,
                block_id,
                item_id,
                item_index,
                item_count,
                &item_ids,
                &on_action_for_key,
                focus_target,
                current_doc,
            );
            if result == CommandExecuted::Yes {
                result
            } else {
                default_kp_handler(kp, ms)
            }
        },
    );

    // Publish focus: the list *block* (not the item) owns the toolbar slot,
    // so report `block_id` while exposing this item's editor handles.
    create_effect(move |_| {
        let is_active = editor_sig.with(|ed| ed.active.get());
        if is_active {
            focus_pub.block.set(Some(block_id));
            focus_pub
                .editor_and_spans
                .set(Some((editor_sig, spans_sig, style_rev, link_url_sig)));
        }
    });

    // Programmatic focus when `focus_target` names this item. Item 0 also
    // answers to the list *block* id, so navigation that lands on the list
    // as a whole (Ctrl+Home/End, Page keys, cross-block arrows) puts the
    // cursor in the first item instead of dropping it.
    create_effect(move |_| {
        let target = focus_target.get();
        if target == Some(item_id) || (item_index == 0 && target == Some(block_id)) {
            editor_sig.with_untracked(|ed| {
                if let Some(view_id) = ed.editor_view_id.get_untracked() {
                    view_id.request_focus();
                    view_id.scroll_to(None);
                }
            });
            focus_target.set(None);
        }
    });

    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    stack((view,))
        .style(move |s| {
            let lines = text_sig.get().split('\n').count().max(1) as f32;
            s.class(GutterClass, |s| s.hide())
                .width_full()
                .height(lines * line_height)
        })
        .into_any()
}

/// Write the item's current editor text back to the document.
fn commit_list_item(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<crate::model::style_span::StyleSpan>>,
    block_id: BlockId,
    item_id: BlockId,
    on_action: &ActionSink,
) {
    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditListItem {
        block_id,
        item_id,
        new_runs,
    });
}

/// List-item key handling: Enter splits, Backspace-at-0 merges, ↑/↓ navigate.
#[allow(clippy::too_many_arguments)]
fn handle_list_item_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<crate::model::style_span::StyleSpan>>,
    block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: &[BlockId],
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> CommandExecuted {
    use floem::keyboard::{Key, NamedKey};

    let shift = ms.shift();
    let ctrl_or_cmd = ms.control() || ms.meta();
    if ctrl_or_cmd {
        // Ctrl/Cmd shortcuts are not handled at the item level; let the
        // default editor handler deal with them.
        return CommandExecuted::No;
    }

    match &kp.key {
        // Shift+Enter — soft line break within the item.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => {
            editor_sig.with_untracked(|ed| {
                ed.doc().receive_char(ed, "\n");
            });
            CommandExecuted::Yes
        }

        // Enter — commit, then split this item at the cursor.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) => {
            let byte_offset =
                editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            on_action(BlockAction::SplitListItem {
                block_id,
                item_id,
                byte_offset,
                new_block_id: None,
            });
            CommandExecuted::Yes
        }

        // Backspace at offset 0 — merge with the previous item, or with the
        // block before the list when this is the first item.
        KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
            let offset = editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            if offset != 0 {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index > 0 {
                on_action(BlockAction::MergeListItemWithPrev { block_id, item_id });
            } else {
                on_action(BlockAction::MergeWithPrev { block_id });
            }
            CommandExecuted::Yes
        }

        // ↑ on the first visual line — move to the previous item, or to the
        // block before the list.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
            let on_first = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
            });
            if !on_first {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index > 0 {
                if let Some(prev) = item_ids.get(item_index - 1) {
                    focus_target.set(Some(*prev));
                }
            } else {
                let prev_block = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
                });
                if let Some(id) = prev_block {
                    focus_target.set(Some(id));
                }
            }
            CommandExecuted::Yes
        }

        // ↓ on the last visual line — move to the next item, or to the block
        // after the list.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
            let on_last = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                vline.0 == ed.last_vline().0
            });
            if !on_last {
                return CommandExecuted::No;
            }
            commit_list_item(editor_sig, spans_sig, block_id, item_id, on_action);
            if item_index + 1 < item_count {
                if let Some(next) = item_ids.get(item_index + 1) {
                    focus_target.set(Some(*next));
                }
            } else {
                let next_block = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    d.blocks.get(i + 1).map(|b| b.id)
                });
                if let Some(id) = next_block {
                    focus_target.set(Some(id));
                }
            }
            CommandExecuted::Yes
        }

        _ => CommandExecuted::No,
    }
}

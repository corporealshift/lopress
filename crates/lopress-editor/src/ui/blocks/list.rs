//! Editable list rendering — the canonical `editor = "list"` implementation.
//!
//! Each `ListItem` gets its own native `BlockEditorState` mounted through
//! the shared `mount_block_editor`. The view is a `v_stack` of
//! `[bullet/number] [item editor]` rows. A per-list `ItemHandles` collects
//! every item's editor signals so every structural list mutation (Enter to
//! split, Backspace to merge, arrow at boundary, Ctrl+Home/End, etc.) can
//! build a fresh `BlockBody::List` from the live buffer of *every* item and
//! emit a single `EditBlockBody` — the data-loss bug from
//! `docs/superpowers/ideas/2026-05-18-list-item-uncommitted-edit-loss.md`
//! is therefore structurally impossible.

use crate::actions::{split_item_at_with_id, BlockAction};
use crate::model::style_span::StyleSpan;
use crate::model::sync::{canonicalize_body, rope_and_spans_to_runs};
use crate::model::types::{BlockBody, BlockId, EditorDoc, InlineRun, ListItem};
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, FocusPublisher,
    StructuralKey,
};
use crate::ui::blocks::paragraph::BODY_FONT_SIZE;
use crate::ui::editing::focus::defer_focus;
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::Editor;
use floem::views::{
    dyn_container, empty, h_stack, label, stack, text, v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use lapce_xi_rope::Rope;
use std::cell::RefCell;
use std::rc::Rc;

/// Greyed hint text shown in empty list items so users see they're editable.
const EMPTY_ITEM_PLACEHOLDER: &str = "Empty item — type to fill";

/// Per-list shared collection of every item's editor signals. Each
/// `list_item_editor` call pushes its handle here at construction; the
/// `commit` and `structural_key` closures walk the handles in order to
/// build a fresh `BlockBody::List` from every item's live buffer.
type ItemHandles = Rc<RefCell<Vec<(BlockId, RwSignal<Editor>, RwSignal<Vec<StyleSpan>>)>>>;

/// Walk the per-item handles and synthesise a `Vec<ListItem>` from each
/// item's live editor buffer. Item ids are preserved from the handle.
fn collect_items(handles: &ItemHandles) -> Vec<ListItem> {
    handles
        .borrow()
        .iter()
        .map(|(item_id, editor_sig, spans_sig)| {
            let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
            let spans = spans_sig.get_untracked();
            let rope = Rope::from(text.as_str());
            let runs = rope_and_spans_to_runs(&rope, &spans);
            ListItem { id: *item_id, runs }
        })
        .collect()
}

/// If `live` differs from the list block's body in the document model,
/// emit an `EditBlockBody` committing it. Used to flush typed-but-
/// uncommitted text into the model as its own undo entry — distinct from
/// any structural change that follows. When `live` already matches the
/// model this is a no-op (no emit, no rebuild, no undo entry).
fn commit_live_if_changed(
    live: &[ListItem],
    list_block_id: BlockId,
    on_action: &ActionSink,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    let differs = current_doc.with_untracked(|maybe| {
        maybe
            .as_ref()
            .and_then(|d| d.blocks.iter().find(|b| b.id == list_block_id))
            // Compare in canonical form: `live` is canonical (collect_items
            // produces canonical runs), so the stored body must be
            // canonicalized too — otherwise a structurally-identical body
            // looks like a change and emits a phantom no-op EditBlockBody.
            .map(|b| {
                !matches!(
                    canonicalize_body(&b.body),
                    BlockBody::List(items) if items.as_slice() == live
                )
            })
            .unwrap_or(false)
    });
    if differs {
        on_action(BlockAction::EditBlockBody {
            block_id: list_block_id,
            new_body: Box::new(BlockBody::List(live.to_vec())),
            built_in: true, // Built-in list editor widget.
        });
    }
}

/// Commit every item's live buffer into the model as one `EditBlockBody`
/// (skipped when nothing changed). Called as the `commit` closure passed
/// to `mount_block_editor` — so Ctrl+Z flushes pending typing before
/// undoing — and from the structural-key navigation branches that move
/// focus away from the list.
fn emit_list_commit(
    handles: &ItemHandles,
    list_block_id: BlockId,
    on_action: &ActionSink,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    let live = collect_items(handles);
    commit_live_if_changed(&live, list_block_id, on_action, current_doc);
}

/// Build the editable list view for a list block.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
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
    let handles: ItemHandles = Rc::new(RefCell::new(Vec::with_capacity(count)));
    let rows: Vec<AnyView> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let prefix = if ordered {
                format!("{}.", idx + 1)
            } else {
                "•".to_string()
            };
            let (editor, editor_sig) = list_item_editor(
                &item.runs,
                block_id,
                item.id,
                idx,
                count,
                Rc::clone(&item_ids),
                Rc::clone(&handles),
                on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                Rc::clone(&on_undo),
                Rc::clone(&on_redo),
            );
            let editor_sig_for_overlay = editor_sig;
            let placeholder_overlay = dyn_container(
                move || editor_sig_for_overlay.with(|ed| ed.doc().text().is_empty()),
                move |is_empty| {
                    if is_empty {
                        label(|| EMPTY_ITEM_PLACEHOLDER.to_string())
                            .style(|s| {
                                s.color(floem::peniko::Color::rgb8(160, 160, 160))
                                    .font_size(15.)
                                    .padding_horiz(2.)
                                    .position(floem::style::Position::Absolute)
                                    .inset_left(0.)
                                    .inset_top(0.)
                            })
                            .into_any()
                    } else {
                        empty().into_any()
                    }
                },
            );

            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                floem::views::stack((
                    editor.style(|s| s.flex_grow(1.0).width_full()),
                    placeholder_overlay,
                ))
                .style(|s| s.flex_grow(1.0)),
            ))
            .style(|s| s.padding_vert(2.).width_full())
            .into_any()
        })
        .collect();
    v_stack_from_iter(rows)
        .style(|s| s.padding_vert(4.).padding_left(8.).width_full())
        .into_any()
}

/// One list item: a `BlockEditorState` mounted through `mount_block_editor`
/// with a list-specific structural-key callback (per spec section 2) and a
/// batched-commit closure that constructs a complete `BlockBody::List`
/// from every item's live buffer on demand.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn list_item_editor(
    runs: &[InlineRun],
    list_block_id: BlockId,
    item_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> (AnyView, RwSignal<Editor>) {
    let cx = Scope::current();
    let state = build_block_editor(cx, runs, BODY_FONT_SIZE as usize);
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let text_sig = state.text_sig;

    // Register this item's editor with the shared handles so every
    // structural callback can read all items' live buffers.
    handles.borrow_mut().push((item_id, editor_sig, spans_sig));

    // Batched commit: flush every item's live buffer into a fresh
    // BlockBody::List and emit a single EditBlockBody (skipped when nothing
    // changed). Used by the shared default handler before Ctrl+Z/Y and
    // focus-changing shortcuts.
    let commit_handles = Rc::clone(&handles);
    let commit_on_action = on_action.clone();
    let commit: CommitClosure = Rc::new(move || {
        emit_list_commit(
            &commit_handles,
            list_block_id,
            &commit_on_action,
            current_doc,
        );
    });

    // List-specific structural-key callback per spec section 2.
    let structural_key = make_list_structural_key(
        list_block_id,
        item_index,
        item_count,
        Rc::clone(&item_ids),
        Rc::clone(&handles),
        editor_sig,
        on_action.clone(),
        focus_target,
        current_doc,
    );

    let view = mount_block_editor(
        state,
        item_id,
        list_block_id,
        on_action,
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
        commit,
        structural_key,
        /* slash_eligible */ false,
    );

    // Item 0 also answers to the list *block* id, so navigation that lands
    // on the list as a whole (cross-block ↑/↓ from above, Ctrl+Home if the
    // list is the first block) puts the cursor in the first item.
    if item_index == 0 {
        create_effect(move |_| {
            if focus_target.get() == Some(list_block_id) {
                editor_sig.with_untracked(|ed| {
                    if let Some(view_id) = ed.editor_view_id.get_untracked() {
                        view_id.request_focus();
                        view_id.scroll_to(None);
                    }
                });
                focus_target.set(None);
            }
        });
    }

    // Per-item height styling: hide the editor gutter (the bullet prefix
    // serves that role) and size to the visual line count.
    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    let view = stack((view,)).style(move |s| {
        let lines = String::from(&text_sig.get()).split('\n').count().max(1) as f32;
        s.class(GutterClass, |s| s.hide())
            .width_full()
            .height(lines * line_height)
    });
    (view.into_any(), editor_sig)
}

/// Build the list-item structural-key callback. Implements the keyboard-
/// isolation behavior table from the spec's section 2: Enter never closes
/// the list; arrows at list boundaries do nothing; empty first-item
/// Backspace removes the item (or the list block, when it's the only item).
#[allow(clippy::too_many_arguments)]
fn make_list_structural_key(
    list_block_id: BlockId,
    item_index: usize,
    item_count: usize,
    item_ids: Rc<Vec<BlockId>>,
    handles: ItemHandles,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> StructuralKey {
    use floem::keyboard::{Key, NamedKey};

    Rc::new(move |kp: &KeyPress, ms: floem::keyboard::Modifiers| {
        let shift = ms.shift();
        let ctrl_or_cmd = ms.control() || ms.meta();

        // Ctrl/Cmd modifier paths that commit-then-navigate. Ctrl+B/I/E/K
        // and Ctrl+Z/Y fall through to the shared handler (they operate on
        // the focused item's own editor / spans signals and do not commit).
        if ctrl_or_cmd {
            match &kp.key {
                KeyInput::Keyboard(Key::Named(NamedKey::Home), _) => {
                    emit_list_commit(&handles, list_block_id, &on_action, current_doc);
                    let first_id =
                        current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
                    if let Some(id) = first_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                KeyInput::Keyboard(Key::Named(NamedKey::End), _) => {
                    emit_list_commit(&handles, list_block_id, &on_action, current_doc);
                    let last_id =
                        current_doc.with_untracked(|d| d.as_ref()?.blocks.last().map(|b| b.id));
                    if let Some(id) = last_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                _ => return None,
            }
        }

        // PageUp / PageDown — 10-block jump. Commit first.
        if matches!(
            &kp.key,
            KeyInput::Keyboard(Key::Named(NamedKey::PageUp | NamedKey::PageDown), _)
        ) {
            let forward = matches!(
                &kp.key,
                KeyInput::Keyboard(Key::Named(NamedKey::PageDown), _)
            );
            let target_id = current_doc.with_untracked(|maybe| {
                let d = maybe.as_ref()?;
                let i = d.blocks.iter().position(|b| b.id == list_block_id)?;
                let j = if forward {
                    (i + 10).min(d.blocks.len().saturating_sub(1))
                } else {
                    i.saturating_sub(10)
                };
                d.blocks.get(j).map(|b| b.id)
            });
            if let Some(id) = target_id {
                emit_list_commit(&handles, list_block_id, &on_action, current_doc);
                defer_focus(focus_target, id);
            }
            return Some(CommandExecuted::Yes);
        }

        match &kp.key {
            // Shift+Enter — soft line break within this item. Fall through.
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => None,

            // Enter — split this item. First commit every item's live
            // buffer as its own undo entry (so typed-but-uncommitted text
            // is a separate undo step), then emit the split as a second
            // entry. Undoing the Enter then restores the typed pre-split
            // state, not the file-load state.
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) => {
                let byte_offset =
                    editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
                let live = collect_items(&handles);
                commit_live_if_changed(&live, list_block_id, &on_action, current_doc);
                let mut split = live;
                let new_item_id = BlockId::new();
                split_item_at_with_id(&mut split, item_index, byte_offset, Some(new_item_id));
                on_action(BlockAction::EditBlockBody {
                    block_id: list_block_id,
                    new_body: Box::new(BlockBody::List(split)),
                    built_in: true, // Built-in list structural-key split.
                });
                defer_focus(focus_target, new_item_id);
                Some(CommandExecuted::Yes)
            }

            // Backspace.
            KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
                let offset =
                    editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
                if offset != 0 {
                    return None; // default handler deletes a char
                }
                let live = collect_items(&handles);
                if item_index > 0 {
                    // Commit pending typing first (separate undo entry),
                    // then merge this item into the previous one.
                    commit_live_if_changed(&live, list_block_id, &on_action, current_doc);
                    let mut merged = live;
                    let cur = merged.remove(item_index);
                    if let Some(prev) = merged.get_mut(item_index - 1) {
                        prev.runs.extend(cur.runs);
                    }
                    let prev_id = item_ids.get(item_index - 1).copied();
                    on_action(BlockAction::EditBlockBody {
                        block_id: list_block_id,
                        new_body: Box::new(BlockBody::List(merged)),
                        built_in: true, // Built-in list structural-key Backspace.
                    });
                    if let Some(id) = prev_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                // First item, offset 0.
                let cur_empty = live
                    .first()
                    .map(|it| it.runs.iter().all(|r| r.text.is_empty()))
                    .unwrap_or(true);
                if !cur_empty {
                    // Non-empty first item: consume, no-op (keyboard
                    // isolation — don't lift content into the previous
                    // block).
                    return Some(CommandExecuted::Yes);
                }
                if live.len() <= 1 {
                    // Only item, and it's empty — remove the list block.
                    on_action(BlockAction::Delete {
                        block_id: list_block_id,
                    });
                } else {
                    // Empty first item with siblings — commit pending
                    // typing in the other items, then drop the empty item.
                    commit_live_if_changed(&live, list_block_id, &on_action, current_doc);
                    let mut without_first = live;
                    without_first.remove(0);
                    let new_first_id = without_first.first().map(|it| it.id);
                    on_action(BlockAction::EditBlockBody {
                        block_id: list_block_id,
                        new_body: Box::new(BlockBody::List(without_first)),
                        built_in: true, // Built-in list structural-key Backspace empty.
                    });
                    if let Some(id) = new_first_id {
                        defer_focus(focus_target, id);
                    }
                }
                Some(CommandExecuted::Yes)
            }

            // ↑.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
                let on_first = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
                });
                if !on_first {
                    return None; // within-item nav — default handler
                }
                if item_index > 0 {
                    emit_list_commit(&handles, list_block_id, &on_action, current_doc);
                    if let Some(id) = item_ids.get(item_index - 1).copied() {
                        defer_focus(focus_target, id);
                    }
                    Some(CommandExecuted::Yes)
                } else {
                    // First item, first vline — keyboard-isolated.
                    Some(CommandExecuted::Yes)
                }
            }

            // ↓.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
                let on_last = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                    vline.0 == ed.last_vline().0
                });
                if !on_last {
                    return None;
                }
                if item_index + 1 < item_count {
                    emit_list_commit(&handles, list_block_id, &on_action, current_doc);
                    if let Some(id) = item_ids.get(item_index + 1).copied() {
                        defer_focus(focus_target, id);
                    }
                    Some(CommandExecuted::Yes)
                } else {
                    // Last item, last vline — keyboard-isolated.
                    Some(CommandExecuted::Yes)
                }
            }

            // Anything else — fall through to the shared default handler
            // (character insertion, etc.).
            _ => None,
        }
    })
}

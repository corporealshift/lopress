//! Per-block native editor state and construction.

use crate::actions::BlockAction;
use crate::model::style_span::{toggle_inline, InlineFlag, StyleSpan};
use crate::model::sync::{inline_runs_to_rope_and_spans, rope_and_spans_to_runs};
use crate::model::types::{BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::style_span::InlineRunStyling;
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::default_key_handler;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::text_document::TextDocument;
use floem::views::editor::view::editor_container_view;
use floem::views::editor::Editor;
use floem::views::{stack, Decorators};
use floem::IntoView;
use lapce_xi_rope::Rope;
use std::rc::Rc;

/// Callback used by editable widgets to push every block-tree mutation
/// through the `actions::apply` chokepoint.
pub type ActionSink = Rc<dyn Fn(BlockAction)>;

/// Pane-level slot that the focused block publishes to so the toolbar
/// can reach the focused block's editor and style-span signals.
#[derive(Clone, Copy)]
pub struct FocusPublisher {
    pub block: RwSignal<Option<BlockId>>,
    pub editor_and_spans: RwSignal<
        Option<(
            RwSignal<Editor>,
            RwSignal<Vec<StyleSpan>>,
            RwSignal<u64>,
            RwSignal<Option<String>>,
        )>,
    >,
}

/// All reactive state owned by one inline block's native editor.
#[derive(Clone, Copy)]
pub struct BlockEditorState {
    pub editor_sig: RwSignal<Editor>,
    pub spans_sig: RwSignal<Vec<StyleSpan>>,
    /// Revision counter; bump to invalidate Floem's text-layout cache after a
    /// style toggle.
    pub style_rev: RwSignal<u64>,
    /// Full block text, kept in sync with the rope via `TextDocument::add_on_update`.
    pub text_sig: RwSignal<String>,
    /// When `Some`, the link-URL input row is shown; holds the editing buffer
    /// seed. `None` hides the row.
    pub link_url_sig: RwSignal<Option<String>>,
}

/// Build a `BlockEditorState` for an inline block, initialised from `runs`.
/// Creates the `TextDocument`, `InlineRunStyling`, and `Editor` in scope `cx`.
pub fn build_block_editor(cx: Scope, runs: &[InlineRun], font_size: usize) -> BlockEditorState {
    let (rope, spans) = inline_runs_to_rope_and_spans(runs);
    // Convert 0.4.0 Rope → String so we can pass it to TextDocument::new (which
    // uses xi-rope 0.3.2 internally — a different crate version from the workspace).
    let initial_text = String::from(&rope);

    let spans_sig = cx.create_rw_signal(spans);
    let style_rev = cx.create_rw_signal(0u64);
    let text_sig = cx.create_rw_signal(initial_text.clone());
    let link_url_sig = cx.create_rw_signal(None::<String>);

    let styling = Rc::new(InlineRunStyling {
        spans: spans_sig,
        text: text_sig,
        rev: style_rev,
        font_size,
    });

    // Pass `initial_text` (String) — xi-rope 0.3.2 implements `From<T: AsRef<str>>` for Rope,
    // so TextDocument accepts any String/&str regardless of which xi-rope version we link.
    let doc = Rc::new(TextDocument::new(cx, initial_text));

    let text_sig_for_update = text_sig;
    doc.add_on_update(move |upd| {
        if let Some(ed) = upd.editor {
            let new_text = String::from(&ed.doc().text());
            text_sig_for_update.set(new_text);
        }
    });

    let editor = Editor::new(cx, doc, styling, false);
    let editor_sig = cx.create_rw_signal(editor);

    BlockEditorState {
        editor_sig,
        spans_sig,
        style_rev,
        text_sig,
        link_url_sig,
    }
}

/// Build the native-editor view for an inline block.
///
/// `focus_target`: when set to `block_id`, this block requests Floem focus.
/// `current_doc`: needed by the key handler to find adjacent blocks for
///   cross-block ↑/↓ navigation.
/// `slash_eligible`: when true, typing `/` on an empty block opens the slash
///   command menu instead of inserting the character (paragraphs only).
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    slash_eligible: bool,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> impl IntoView {
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let style_rev = state.style_rev;
    let text_sig = state.text_sig;
    let link_url_sig = state.link_url_sig;

    let on_action_for_key = on_action;

    // Build the default command handler once; it handles arrow navigation,
    // backspace deletion, etc. via the editor's built-in keymap.
    // We call it explicitly because editor_container_view discards the return
    // value of our closure (see view.rs:1172 — semicolon), so returning
    // CommandExecuted::No does NOT automatically fall through to any default.
    let default_kp_handler = default_key_handler(editor_sig);

    let view = editor_container_view(
        editor_sig,
        // Only show the cursor for the block that actually has keyboard focus.
        // Passing |_| true causes every block to paint a cursor permanently.
        move |_| editor_sig.with_untracked(|ed| ed.active.get()),
        move |kp, ms| {
            let result = handle_key(
                kp,
                ms,
                editor_sig,
                spans_sig,
                style_rev,
                block_id,
                &on_action_for_key,
                focus_target,
                current_doc,
                &on_undo,
                &on_redo,
                slash_eligible,
                link_url_sig,
            );
            // If we consumed the key (block-level action), stop here.
            // Otherwise delegate to the editor's built-in command dispatch
            // so that arrow keys, backspace, etc. work normally.
            if result == CommandExecuted::Yes {
                result
            } else {
                default_kp_handler(kp, ms)
            }
        },
    );

    // Publish focus so the toolbar can reach our editor + spans.
    create_effect(move |_| {
        let is_active = editor_sig.with(|ed| ed.active.get());
        if is_active {
            focus_pub.block.set(Some(block_id));
            focus_pub
                .editor_and_spans
                .set(Some((editor_sig, spans_sig, style_rev, link_url_sig)));
        }
    });

    // When `focus_target` is set to this block, programmatically focus the
    // native editor content view via the ViewId Floem stores on the editor.
    create_effect(move |_| {
        if focus_target.get() == Some(block_id) {
            editor_sig.with_untracked(|ed| {
                if let Some(view_id) = ed.editor_view_id.get_untracked() {
                    view_id.request_focus();
                    view_id.scroll_to(None);
                }
            });
            focus_target.set(None);
        }
    });

    // `editor_container_view` returns a stack with `.absolute().size_pct(100%)`
    // baked in.  `AnyView = Box<dyn View>` delegates its ViewId to the inner
    // view, so `.into_any().style()` modifies the inner absolute stack in-place
    // — leaving it out of normal flow and contributing zero height to the block
    // list.  Wrapping with `stack((view,))` creates a NEW layout node that IS
    // in normal flow; the inner absolute stack then fills it via size_pct(100%).
    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    stack((view,)).style(move |s| {
        let lines = text_sig.get().split('\n').count().max(1) as f32;
        s.class(GutterClass, |s| s.hide())
            .width_full()
            .height(lines * line_height)
    })
}

// ── Key handler ──────────────────────────────────────────────────────────────

fn handle_key(
    kp: &KeyPress,
    ms: floem::keyboard::Modifiers,
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: &Rc<dyn Fn()>,
    on_redo: &Rc<dyn Fn()>,
    slash_eligible: bool,
    link_url_sig: RwSignal<Option<String>>,
) -> CommandExecuted {
    use floem::keyboard::{Key, NamedKey};

    let shift = ms.shift();
    let ctrl_or_cmd = ms.control() || ms.meta();

    // ── Ctrl/Cmd shortcuts ───────────────────────────────────────────────────
    if ctrl_or_cmd {
        if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
            match s.as_str() {
                "z" | "Z" => {
                    if ms.shift() {
                        on_redo();
                    } else {
                        on_undo();
                    }
                    return CommandExecuted::Yes;
                }
                "y" | "Y" => {
                    on_redo();
                    return CommandExecuted::Yes;
                }
                "b" | "B" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Bold);
                    return CommandExecuted::Yes;
                }
                "i" | "I" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Italic);
                    return CommandExecuted::Yes;
                }
                "e" | "E" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Code);
                    return CommandExecuted::Yes;
                }
                "k" | "K" => {
                    apply_style_toggle(editor_sig, spans_sig, style_rev, InlineFlag::Link);
                    // Opening the URL row only makes sense when a link span is
                    // now active in the selection.
                    let has_link = selection_has_link(editor_sig, spans_sig);
                    link_url_sig.set(if has_link { Some(String::new()) } else { None });
                    return CommandExecuted::Yes;
                }
                _ => {}
            }
        }
        // Ctrl+Home / Ctrl+End jump focus to the first / last block.
        if let KeyInput::Keyboard(Key::Named(NamedKey::Home), _) = kp.key {
            commit_from_editor(editor_sig, spans_sig, block_id, on_action);
            let first_id = current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
            if let Some(id) = first_id {
                focus_target.set(Some(id));
            }
            return CommandExecuted::Yes;
        }
        if let KeyInput::Keyboard(Key::Named(NamedKey::End), _) = kp.key {
            commit_from_editor(editor_sig, spans_sig, block_id, on_action);
            let last_id = current_doc.with_untracked(|d| d.as_ref()?.blocks.last().map(|b| b.id));
            if let Some(id) = last_id {
                focus_target.set(Some(id));
            }
            return CommandExecuted::Yes;
        }
        return CommandExecuted::No;
    }

    // Slash command trigger: `/` typed on an empty Paragraph block opens
    // the slash menu instead of inserting the character.
    if !shift {
        if let KeyInput::Keyboard(Key::Character(ref s), _) = kp.key {
            if s.as_str() == "/" && slash_eligible {
                let is_empty = editor_sig.with_untracked(|ed| ed.doc().text().len() == 0);
                if is_empty {
                    on_action(BlockAction::OpenSlashMenu { block_id });
                    return CommandExecuted::Yes;
                }
            }
        }
    }

    match &kp.key {
        // Shift+Enter — insert a soft line break (newline) within the block.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => {
            editor_sig.with_untracked(|ed| {
                ed.doc().receive_char(ed, "\n");
            });
            CommandExecuted::Yes
        }

        // Enter — commit runs and split the block at the cursor byte offset.
        KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) => {
            let byte_offset =
                editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            commit_from_editor(editor_sig, spans_sig, block_id, on_action);
            on_action(BlockAction::Split {
                block_id,
                byte_offset,
            });
            CommandExecuted::Yes
        }

        // Backspace at offset 0 — merge with the previous block.
        KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
            let offset = editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
            if offset == 0 {
                commit_from_editor(editor_sig, spans_sig, block_id, on_action);
                on_action(BlockAction::MergeWithPrev { block_id });
                CommandExecuted::Yes
            } else {
                CommandExecuted::No
            }
        }

        // ↑ on first visual line — jump focus to the previous block.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
            let on_first = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
            });
            if on_first {
                commit_and_jump_prev(
                    editor_sig,
                    spans_sig,
                    block_id,
                    on_action,
                    focus_target,
                    current_doc,
                );
                CommandExecuted::Yes
            } else {
                CommandExecuted::No
            }
        }

        // ↓ on last visual line — jump focus to the next block.
        KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
            let on_last = editor_sig.with_untracked(|ed| {
                let offset = ed.cursor.with_untracked(|c| c.offset());
                let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                let last = ed.last_vline();
                vline.0 == last.0
            });
            if on_last {
                commit_and_jump_next(
                    editor_sig,
                    spans_sig,
                    block_id,
                    on_action,
                    focus_target,
                    current_doc,
                );
                CommandExecuted::Yes
            } else {
                CommandExecuted::No
            }
        }

        // Page Up — jump 10 blocks back (clamped to the first block).
        KeyInput::Keyboard(Key::Named(NamedKey::PageUp), _) => {
            let target_id = current_doc.with_untracked(|maybe| {
                let d = maybe.as_ref()?;
                let i = d.blocks.iter().position(|b| b.id == block_id)?;
                let j = i.saturating_sub(10);
                d.blocks.get(j).map(|b| b.id)
            });
            if let Some(id) = target_id {
                commit_from_editor(editor_sig, spans_sig, block_id, on_action);
                focus_target.set(Some(id));
            }
            CommandExecuted::Yes
        }

        // Page Down — jump 10 blocks forward (clamped to the last block).
        KeyInput::Keyboard(Key::Named(NamedKey::PageDown), _) => {
            let target_id = current_doc.with_untracked(|maybe| {
                let d = maybe.as_ref()?;
                let i = d.blocks.iter().position(|b| b.id == block_id)?;
                let j = (i + 10).min(d.blocks.len().saturating_sub(1));
                d.blocks.get(j).map(|b| b.id)
            });
            if let Some(id) = target_id {
                commit_from_editor(editor_sig, spans_sig, block_id, on_action);
                focus_target.set(Some(id));
            }
            CommandExecuted::Yes
        }

        _ => CommandExecuted::No,
    }
}

/// True if any style span overlapping the current editor selection carries a
/// link. Used to decide whether the URL input row should open after Ctrl+K.
fn selection_has_link(editor_sig: RwSignal<Editor>, spans_sig: RwSignal<Vec<StyleSpan>>) -> bool {
    use floem::views::editor::core::cursor::CursorMode;
    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(o) => (*o, *o),
            CursorMode::Visual { start, end, .. } => (*start.min(end), *start.max(end)),
        })
    });
    spans_sig.with_untracked(|spans| {
        spans.iter().any(|s| {
            let lo = s.start.max(sel_start);
            let hi = s.end.min(sel_end);
            lo < hi && s.link.is_some()
        })
    })
}

/// Read the current selection byte range from the editor and apply a style
/// toggle to the spans. Also bumps `style_rev` to invalidate the layout cache.
pub fn apply_style_toggle(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    style_rev: RwSignal<u64>,
    flag: InlineFlag,
) {
    use floem::views::editor::core::cursor::CursorMode;

    let (sel_start, sel_end) = editor_sig.with_untracked(|ed| {
        ed.cursor.with_untracked(|c| match &c.mode {
            CursorMode::Insert(sel) => (sel.min_offset(), sel.max_offset()),
            CursorMode::Normal(offset) => (*offset, *offset),
            CursorMode::Visual { start, end, .. } => {
                let (lo, hi) = if start <= end {
                    (*start, *end)
                } else {
                    (*end, *start)
                };
                (lo, hi)
            }
        })
    });
    if sel_start >= sel_end {
        return;
    }
    spans_sig.update(|s| toggle_inline(s, sel_start, sel_end, flag));
    style_rev.update(|r| *r = r.wrapping_add(1));
}

fn commit_from_editor(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
) {
    // ed.doc().text() returns xi-rope 0.3.2 Rope; convert via String to the
    // workspace's 0.4.0 Rope that rope_and_spans_to_runs expects.
    let text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
    let spans = spans_sig.get_untracked();
    let rope = Rope::from(text.as_str());
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditInline { block_id, new_runs });
}

fn commit_and_jump_prev(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let prev_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
    });
    if let Some(id) = prev_id {
        focus_target.set(Some(id));
    }
}

fn commit_and_jump_next(
    editor_sig: RwSignal<Editor>,
    spans_sig: RwSignal<Vec<StyleSpan>>,
    block_id: BlockId,
    on_action: &ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
) {
    commit_from_editor(editor_sig, spans_sig, block_id, on_action);
    let next_id = current_doc.with_untracked(|maybe| {
        let d = maybe.as_ref()?;
        let i = d.blocks.iter().position(|b| b.id == block_id)?;
        d.blocks.get(i + 1).map(|b| b.id)
    });
    if let Some(id) = next_id {
        focus_target.set(Some(id));
    }
}

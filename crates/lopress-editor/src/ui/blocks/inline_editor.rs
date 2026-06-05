//! Per-block native editor state and construction.

use crate::actions::BlockAction;
use crate::model::style_span::{toggle_inline, InlineFlag, StyleSpan};
use crate::model::sync::{inline_runs_to_rope_and_spans, rope_and_spans_to_runs};
use crate::model::types::{BlockId, BlockKind, EditorDoc, InlineRun};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::style_span::InlineRunStyling;
use floem::event::{Event, EventListener};
use floem::reactive::{create_effect, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::keypress::default_key_handler;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::text_document::TextDocument;
use floem::views::editor::view::editor_view;
use floem::views::editor::Editor;
use floem::views::{stack, Decorators};
use floem::IntoView;
use floem::View;
use lapce_xi_rope::Rope;

use std::rc::Rc;

/// Caret color for inline block editors — dark enough to contrast on white.
const CARET_COLOR: floem::peniko::Color = floem::peniko::Color::rgb8(40, 40, 40);

/// Callback used by editable widgets to push every block-tree mutation
/// through the `actions::apply` chokepoint.
pub type ActionSink = Rc<dyn Fn(BlockAction)>;

/// The focused block's editor signal, style-span signal, style-revision
/// counter, and link-URL signal — published so the toolbar can reach the
/// active editor.
pub type EditorHandles = (
    RwSignal<Editor>,
    RwSignal<Vec<StyleSpan>>,
    RwSignal<u64>,
    RwSignal<Option<String>>,
);

/// Pane-level slot that the focused block publishes to so the toolbar
/// can reach the focused block's editor and style-span signals.
#[derive(Clone, Copy)]
pub struct FocusPublisher {
    pub block: RwSignal<Option<BlockId>>,
    pub editor_and_spans: RwSignal<Option<EditorHandles>>,
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
    pub text_sig: RwSignal<Rope>,
    /// When `Some`, the link-URL input row is shown; holds the editing buffer
    /// seed. `None` hides the row.
    pub link_url_sig: RwSignal<Option<String>>,
}

/// Pixel height of a block given its wrapped visual-line count. Clamps to at
/// least one line so an empty block still has height.
fn block_height_px(visual_lines: u16, line_height: f32) -> f32 {
    f32::from(visual_lines.max(1)) * line_height
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
    let text_sig = cx.create_rw_signal(rope.clone());
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
            let new_rope = ed.doc().text(); // cheap Arc bump, no full-text copy
            text_sig_for_update.set(new_rope);
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

/// Caller-provided structural-key callback. Invoked first on every keypress;
/// `Some(CommandExecuted::Yes)` short-circuits the shared default handling
/// (Ctrl shortcuts, slash, Enter/Backspace/arrows). `None` falls through.
/// Paragraphs pass a no-op that returns `None` for every key; list items use
/// it to intercept item-level Enter / Backspace-at-0 / arrows.
pub type StructuralKey =
    Rc<dyn Fn(&KeyPress, floem::keyboard::Modifiers) -> Option<CommandExecuted>>;

/// Caller-provided commit closure. Called by the shared handler before any
/// focus-changing or block-jumping shortcut (Ctrl+Home/End, PageUp/Down,
/// cross-block ↑/↓). Currently only consumed by the structural-key callers
/// (lists need it batched); paragraphs flush their own buffer via
/// `commit_from_editor` inside the shared handler, but plumb the closure
/// through for symmetry. List items will use this in stage 4 task 3.
pub type CommitClosure = Rc<dyn Fn()>;

/// Build the native-editor view for an inline block (paragraph / heading).
///
/// Thin wrapper around `mount_block_editor` that supplies the paragraph's
/// `commit` closure and a no-op `structural_key`. Behavior identical to the
/// previous monolithic implementation; the extraction enables list items to
/// share the same mount (stage 4 task 3) by providing their own
/// `structural_key`.
pub fn editable_inline(
    state: BlockEditorState,
    block_id: BlockId,
    env: &BlockEnv,
    slash_eligible: bool,
) -> impl IntoView {
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let on_action_for_commit = env.on_action.clone();
    let commit: CommitClosure = Rc::new(move || {
        // Suppress the commit when the block's kind is no longer inline-bodied.
        // A ChangeType swaps the kind from Paragraph/Heading to Code/List
        // while this editor is still mounted; the FocusLost that follows
        // would emit a stale EditBlockBody{Inline} that overwrites the
        // correct body shape for Code/List blocks.
        let should_commit = env.current_doc.with_untracked(|maybe| {
            maybe.as_ref().and_then(|doc| {
                doc.blocks
                    .iter()
                    .find(|b| b.id == block_id)
                    .map(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::Heading(_)))
            })
        });
        if should_commit.unwrap_or(false) {
            commit_from_editor(editor_sig, spans_sig, block_id, &on_action_for_commit);
        }
    });
    let structural_key: StructuralKey = Rc::new(|_, _| None);
    mount_block_editor(
        state,
        block_id,
        block_id,
        env,
        commit,
        structural_key,
        slash_eligible,
    )
}

/// Shared editor mount. Owns the `editor_view` mount, focus tracking,
/// pointer/keyboard event wiring, and the height-from-visual-lines styling.
/// Calls `structural_key` first on every keypress; falls through to the
/// shared default handler (Ctrl shortcuts, slash, Enter/Backspace/arrows)
/// when `structural_key` returns `None`.
///
/// `block_id` is what `focus_target` reacts to and what the (now mostly
/// unreached) default `handle_key` emits on. `publish_block_id` is what
/// `focus_pub.block` reports when this editor becomes active — for list
/// items it's the *list* block's id (the toolbar's "active block"), not
/// the per-item id. Paragraphs pass the same id for both.
pub fn mount_block_editor(
    state: BlockEditorState,
    block_id: BlockId,
    publish_block_id: BlockId,
    env: &BlockEnv,
    _commit: CommitClosure,
    structural_key: StructuralKey,
    slash_eligible: bool,
) -> impl IntoView {
    let editor_sig = state.editor_sig;
    let spans_sig = state.spans_sig;
    let style_rev = state.style_rev;
    let link_url_sig = state.link_url_sig;

    let on_action_for_key = env.on_action.clone();
    let commit_for_key = _commit;
    let commit_on_focus_lost = Rc::clone(&commit_for_key);

    // Capture env fields into owned/copy types so the closures outlive `env`.
    let focus_target = env.focus_target;
    let focus_pub = env.focus_pub;
    let on_undo = env.on_undo.clone();
    let on_redo = env.on_redo.clone();
    let handle_on_action = env.on_action.clone();
    let handle_current_doc = env.current_doc;
    let handle_focus_target = env.focus_target;
    let handle_on_undo = env.on_undo.clone();
    let handle_on_redo = env.on_redo.clone();

    // Build the default command handler once (arrows, backspace, etc).
    let default_kp_handler = default_key_handler(editor_sig);
    let combined_key = move |kp: &KeyPress, ms: floem::keyboard::Modifiers| {
        // 1. Caller's structural-key callback. Short-circuits the shared
        //    handler when the caller wants block-type-specific semantics
        //    (list items override Enter/Backspace/arrows).
        if let Some(result) = structural_key(kp, ms) {
            return result;
        }
        // 2. Shared default handler — Ctrl shortcuts, slash, block-level
        //    Enter/Backspace/arrow/PageUp/PageDown defaults.
        let result = handle_key(
            kp,
            ms,
            editor_sig,
            spans_sig,
            style_rev,
            block_id,
            &handle_on_action,
            handle_focus_target,
            handle_current_doc,
            &handle_on_undo,
            &handle_on_redo,
            &commit_for_key,
            slash_eligible,
            link_url_sig,
        );
        if result == CommandExecuted::Yes {
            result
        } else {
            // 3. Floem's default editor handler — cursor movement, etc.
            default_kp_handler(kp, ms)
        }
    };

    // Lower-level editor view: no gutter, no per-block scroll. `is_active`
    // gates caret painting; it must mean "this block holds keyboard focus".
    // `ed.active` is NOT that — Floem sets it true only between pointer-down
    // and pointer-up, so gating on it makes the caret vanish the moment the
    // mouse button is released. Track focus explicitly via Focus events.
    let focused = RwSignal::new(false);
    let view = editor_view(editor_sig, move |_| focused.get());
    let view_id = view.id();
    editor_sig.with_untracked(|ed| ed.editor_view_id.set(Some(view_id)));

    let view = view
        .style(|s| {
            s.size_full()
                .cursor(floem::style::CursorStyle::Text)
                .set(floem::style::CursorColor, CARET_COLOR)
        })
        .on_event_cont(EventListener::FocusGained, move |_| {
            focused.set(true);
            editor_sig.with_untracked(|ed| ed.editor_view_focused.notify());
        })
        .on_event_cont(EventListener::FocusLost, move |_| {
            focused.set(false);
            editor_sig.with_untracked(|ed| {
                ed.editor_view_focus_lost.notify();
                // Collapse any active selection to a caret on focus loss
                // so the visual selection background does not linger when
                // the user clicks into a sibling editor (e.g. another
                // item in the same list).
                use floem::views::editor::core::cursor::CursorMode;
                use floem::views::editor::core::selection::Selection;
                ed.cursor.update(|c| {
                    let offset = c.offset();
                    c.mode = CursorMode::Insert(Selection::caret(offset));
                });
            });
            // Flush any typed-but-uncommitted buffer back to the model.
            // Required because Floem's `dyn_container` always rebuilds the
            // editor pane when `current_doc.update()` fires (e.g. an
            // EditAttrs on this block from the lang field), and that
            // rebuild reconstructs each block editor from the model body.
            // Without this commit, focusing away from a block discards
            // any typed text that wasn't already committed via a
            // structural key (Enter/Backspace/arrows).
            commit_on_focus_lost();
        })
        .on_event_cont(EventListener::PointerDown, move |event| {
            if let Event::PointerDown(pe) = event {
                view_id.request_active();
                view_id.request_focus();
                editor_sig.get_untracked().pointer_down(pe);
            }
        })
        .on_event_cont(EventListener::PointerMove, move |event| {
            if let Event::PointerMove(pe) = event {
                editor_sig.get_untracked().pointer_move(pe);
            }
        })
        .on_event_cont(EventListener::PointerUp, move |event| {
            if let Event::PointerUp(pe) = event {
                editor_sig.get_untracked().pointer_up(pe);
            }
        })
        .on_event_stop(EventListener::KeyDown, move |event| {
            let Event::KeyDown(key_event) = event else {
                return;
            };
            let key_text = key_event.key.text.clone();
            let Ok(keypress) = KeyPress::try_from(key_event) else {
                return;
            };
            if combined_key(&keypress, key_event.modifiers) == CommandExecuted::Yes {
                return;
            }

            // Character insertion: Floem's editor_view does not insert text
            // itself — editor_content used to. Replicate that, with SHIFT/ALTGR
            // (and ALT on macOS) cleared so shifted characters still type.
            let mut mods = key_event.modifiers;
            mods.set(floem::keyboard::Modifiers::SHIFT, false);
            mods.set(floem::keyboard::Modifiers::ALTGR, false);
            #[cfg(target_os = "macos")]
            mods.set(floem::keyboard::Modifiers::ALT, false);
            if mods.is_empty() {
                use floem::keyboard::{Key, NamedKey};
                match keypress.key {
                    KeyInput::Keyboard(Key::Character(c), _) => {
                        editor_sig.get_untracked().receive_char(&c);
                    }
                    KeyInput::Keyboard(Key::Named(NamedKey::Space), _) => {
                        editor_sig.get_untracked().receive_char(" ");
                    }
                    KeyInput::Keyboard(Key::Unidentified(_), _) => {
                        if let Some(text) = key_text {
                            editor_sig.get_untracked().receive_char(&text);
                        }
                    }
                    _ => {}
                }
            }
        })
        .on_move(move |point| {
            editor_sig.with_untracked(|ed| ed.window_origin.set(point));
        });

    // Publish focus so the toolbar can reach our editor + spans. For list
    // items, `publish_block_id` is the list block's id (the toolbar slot
    // is owned by the list, not the item); for paragraphs it equals
    // `block_id`.
    create_effect(move |_| {
        let is_active = editor_sig.with(|ed| ed.active.get());
        if is_active {
            focus_pub.block.set(Some(publish_block_id));
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

    // Wrap the `editor_view` in a `stack` so the explicit per-block height
    // below lands on a normal-flow layout node; the inner `editor_view` fills
    // it via `size_full()`.
    let line_height = editor_sig.with_untracked(|ed| ed.line_height(0));
    stack((view,)).style(move |s| {
        // Track screen_lines so the height recomputes when wrapping reflows
        // (text edit or column-width change). `last_vline()` is the index of
        // the last wrapped visual line; +1 converts it to a count.
        let visual_lines = editor_sig.with(|ed| {
            ed.screen_lines.get();
            u16::try_from(ed.last_vline().0 + 1).unwrap_or(u16::MAX)
        });
        s.width_full()
            .height(block_height_px(visual_lines, line_height))
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
    commit: &CommitClosure,
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
                    // Flush any typed-but-uncommitted buffer first so the
                    // undo stack has an entry to pop — typing alone records
                    // nothing until a commit. `commit` is a no-op (records
                    // nothing) when there is no pending change.
                    commit();
                    if ms.shift() {
                        on_redo();
                    } else {
                        on_undo();
                    }
                    return CommandExecuted::Yes;
                }
                "y" | "Y" => {
                    commit();
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
                let is_empty = editor_sig.with_untracked(|ed| ed.doc().text().is_empty());
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
                new_block_id: None,
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
    let rope = editor_sig.with_untracked(|ed| ed.doc().text());
    let spans = spans_sig.get_untracked();
    let new_runs = rope_and_spans_to_runs(&rope, &spans);
    on_action(BlockAction::EditBlockBody {
        block_id,
        new_body: Box::new(crate::model::types::BlockBody::Inline(new_runs)),
        built_in: true, // Built-in inline editor widget.
    });
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

#[cfg(test)]
mod tests {
    use super::block_height_px;

    #[test]
    fn block_height_scales_with_visual_lines() {
        assert!((block_height_px(1, 20.0) - 20.0).abs() < f32::EPSILON);
        assert!((block_height_px(3, 20.0) - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn block_height_clamps_empty_to_one_line() {
        assert!((block_height_px(0, 20.0) - 20.0).abs() < f32::EPSILON);
    }
}

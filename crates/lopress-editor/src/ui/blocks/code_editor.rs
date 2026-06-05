//! Editable code block — the canonical `editor = "code"` implementation.
//!
//! Builds a single `BlockEditorState` via `build_block_editor` (fed a
//! synthetic single `InlineRun` carrying the code body, with no style spans),
//! then mounts via `mount_block_editor` with a code-specific commit closure
//! and a code-specific structural-key callback. The view wraps the mounted
//! editor in a frame with a corner lang label, monospace font, and height
//! sized to the visual-line count.

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, EditorDoc, InlineRun};
use crate::ui::blocks::env::BlockEnv;
use crate::ui::blocks::inline_editor::{
    build_block_editor, mount_block_editor, ActionSink, CommitClosure, StructuralKey,
};
use crate::ui::blocks::paragraph::MONO_FAMILY;
use crate::ui::editing::focus::defer_focus;
use floem::event::EventListener;
use floem::peniko::Color;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use floem::views::editor::command::CommandExecuted;
use floem::views::editor::core::cursor::CursorAffinity;
use floem::views::editor::gutter::GutterClass;
use floem::views::editor::keypress::key::KeyInput;
use floem::views::editor::keypress::press::KeyPress;
use floem::views::editor::Editor;
use floem::views::{empty, h_stack, stack, text_input, Decorators};
use floem::{AnyView, IntoView};
use std::rc::Rc;

/// Code-specific font size (logical px) for the code body.
const CODE_FONT_SIZE: usize = 13;

/// Commit closure for the code widget. Reads the editor buffer, compares
/// against the model's current body for `block_id`, and emits
/// `EditBlockBody { Code }` when they differ.
fn make_code_commit(
    block_id: BlockId,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    current_doc: RwSignal<Option<EditorDoc>>,
) -> CommitClosure {
    let commit_on_action = on_action.clone();
    Rc::new(move || {
        let live_text = editor_sig.with_untracked(|ed| String::from(&ed.doc().text()));
        let differs = current_doc.with_untracked(|maybe| {
            maybe
                .as_ref()
                .and_then(|d| d.blocks.iter().find(|b| b.id == block_id))
                .map(|b| !matches!(&b.body, BlockBody::Code(s) if s == &live_text))
                .unwrap_or(false)
        });
        if differs {
            commit_on_action(BlockAction::EditBlockBody {
                block_id,
                new_body: Box::new(BlockBody::Code(live_text)),
                built_in: true, // Built-in code editor widget.
            });
        }
    })
}

/// Code-specific structural-key callback. Implements the code-native keymap
/// table from the spec: Enter/Tab insert into the body, navigation keys
/// jump blocks at vline boundaries, Backspace at offset 0 of an empty body
/// deletes the block, Backspace at offset 0 of a non-empty body is
/// keyboard-isolated.
fn make_code_structural_key(
    block_id: BlockId,
    editor_sig: RwSignal<Editor>,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    current_doc: RwSignal<Option<EditorDoc>>,
    commit: CommitClosure,
) -> StructuralKey {
    use floem::keyboard::{Key, NamedKey};

    Rc::new(move |kp: &KeyPress, ms: floem::keyboard::Modifiers| {
        let shift = ms.shift();
        let ctrl_or_cmd = ms.control() || ms.meta();

        // Commit before any navigation action.
        let do_commit = || commit();

        // Ctrl/Cmd modifier paths that commit-then-navigate.
        if ctrl_or_cmd {
            match &kp.key {
                KeyInput::Keyboard(Key::Named(NamedKey::Home), _) => {
                    do_commit();
                    let first_id =
                        current_doc.with_untracked(|d| d.as_ref()?.blocks.first().map(|b| b.id));
                    if let Some(id) = first_id {
                        defer_focus(focus_target, id);
                    }
                    return Some(CommandExecuted::Yes);
                }
                KeyInput::Keyboard(Key::Named(NamedKey::End), _) => {
                    do_commit();
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
            do_commit();
            let target_id = current_doc.with_untracked(|maybe| {
                let d = maybe.as_ref()?;
                let i = d.blocks.iter().position(|b| b.id == block_id)?;
                let j = if forward {
                    (i + 10).min(d.blocks.len().saturating_sub(1))
                } else {
                    i.saturating_sub(10)
                };
                d.blocks.get(j).map(|b| b.id)
            });
            if let Some(id) = target_id {
                defer_focus(focus_target, id);
            }
            return Some(CommandExecuted::Yes);
        }

        match &kp.key {
            // Enter (no mods) — insert newline, no block split.
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if !shift => {
                editor_sig.get_untracked().receive_char("\n");
                Some(CommandExecuted::Yes)
            }

            // Shift+Enter — same as Enter (soft line break).
            KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) if shift => {
                editor_sig.get_untracked().receive_char("\n");
                Some(CommandExecuted::Yes)
            }

            // Shift+Tab — consume, no-op (defer outdent to a follow-up).
            // Must come BEFORE the unguarded Tab arm so the shift guard
            // is evaluated first.
            KeyInput::Keyboard(Key::Named(NamedKey::Tab), _) if shift => Some(CommandExecuted::Yes),

            // Tab — insert two spaces.
            KeyInput::Keyboard(Key::Named(NamedKey::Tab), _) => {
                editor_sig.get_untracked().receive_char("  ");
                Some(CommandExecuted::Yes)
            }

            // Backspace.
            KeyInput::Keyboard(Key::Named(NamedKey::Backspace), _) => {
                let offset =
                    editor_sig.with_untracked(|ed| ed.cursor.with_untracked(|c| c.offset()));
                if offset > 0 {
                    return None; // default handler deletes one char
                }
                // Offset is 0.
                let body_is_empty = editor_sig.with_untracked(|ed| ed.doc().text().is_empty());
                if body_is_empty {
                    // Empty body at offset 0 — delete the block.
                    do_commit();
                    on_action(BlockAction::Delete { block_id });
                    return Some(CommandExecuted::Yes);
                }
                // Non-empty body at offset 0 — keyboard isolation.
                Some(CommandExecuted::Yes)
            }

            // ArrowUp at first vline — jump to previous block.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowUp), _) => {
                let on_first = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    ed.vline_of_offset(offset, CursorAffinity::Backward).0 == 0
                });
                if !on_first {
                    return None; // within-block navigation
                }
                do_commit();
                let prev_id = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    i.checked_sub(1).and_then(|j| d.blocks.get(j)).map(|b| b.id)
                });
                if let Some(id) = prev_id {
                    defer_focus(focus_target, id);
                }
                Some(CommandExecuted::Yes)
            }

            // ArrowDown at last vline — jump to next block.
            KeyInput::Keyboard(Key::Named(NamedKey::ArrowDown), _) => {
                let on_last = editor_sig.with_untracked(|ed| {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    let vline = ed.vline_of_offset(offset, CursorAffinity::Forward);
                    vline.0 == ed.last_vline().0
                });
                if !on_last {
                    return None;
                }
                do_commit();
                let next_id = current_doc.with_untracked(|maybe| {
                    let d = maybe.as_ref()?;
                    let i = d.blocks.iter().position(|b| b.id == block_id)?;
                    d.blocks.get(i + 1).map(|b| b.id)
                });
                if let Some(id) = next_id {
                    defer_focus(focus_target, id);
                }
                Some(CommandExecuted::Yes)
            }

            // Anything else — fall through to the shared default handler.
            _ => None,
        }
    })
}

/// Build the editable code block view.
///
/// Creates a single `BlockEditorState` from the code body (as a synthetic
/// `InlineRun`), mounts it via `mount_block_editor` with a code-specific
/// commit closure and structural-key callback, and wraps everything in a
/// styled frame with a corner lang label.
pub fn editable_code_view(body: &str, lang: &str, block_id: BlockId, env: &BlockEnv) -> AnyView {
    let cx = Scope::current();

    // Build editor state from a single synthetic InlineRun carrying the body.
    // Code has no inline styles, so InlineRun::plain (default bold/italic/
    // code = false, link = None) is exactly right.
    let runs = vec![InlineRun::plain(body)];
    let state = build_block_editor(cx, &runs, CODE_FONT_SIZE);
    let editor_sig = state.editor_sig;
    let text_sig = state.text_sig;

    // Code-specific commit closure: read buffer, compare with model, emit
    // EditBlockBody { Code } on diff.
    let commit_on_action = env.on_action.clone();
    let commit = make_code_commit(block_id, editor_sig, commit_on_action, env.current_doc);

    // Code-specific structural-key callback.
    let structural_key = make_code_structural_key(
        block_id,
        editor_sig,
        env.on_action.clone(),
        env.focus_target,
        env.current_doc,
        commit.clone(),
    );

    // Clone for the lang widget before mount_block_editor consumes on_action.
    let lang_on_action = env.on_action.clone();

    // Mount the editor. slash_eligible: false — "/" does not open the slash
    // menu inside a code body.
    let editor_view = mount_block_editor(
        state,
        block_id,
        block_id,
        env,
        commit,
        structural_key,
        /* slash_eligible */ false,
    );

    // Lang input in the top-right corner. An always-on `text_input` bound
    // to `lang_sig`; styled to look like a plain corner label until focused,
    // when it grows a subtle border. On FocusLost or Enter we commit the new
    // lang via `EditAttrs` (deferred so the click that moves focus has fully
    // settled before any signal mutation lands). Escape reverts.
    let lang_sig: RwSignal<String> = RwSignal::new(lang.to_string());
    let lang_committed: RwSignal<String> = RwSignal::new(lang.to_string());

    let lang_on_action_for_blur = lang_on_action.clone();
    let lang_on_action_for_key = lang_on_action.clone();

    let lang_input = text_input(lang_sig)
        .on_event_stop(EventListener::FocusLost, move |_| {
            let on_action = lang_on_action_for_blur.clone();
            floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
                let new_lang = lang_sig.get_untracked();
                if new_lang != lang_committed.get_untracked() {
                    let mut new_attrs = serde_json::Map::new();
                    new_attrs.insert(
                        "lang".to_string(),
                        serde_json::Value::String(new_lang.clone()),
                    );
                    on_action(BlockAction::EditAttrs {
                        block_id,
                        new_attrs: Box::new(new_attrs),
                    });
                    lang_committed.set(new_lang);
                }
            });
        })
        .on_event_stop(EventListener::KeyDown, move |e: &floem::event::Event| {
            if let floem::event::Event::KeyDown(k) = e {
                if matches!(
                    k.key.logical_key,
                    floem::keyboard::Key::Named(floem::keyboard::NamedKey::Enter)
                ) {
                    let on_action = lang_on_action_for_key.clone();
                    floem::action::exec_after(std::time::Duration::from_millis(0), move |_| {
                        let new_lang = lang_sig.get_untracked();
                        if new_lang != lang_committed.get_untracked() {
                            let mut new_attrs = serde_json::Map::new();
                            new_attrs.insert(
                                "lang".to_string(),
                                serde_json::Value::String(new_lang.clone()),
                            );
                            on_action(BlockAction::EditAttrs {
                                block_id,
                                new_attrs: Box::new(new_attrs),
                            });
                            lang_committed.set(new_lang);
                        }
                    });
                } else if matches!(
                    k.key.logical_key,
                    floem::keyboard::Key::Named(floem::keyboard::NamedKey::Escape)
                ) {
                    let original = lang_committed.get_untracked();
                    lang_sig.set(original);
                }
            }
        })
        .style(|s| {
            s.color(Color::rgb8(120, 120, 120))
                .font_size(11.)
                .padding_horiz(6.)
                .padding_vert(0.)
                .min_width(60.)
                .border(0.0)
                .background(Color::TRANSPARENT)
                .cursor(floem::style::CursorStyle::Text)
                .hover(|s| s.background(Color::rgb8(235, 235, 235)))
                .focus(|s| {
                    s.border(1.0)
                        .border_color(Color::rgb8(180, 180, 180))
                        .background(Color::WHITE)
                })
        });

    let header = h_stack((empty().style(|s| s.flex_grow(1.0)), lang_input));

    // Body: wrap the mounted editor in a stack that hides the gutter and
    // applies monospace font + padding. Height tracks the visual line count.
    let line_height = f64::from(editor_sig.with_untracked(|ed| ed.line_height(0)));
    let body_view = stack((editor_view,)).style(move |s| {
        // Line count is bounded by viewport height / line_height — never exceeds
        // ~10 k lines on screen, well within f64's 53-bit mantissa.
        #[allow(clippy::cast_precision_loss)]
        let lines = String::from(&text_sig.get()).split('\n').count().max(1) as f64;
        s.class(GutterClass, |s| s.hide())
            .font_family(MONO_FAMILY.to_string())
            .font_size(13.)
            .padding(10.)
            .width_full()
            .height(lines * line_height + 20.)
    });

    // Outer frame: same styling as the read-only `code::render_code`.
    stack((header, body_view))
        .style(|s| {
            s.flex_col()
                .width_full()
                .background(Color::rgb8(245, 245, 245))
                .border_radius(4.)
                .border(1.)
                .border_color(Color::rgb8(220, 220, 220))
                .margin_vert(8.)
        })
        .into_any()
}

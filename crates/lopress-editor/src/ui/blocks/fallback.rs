//! Recoverable fallback view for blocks that can't be rendered by their normal editor.
//!
//! Renders visible content (flat text or pretty-printed JSON for Opaque bodies),
//! a persistent inline warning banner, and a PointerDown handler that sets focus
//! so the toolbar mounts — giving the user Change Type / Delete to recover.

use crate::actions::body_to_flat_text;
use crate::model::types::{BlockBody, EditorBlock};
use crate::ui::blocks::inline_editor::FocusPublisher;
use crate::ui::blocks::paragraph::MONO_FAMILY;
use floem::event::{EventListener, EventPropagation};
use floem::peniko::Color;
use floem::reactive::SignalUpdate;
use floem::views::{label, text, v_stack, Decorators};
use floem::{AnyView, IntoView};

/// The recovery hint shown inline on a fallback block. Self-clears because the
/// fallback view is no longer constructed once the block renders normally.
///
/// An `Opaque` body is an unknown or unregistered block type loaded from disk:
/// there is no sensible target kind to convert it into (`apply_change_type`
/// can't reshape arbitrary JSON, and the flattened text is empty), so Change
/// Type does not recover it — the copy points only to Delete. Any other body
/// shape is a genuine kind/body mismatch that Change Type *can* recover by
/// re-mounting a working editor.
fn warning_text_for(body: &BlockBody) -> &'static str {
    match body {
        BlockBody::Opaque(_) => {
            "This block's type isn't available here, so it can't be edited — showing its raw content. Delete it to remove this block."
        }
        _ => {
            "This block couldn't be displayed with its editor — showing its raw content. Change its type or delete it to recover."
        }
    }
}

/// Build a recoverable fallback view for a block that can't be rendered normally.
///
/// Renders the block's flat text (or pretty-printed JSON for Opaque bodies),
/// an inline warning banner, and a PointerDown handler that sets `focus_pub.block`
/// (mounting the toolbar) and clears `focus_pub.editor_and_spans` (preventing
/// stale editor handles from being read by the toolbar's pre-commit).
///
/// The fallback is read-only — no in-place editing, because the body shape is
/// ambiguous and committing it would risk a fresh mismatch. Recovery is via the
/// toolbar only (Change Type re-mounts a working editor; Delete removes the block).
pub fn fallback_block_view(block: &EditorBlock, focus_pub: FocusPublisher) -> AnyView {
    let block_id = block.id;

    // Visible content: flat text for known body shapes, pretty-printed JSON for Opaque.
    let content = match &block.body {
        BlockBody::Opaque(value) => {
            // Opaque bodies have no flat text; show the pretty-printed JSON
            // the same way the opaque renderer does (opaque.rs), so the user
            // can still see the raw content even when it can't be classified.
            let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            text(json)
                .style(|s| {
                    s.font_family(MONO_FAMILY.to_string())
                        .font_size(12.)
                        .padding(8.)
                        .background(Color::rgb8(245, 245, 245))
                        .width_full()
                })
                .into_any()
        }
        _ => {
            let flat = body_to_flat_text(&block.body);
            text(flat)
                .style(|s| s.font_size(14.).padding(8.).width_full())
                .into_any()
        }
    };

    // Warning banner: persistent, inline, non-blocking. Copy depends on whether
    // the block is recoverable via Change Type (mismatch) or only Delete (Opaque).
    let warning_text = warning_text_for(&block.body);
    let warning = label(move || warning_text.to_string()).style(|s| {
        s.font_size(11.)
            .color(Color::rgb8(180, 120, 40))
            .padding_horiz(8.)
            .padding_vert(4.)
            .background(Color::rgb8(255, 248, 230))
            .border_radius(4.)
            .margin(6.)
            .width_full()
    });

    // The body: a full-width warning banner above the raw content, with a
    // PointerDown that sets focus. Vertical so the banner is never clipped
    // beside the content (a horizontal stack hid the message off the edge).
    let body = v_stack((warning, content))
        .style(|s| {
            s.width_full()
                .border(1.)
                .border_color(Color::rgb8(220, 200, 160))
                .border_radius(4.)
                .background(Color::rgb8(255, 252, 240))
        })
        .on_event(EventListener::PointerDown, move |_| {
            // Mount the toolbar: set the focused block id.
            focus_pub.block.set(Some(block_id));
            // Clear stale editor handles so the toolbar's pre-commit doesn't
            // read a previous block's inline editor and fire it against this one.
            focus_pub.editor_and_spans.set(None);
            EventPropagation::Continue
        });

    body.into_any()
}

#[cfg(test)]
mod tests {
    use super::warning_text_for;
    use crate::model::types::{BlockBody, InlineRun};

    #[test]
    fn opaque_warning_points_to_delete_only() {
        // An Opaque block can't be recovered via Change Type, so its copy must
        // not promise it — only Delete.
        let w = warning_text_for(&BlockBody::Opaque(serde_json::json!({ "src": "x.mp4" })));
        assert!(w.contains("Delete"), "expected a Delete hint, got: {w}");
        assert!(
            !w.contains("Change its type"),
            "Opaque copy must not promise Change Type recovery, got: {w}"
        );
    }

    #[test]
    fn mismatch_warning_offers_change_type() {
        // A genuine kind/body mismatch (non-Opaque body) is recoverable via
        // Change Type, so the copy should offer it.
        let w = warning_text_for(&BlockBody::Inline(vec![InlineRun::plain("hi")]));
        assert!(
            w.contains("Change its type"),
            "expected a Change Type hint, got: {w}"
        );
    }
}

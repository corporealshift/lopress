//! Pane-rebuild key: a lightweight discriminant for the editor key and
//! per-block metadata used to key the `dyn_container` in `editing_view`.
//!
//! Within-block text edits (which fire `EditInline` → `current_doc.update`)
//! must NOT tear down the per-block widgets, otherwise focus is lost every
//! time the user commits runs. The per-block widgets own their own
//! `runs_sig` reactive copies; structural changes (split, delete, insert,
//! reorder) change the id list and trigger a rebuild. Block-editor changes
//! (toolbar P/H1/H2/Code/UL/OL buttons) do too — discriminant comparison
//! covers `"heading"` vs `"paragraph"`, heading level changes, etc.

use crate::model::descriptor;
use crate::model::types::{BlockId, EditorDoc};
use floem::reactive::{RwSignal, SignalWith};
use std::rc::Rc;

/// Build the closure that keys the editor-pane `dyn_container`.
///
/// Returns a closure that, when called, produces the current block id
/// sequence + per-block editor key + plugin presence. This closure is passed
/// as the key function to `dyn_container`.
///
/// For heading blocks the heading level is included in the key so that
/// changing from H1 → H2 triggers a pane rebuild (the heading widget
/// reads the level from `attrs`, not from `BlockKind`).
pub fn build_pane_key(
    current_doc: RwSignal<Option<EditorDoc>>,
) -> impl Fn() -> Option<Vec<(BlockId, Rc<str>, bool)>> + Copy {
    move || {
        current_doc.with(|d| {
            d.as_ref().map(|d| {
                d.blocks
                    .iter()
                    .map(|b| {
                        let editor = b
                            .plugin
                            .editor
                            .clone()
                            .unwrap_or_else(|| Rc::from(descriptor::EDITOR_PARAGRAPH));
                        // For headings, include the level in the key so
                        // H1→H2 changes trigger a rebuild.
                        let key = if &*editor == descriptor::EDITOR_HEADING {
                            let level = b
                                .plugin
                                .attrs
                                .get("level")
                                .and_then(|v| v.as_u64())
                                .map(|n| format!("{}:{}", descriptor::EDITOR_HEADING, n))
                                .unwrap_or_else(|| editor.to_string());
                            Rc::from(level.as_str())
                        } else {
                            editor
                        };
                        (b.id, key, true)
                    })
                    .collect::<Vec<_>>()
            })
        })
    }
}

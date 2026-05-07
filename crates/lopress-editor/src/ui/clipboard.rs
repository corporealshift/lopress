//! Clipboard helpers for selection copy / cut / paste.
//!
//! Floem 0.2 only exposes a single text payload, so we round-trip through
//! markdown. That works for external pastes (markdown is the universal
//! cross-app format here) but means a copy-paste roundtrip inside lopress
//! goes through serialize → text → parse, which can normalize formatting
//! (e.g. heading levels round-trip exactly, but trailing whitespace inside
//! styled runs may shift). A future task can add a magic-header escape hatch
//! to preserve the raw `Vec<EditorBlock>` JSON alongside the markdown.

use crate::model::from_core::doc_from_core;
use crate::model::to_core::doc_to_core;
use crate::model::types::{BlockBody, EditorBlock, EditorDoc, InlineRun};
use crate::selection::DocSelection;
use crate::ui::blocks::inline_editor::Caret;
use floem::Clipboard;
use lopress_core::FrontMatter;

/// Extract the slice of `doc` covered by `selection` as a `Vec<EditorBlock>`.
/// Single-block selections produce one block whose runs are the slice. Multi-
/// block selections produce a leading partial, all middle blocks intact, and
/// a trailing partial.
pub fn extract_selection_blocks(doc: &EditorDoc, selection: DocSelection) -> Vec<EditorBlock> {
    let (start, end) = selection.ordered(doc);
    let Some(start_idx) = doc.blocks.iter().position(|b| b.id == start.block) else {
        return Vec::new();
    };
    let Some(end_idx) = doc.blocks.iter().position(|b| b.id == end.block) else {
        return Vec::new();
    };

    if start_idx == end_idx {
        return vec![slice_inline_block(
            &doc.blocks[start_idx],
            Some(Caret {
                run: start.run,
                offset: start.offset,
            }),
            Some(Caret {
                run: end.run,
                offset: end.offset,
            }),
        )];
    }

    let mut out = Vec::with_capacity(end_idx - start_idx + 1);
    out.push(slice_inline_block(
        &doc.blocks[start_idx],
        Some(Caret {
            run: start.run,
            offset: start.offset,
        }),
        None,
    ));
    for b in &doc.blocks[start_idx + 1..end_idx] {
        out.push(b.clone());
    }
    out.push(slice_inline_block(
        &doc.blocks[end_idx],
        None,
        Some(Caret {
            run: end.run,
            offset: end.offset,
        }),
    ));
    out
}

/// Clone `block` keeping only the chars in `[start_caret, end_caret)`.
/// `None` for either bound means "from block start" / "to block end".
fn slice_inline_block(
    block: &EditorBlock,
    start_caret: Option<Caret>,
    end_caret: Option<Caret>,
) -> EditorBlock {
    let mut clone = block.clone();
    let runs = match &block.body {
        BlockBody::Inline(r) => r.clone(),
        _ => {
            // Non-inline: copy whole.
            return clone;
        }
    };

    let start = start_caret.unwrap_or(Caret::START);
    let end = end_caret.unwrap_or_else(|| Caret::end(&runs));

    let mut out: Vec<InlineRun> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        if i < start.run || i > end.run {
            continue;
        }
        let chars: Vec<char> = r.text.chars().collect();
        let lo = if i == start.run {
            start.offset.min(chars.len())
        } else {
            0
        };
        let hi = if i == end.run {
            end.offset.min(chars.len())
        } else {
            chars.len()
        };
        if lo >= hi {
            continue;
        }
        let mut clipped = r.clone();
        clipped.text = chars[lo..hi].iter().collect();
        out.push(clipped);
    }
    if out.is_empty() {
        out.push(InlineRun::plain(""));
    }
    clone.body = BlockBody::Inline(out);
    clone
}

/// Serialize `blocks` as a markdown string suitable for clipboard payload.
pub fn blocks_to_markdown(blocks: &[EditorBlock]) -> String {
    let editor_doc = EditorDoc {
        blocks: blocks.to_vec(),
        front_matter: FrontMatter::default(),
    };
    let core_doc = doc_to_core(&editor_doc);
    lopress_core::serialize(&core_doc)
}

/// Parse a markdown string into a `Vec<EditorBlock>`. Returns an empty
/// vector if parsing fails — paste then becomes a no-op.
///
/// `registry` is consulted for plugin-declared block types; an external
/// paste with no plugin context can pass `&PluginRegistry::default()`.
pub fn markdown_to_blocks(s: &str, registry: &lopress_plugin::PluginRegistry) -> Vec<EditorBlock> {
    match lopress_core::parse(s) {
        Ok(core_doc) => doc_from_core(&core_doc, registry).blocks,
        Err(_) => Vec::new(),
    }
}

/// Write `text` to the system clipboard. Errors are silenced — clipboard is
/// best-effort.
pub fn write_clipboard(text: String) {
    let _ = Clipboard::set_contents(text);
}

/// Read the system clipboard. Returns `None` on failure or empty content.
pub fn read_clipboard() -> Option<String> {
    Clipboard::get_contents().ok().filter(|s| !s.is_empty())
}

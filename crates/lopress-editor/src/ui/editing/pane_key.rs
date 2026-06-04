//! Pane-rebuild key: a lightweight discriminant for `BlockKind` and per-block
//! metadata used to key the `dyn_container` in `editing_view`.
//!
//! Within-block text edits (which fire `EditInline` → `current_doc.update`)
//! must NOT tear down the per-block widgets, otherwise focus is lost every
//! time the user commits runs. The per-block widgets own their own
//! `runs_sig` reactive copies; structural changes (split, delete, insert,
//! reorder) change the id list and trigger a rebuild. Block-kind changes
//! (toolbar P/H1/H2/Code/UL/OL buttons) do too — discriminant comparison
//! covers `Heading(1)` vs `Heading(2)`, `List{ordered:false}` vs
//! `ordered:true`, etc.

use crate::model::types::{BlockId, BlockKind, EditorDoc};
use floem::reactive::{RwSignal, SignalWith};

/// Compact equality tag for `BlockKind` used by the editor-pane rebuild key.
/// `Eq` is fine; this is just a discriminator (Heading(1) vs Heading(2),
/// List{ordered:false} vs ordered:true, etc.) so we trigger a pane rebuild
/// when the toolbar's type buttons swap a block's kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum KindTag {
    Paragraph,
    Heading(u8),
    Code,
    List { ordered: bool },
    Image,
    Table,
    Opaque,
}

pub fn kind_tag(k: &BlockKind) -> KindTag {
    match k {
        BlockKind::Paragraph => KindTag::Paragraph,
        BlockKind::Heading(level) => KindTag::Heading(*level),
        BlockKind::Code { .. } => KindTag::Code,
        BlockKind::List { ordered } => KindTag::List { ordered: *ordered },
        BlockKind::Image => KindTag::Image,
        BlockKind::Table => KindTag::Table,
        BlockKind::Opaque { .. } => KindTag::Opaque,
    }
}

/// Build the closure that keys the editor-pane `dyn_container`.
///
/// Returns a closure that, when called, produces the current block id
/// sequence + per-block kind tag + plugin presence. This closure is passed
/// as the key function to `dyn_container`.
pub fn build_pane_key(
    current_doc: RwSignal<Option<EditorDoc>>,
) -> impl Fn() -> Option<Vec<(BlockId, KindTag, bool)>> + Copy {
    move || {
        current_doc.with(|d| {
            d.as_ref().map(|d| {
                d.blocks
                    .iter()
                    .map(|b| (b.id, kind_tag(&b.kind), b.plugin.is_some()))
                    .collect::<Vec<_>>()
            })
        })
    }
}

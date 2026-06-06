//! Block descriptor table — one authoritative declaration per built-in block type.
//!
//! This is the **single source of truth** for each block type's data facts:
//! core type (native claim), body shape, editor key, whether it's a built-in,
//! and slash/toolbar presentation metadata. Every other site that previously
//! re-encoded these facts (hardcoded match arms, magic strings, separate
//! `editor_for` registry) now reads from this table.
//!
//! ## Architecture: model ← ui
//!
//! This module is in `model/` and references **no ui types** (`EditorWidget`,
//! `BlockEnv`, `AnyView`, nothing under `crate::ui`). The widget fn-pointers
//! live in `crate::ui::blocks::editor_registry::editor_for`, keyed by the
//! same `editor` string the descriptor carries. The link between the two
//! layers is that string — nothing else.
//!
//! ## Heading-level menu wrinkle
//!
//! There is ONE `heading` descriptor, but the slash menu shows H1–H3 and
//! the toolbar shows H1–H6. The descriptor carries a single `MenuEntry`
//! with `title: "Heading"`. The menu-generation code (in `slash_menu.rs`
//! and `toolbar.rs`) detects the heading descriptor by its `editor` key
//! and expands it into per-level entries. Each expanded entry uses a
//! `default_block` closure that produces `EditorBlock::heading(n, vec![])`
//! with the correct level.

use crate::model::types::EditorBlock;

/// The body shape a block's editor produces and round-trips.
///
/// This enum is the stable contract between the descriptor table and the
/// parser/serializer helpers in `from_core.rs` and `to_core.rs`. It outlives
/// `BlockKind` (Stage B deletes `BlockKind` and leans on `BodyShape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyShape {
    /// `Vec<InlineRun>` — paragraph, heading.
    Inline,
    /// `String` — code block.
    Code,
    /// `Vec<ListItem>` — ordered or unordered list.
    List,
    /// `TableData` — table with rows, cells, and column alignments.
    Table,
    /// `serde_json::Value` — image placeholder, unknown/removed types.
    Opaque,
}

/// One entry in the slash menu or toolbar for a block type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(unpredictable_function_pointer_comparisons)] // default_block is a fn pointer
pub struct MenuEntry {
    /// Display label in the slash menu. `None` → not in slash menu.
    pub slash_label: Option<&'static str>,
    /// Display label in the toolbar. `None` → not in toolbar.
    pub toolbar_label: Option<&'static str>,
    /// Category bucket for grouping.
    pub category: &'static str,
    /// Construct the default block for this entry. Used by the slash menu to
    /// insert a fresh block and by the toolbar to derive the ChangeType action.
    pub default_block: fn() -> EditorBlock,
}

/// Everything the editor needs to know about one built-in block type, in one place.
///
/// A descriptor is the **single source of truth** for:
/// - Which editor key this type uses (`editor`)
/// - Which native core type it claims when serialized (`native`)
/// - What body shape its editor produces (`body_shape`)
/// - Whether it's a built-in (suppresses plugin chrome) (`builtin`)
/// - How it appears in the slash menu / toolbar (`menu`)
/// - How to construct a fresh default block (`default_block`)
///
/// The widget fn-pointer lives in `crate::ui::blocks::editor_registry`, keyed
/// by the same `editor` string. This module does NOT reference any ui types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(unpredictable_function_pointer_comparisons)] // default_block is a fn pointer; equality is only used in tests
pub struct BlockDescriptor {
    /// The `editor` key — the primary identity. Matches PluginMeta.editor and
    /// the manifest `editor` field. E.g. "paragraph", "heading", "code", "list",
    /// "image", "table", "separator", "more".
    pub editor: &'static str,
    /// The core markdown type this block claims when serialized natively, if any.
    /// `Some("paragraph")`, `Some("list")`, … ; `None` → comment container.
    pub native: Option<&'static str>,
    /// Body shape produced by this block's editor widget.
    pub body_shape: BodyShape,
    /// Whether this block is a built-in (base plugin) — suppresses plugin chrome.
    pub builtin: bool,
    /// Slash-menu / toolbar presentation. A list of entries — each entry
    /// may or may not appear in each menu (controlled by `slash_label` /
    /// `toolbar_label`). `&[]` → not in any menu.
    pub menu: &'static [MenuEntry],
    /// Construct the canonical empty/default block for this type (used by the
    /// slash menu, toolbar ChangeType, and split's tail-block creation).
    ///
    /// For the heading descriptor itself, this produces a level-1 heading.
    /// Menu-generation code that expands the heading descriptor into H1–H6
    /// replaces this closure with one that produces the correct level.
    pub default_block: fn() -> EditorBlock,
}

/// Named constants for built-in editor keys — replaces magic strings in
/// `actions.rs`, `to_core.rs`, and other sites that previously checked
/// `block_type_name == "lopress:more"` or `editor == "list"` etc.
pub const EDITOR_PARAGRAPH: &str = "paragraph";
pub const EDITOR_HEADING: &str = "heading";
pub const EDITOR_CODE: &str = "code";
pub const EDITOR_LIST: &str = "list";
pub const EDITOR_IMAGE: &str = "image";
pub const EDITOR_TABLE: &str = "table";
pub const EDITOR_SEPARATOR: &str = "separator";
pub const EDITOR_MORE: &str = "more";

/// The full descriptor table — one entry per built-in block type.
///
/// The order defines display order in menus: paragraph, heading, code, list,
/// image, read-more, separator, table.
fn descriptor_table() -> &'static [BlockDescriptor] {
    &[
        BlockDescriptor {
            editor: EDITOR_PARAGRAPH,
            native: Some(EDITOR_PARAGRAPH),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Paragraph"),
                toolbar_label: Some("P"),
                category: "Text",
                default_block: || EditorBlock::paragraph(vec![]),
            }],
            default_block: || EditorBlock::paragraph(vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_HEADING,
            native: Some(EDITOR_HEADING),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[
                MenuEntry {
                    slash_label: Some("Heading 1"),
                    toolbar_label: Some("H1"),
                    category: "Text",
                    default_block: || EditorBlock::heading(1, vec![]),
                },
                MenuEntry {
                    slash_label: Some("Heading 2"),
                    toolbar_label: Some("H2"),
                    category: "Text",
                    default_block: || EditorBlock::heading(2, vec![]),
                },
                MenuEntry {
                    slash_label: Some("Heading 3"),
                    toolbar_label: Some("H3"),
                    category: "Text",
                    default_block: || EditorBlock::heading(3, vec![]),
                },
                MenuEntry {
                    slash_label: None,
                    toolbar_label: Some("H4"),
                    category: "Text",
                    default_block: || EditorBlock::heading(4, vec![]),
                },
                MenuEntry {
                    slash_label: None,
                    toolbar_label: Some("H5"),
                    category: "Text",
                    default_block: || EditorBlock::heading(5, vec![]),
                },
                MenuEntry {
                    slash_label: None,
                    toolbar_label: Some("H6"),
                    category: "Text",
                    default_block: || EditorBlock::heading(6, vec![]),
                },
            ],
            default_block: || EditorBlock::heading(1, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_CODE,
            native: Some(EDITOR_CODE),
            body_shape: BodyShape::Code,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Code block"),
                toolbar_label: Some("Code"),
                category: "Blocks",
                default_block: || EditorBlock::code(String::new(), String::new()),
            }],
            default_block: || EditorBlock::code(String::new(), String::new()),
        },
        BlockDescriptor {
            editor: EDITOR_LIST,
            native: Some(EDITOR_LIST),
            body_shape: BodyShape::List,
            builtin: true,
            menu: &[
                MenuEntry {
                    slash_label: Some("Unordered list"),
                    toolbar_label: Some("UL"),
                    category: "Blocks",
                    default_block: || EditorBlock::list(false, vec![]),
                },
                MenuEntry {
                    slash_label: Some("Ordered list"),
                    toolbar_label: Some("OL"),
                    category: "Blocks",
                    default_block: || EditorBlock::list(true, vec![]),
                },
            ],
            default_block: || EditorBlock::list(false, vec![]),
        },
        BlockDescriptor {
            editor: EDITOR_IMAGE,
            native: Some(EDITOR_IMAGE),
            body_shape: BodyShape::Opaque,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Image"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::image("", "", ""),
            }],
            default_block: || EditorBlock::image("", "", ""),
        },
        BlockDescriptor {
            editor: EDITOR_MORE,
            native: None,
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[], // "more" is inserted by a dedicated affordance, not the slash menu
            default_block: || EditorBlock::read_more(),
        },
        BlockDescriptor {
            editor: EDITOR_SEPARATOR,
            native: Some(EDITOR_SEPARATOR),
            body_shape: BodyShape::Inline,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Separator"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::separator(),
            }],
            default_block: || EditorBlock::separator(),
        },
        BlockDescriptor {
            editor: EDITOR_TABLE,
            native: Some(EDITOR_TABLE),
            body_shape: BodyShape::Table,
            builtin: true,
            menu: &[MenuEntry {
                slash_label: Some("Table"),
                toolbar_label: None,
                category: "Blocks",
                default_block: || EditorBlock::table_default(),
            }],
            default_block: || EditorBlock::table_default(),
        },
    ]
}

/// Return all descriptors in display order.
pub fn descriptors() -> &'static [BlockDescriptor] {
    descriptor_table()
}

/// Look up a descriptor by its `editor` key.
pub fn descriptor_for(editor: &str) -> Option<&'static BlockDescriptor> {
    descriptor_table().iter().find(|d| d.editor == editor)
}

/// Look up a descriptor by its native core type claim.
pub fn descriptor_for_native(core_type: &str) -> Option<&'static BlockDescriptor> {
    descriptor_table()
        .iter()
        .find(|d| d.native == Some(core_type))
}

/// Slash-menu items: descriptors filtered to entries with `slash_label`.
/// Returns `(label, default_block_fn)` tuples in display order.
///
/// # Panics
///
/// Panics if a descriptor's `slash_menu_entries` filter passes an entry
/// whose `slash_label` is `None` — this is a programming error since the
/// filter guarantees only `Some` entries are mapped.
#[allow(clippy::unwrap_used)] // safe: filter guarantees slash_label.is_some()
#[allow(clippy::type_complexity)] // a slice of (label, fn() -> EditorBlock) tuples is the natural menu-projection shape
pub fn slash_menu_entries() -> &'static [(&'static str, fn() -> EditorBlock)] {
    static CACHED: std::sync::OnceLock<Vec<(&'static str, fn() -> EditorBlock)>> =
        std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        descriptors()
            .iter()
            .flat_map(|d| d.menu.iter())
            .filter(|e| e.slash_label.is_some())
            .map(|e| (e.slash_label.unwrap(), e.default_block))
            .collect()
    })
}

/// Toolbar items: descriptors filtered to entries with `toolbar_label`.
/// Returns `(label, default_block_fn)` tuples in display order.
///
/// # Panics
///
/// Panics if a descriptor's `toolbar_menu_entries` filter passes an entry
/// whose `toolbar_label` is `None` — this is a programming error since the
/// filter guarantees only `Some` entries are mapped.
#[allow(clippy::unwrap_used)] // safe: filter guarantees toolbar_label.is_some()
#[allow(clippy::type_complexity)] // a slice of (label, fn() -> EditorBlock) tuples is the natural menu-projection shape
pub fn toolbar_menu_entries() -> &'static [(&'static str, fn() -> EditorBlock)] {
    static CACHED: std::sync::OnceLock<Vec<(&'static str, fn() -> EditorBlock)>> =
        std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        descriptors()
            .iter()
            .flat_map(|d| d.menu.iter())
            .filter(|e| e.toolbar_label.is_some())
            .map(|e| (e.toolbar_label.unwrap(), e.default_block))
            .collect()
    })
}

#[cfg(test)]
mod exclusivity_tests {
    use super::*;

    #[test]
    fn no_two_descriptors_share_editor_key() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            assert!(seen.insert(d.editor), "duplicate editor key: {}", d.editor);
        }
    }

    #[test]
    fn no_two_descriptors_share_native_claim() {
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(seen.insert(native), "duplicate native claim: {}", native);
            }
        }
    }

    #[test]
    fn descriptor_for_editor_finds_all() {
        for d in descriptors() {
            assert!(
                descriptor_for(d.editor).is_some(),
                "descriptor_for({}) returned None",
                d.editor
            );
        }
    }

    #[test]
    fn descriptor_for_native_finds_all_native() {
        for d in descriptors() {
            if let Some(native) = d.native {
                assert!(
                    descriptor_for_native(native).is_some(),
                    "descriptor_for_native({}) returned None",
                    native
                );
            }
        }
    }

    #[test]
    fn descriptors_align_with_descriptor_bodies() {
        // Descriptor body_shapes agree on the mapping:
        // paragraph → Inline, heading → Inline, code → Code, list → List,
        // table → Table, image → Opaque.

        let paragraph_desc = descriptor_for(EDITOR_PARAGRAPH).unwrap();
        assert!(matches!(paragraph_desc.body_shape, BodyShape::Inline));

        let heading_desc = descriptor_for(EDITOR_HEADING).unwrap();
        assert!(matches!(heading_desc.body_shape, BodyShape::Inline));

        let code_desc = descriptor_for(EDITOR_CODE).unwrap();
        assert!(matches!(code_desc.body_shape, BodyShape::Code));

        let list_desc = descriptor_for(EDITOR_LIST).unwrap();
        assert!(matches!(list_desc.body_shape, BodyShape::List));

        let table_desc = descriptor_for(EDITOR_TABLE).unwrap();
        assert!(matches!(table_desc.body_shape, BodyShape::Table));

        let image_desc = descriptor_for(EDITOR_IMAGE).unwrap();
        assert!(matches!(image_desc.body_shape, BodyShape::Opaque));
    }

    #[test]
    fn all_descriptors_have_consistent_editor_and_native() {
        // Every descriptor that has a native claim must find a matching
        // descriptor when looked up by that native value.
        for d in descriptors() {
            if let Some(native) = d.native {
                assert_eq!(
                    descriptor_for_native(native).map(|x| x.editor),
                    Some(d.editor),
                    "native '{}' maps to wrong editor",
                    native
                );
            }
        }
    }

    #[test]
    fn descriptor_default_blocks_produce_valid_blocks() {
        use crate::model::types::BlockBody;

        // Each descriptor's default_block closure must produce a block whose
        // body_shape matches the descriptor's declared body_shape.
        for desc in descriptors() {
            let block = (desc.default_block)();
            match desc.body_shape {
                BodyShape::Inline => {
                    assert!(matches!(&block.body, BlockBody::Inline(_)));
                }
                BodyShape::Code => {
                    assert!(matches!(&block.body, BlockBody::Code(_)));
                }
                BodyShape::List => {
                    assert!(matches!(&block.body, BlockBody::List(_)));
                }
                BodyShape::Table => {
                    assert!(matches!(&block.body, BlockBody::Table(_)));
                }
                BodyShape::Opaque => {
                    assert!(matches!(&block.body, BlockBody::Opaque(_)));
                }
            }
        }
    }
}

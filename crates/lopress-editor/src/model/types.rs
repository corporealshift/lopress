use lopress_plugin::AttrDecl;
use serde_json::Value;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable identity for a block within an open document. Not persisted to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u64);

impl BlockId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Raw monotonic counter value. Stable but opaque — for comparison
    /// fallbacks when block identity outlives presence in the doc.
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorDoc {
    pub blocks: Vec<EditorBlock>,
    pub front_matter: lopress_core::FrontMatter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorBlock {
    pub id: BlockId,
    pub kind: BlockKind,
    pub body: BlockBody,
    pub plugin: PluginMeta,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8), // 1..=6
    Code { lang: Rc<str> },
    List { ordered: bool },
    Image,
    Table,
    Opaque { type_name: Rc<str> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockBody {
    Inline(Vec<InlineRun>),
    Code(String),
    List(Vec<ListItem>),
    Table(TableData),
    Opaque(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub id: BlockId,
    pub runs: Vec<InlineRun>,
}

/// Column alignment for a table. Maps to the `attrs.align` strings on disk
/// ("none"/"left"/"center"/"right") and to GFM delimiter-row tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Align::None => "none",
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s {
            "left" => Align::Left,
            "center" => Align::Center,
            "right" => Align::Right,
            _ => Align::None,
        }
    }
}

/// One table cell: an id (for focus) plus its inline runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub id: BlockId,
    pub runs: Vec<InlineRun>,
}

/// One table row: an id plus its cells. `rows[0]` of a `TableData` is the header.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub id: BlockId,
    pub cells: Vec<TableCell>,
}

/// A table body: per-column alignment plus the rows (row 0 = header).
#[derive(Debug, Clone, PartialEq)]
pub struct TableData {
    pub align: Vec<Align>,
    pub rows: Vec<TableRow>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct InlineRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl InlineRun {
    pub fn plain<S: Into<String>>(text: S) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    pub block_type_name: Rc<str>,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Rc<[AttrDecl]>,
    /// True when this block is owned by a built-in base plugin. The plugin
    /// block view suppresses chrome (header strip, attr form) when set.
    pub builtin: bool,
    /// The block's editor key (manifest `editor` field). Drives `render_body`
    /// dispatch via the editor registry. `None` → generic attr-form editor.
    pub editor: Option<Rc<str>>,
    /// The native core type this block claims (manifest `native` field).
    /// `Some` → `to_core` serializes it as bare native markdown of this type.
    /// `None` → `to_core` uses the comment container.
    pub native: Option<Rc<str>>,
}

impl PluginMeta {
    /// The canonical `PluginMeta` for a built-in paragraph block.
    ///
    /// Mirrors what `from_core` stamps for a `paragraph` core block, so a
    /// paragraph created inside the editor (e.g. via `ChangeType` from the
    /// toolbar or slash menu) carries the same plugin identity as one loaded
    /// from disk — taking the plugin render path and native serialization.
    /// `attr_decls` is empty: the paragraph is `builtin`, so the attr form
    /// is suppressed.
    pub fn paragraph() -> Self {
        Self {
            block_type_name: Rc::from("paragraph"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("paragraph")),
            native: Some(Rc::from("paragraph")),
        }
    }

    /// The canonical `PluginMeta` for a built-in heading block.
    ///
    /// Mirrors what `from_core` stamps for a `heading` core block, so a
    /// heading created inside the editor (e.g. via `ChangeType` from the
    /// toolbar or slash menu) carries the same plugin identity as one loaded
    /// from disk. `attrs["level"]` mirrors `BlockKind::Heading(level)` so
    /// the heading widget can read the level from attrs (not from the enum).
    /// `attr_decls` is empty: the heading is `builtin`, so the attr form
    /// is suppressed.
    pub fn heading(level: u8) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert(
            "level".to_string(),
            Value::Number(serde_json::Number::from(level)),
        );
        Self {
            block_type_name: Rc::from("heading"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("heading")),
            native: Some(Rc::from("heading")),
        }
    }

    /// The canonical `PluginMeta` for a built-in list block.
    ///
    /// Mirrors what `from_core` stamps for a `list` core block, so a list
    /// created inside the editor (e.g. via `ChangeType` from the toolbar or
    /// slash menu) carries the same plugin identity as one loaded from disk —
    /// taking the plugin render path and native serialization. `attr_decls`
    /// is empty: the list is `builtin`, so the attr form is suppressed.
    pub fn list(ordered: bool) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("ordered".to_string(), Value::Bool(ordered));
        Self {
            block_type_name: Rc::from("list"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("list")),
            native: Some(Rc::from("list")),
        }
    }

    /// The canonical `PluginMeta` for a built-in code block.
    ///
    /// Mirrors what `from_core` stamps for a `code` core block, so a code
    /// created inside the editor (e.g. via `ChangeType` from the toolbar or
    /// slash menu) carries the same plugin identity as one loaded from disk.
    /// `attr_decls` is empty: the code block is `builtin`, so the attr form
    /// is suppressed.
    pub fn code(lang: &str) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("lang".to_string(), Value::String(lang.to_string()));
        Self {
            block_type_name: Rc::from("code"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("code")),
            native: Some(Rc::from("code")),
        }
    }

    /// The canonical `PluginMeta` for an image block. Native `image` claim,
    /// built-in (chrome suppressed), edited via the `"image"` widget. `attrs`
    /// carries `src` (+ optional `alt`/`caption`).
    pub fn image(src: &str, alt: &str, caption: &str) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("src".to_string(), Value::String(src.to_string()));
        if !alt.is_empty() {
            attrs.insert("alt".to_string(), Value::String(alt.to_string()));
        }
        if !caption.is_empty() {
            attrs.insert("caption".to_string(), Value::String(caption.to_string()));
        }
        Self {
            block_type_name: Rc::from("image"),
            attrs,
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("image")),
            native: Some(Rc::from("image")),
        }
    }

    /// The canonical `PluginMeta` for the read-more marker.
    ///
    /// A comment-container block (no `native` claim), built-in (chrome
    /// suppressed), edited via the `"more"` divider widget. No attrs.
    pub fn read_more() -> Self {
        Self {
            block_type_name: Rc::from("lopress:more"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("more")),
            native: None,
        }
    }

    /// `PluginMeta` for the separator: a native `separator` claim, built-in
    /// (chrome suppressed), edited via the `"separator"` divider widget. No attrs.
    pub fn separator() -> Self {
        Self {
            block_type_name: Rc::from("separator"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("separator")),
            native: Some(Rc::from("separator")),
        }
    }

    /// `PluginMeta` for a table: native `table` claim, built-in (chrome
    /// suppressed), edited via the `"table"` widget. No attr-form attrs (the
    /// align array lives in the table body, not the attr form).
    pub fn table() -> Self {
        Self {
            block_type_name: Rc::from("table"),
            attrs: serde_json::Map::new(),
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("table")),
            native: Some(Rc::from("table")),
        }
    }
}

impl EditorBlock {
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(runs),
            plugin: PluginMeta::paragraph(),
        }
    }

    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        let level = level.clamp(1, 6);
        Self {
            id: BlockId::new(),
            kind: BlockKind::Heading(level),
            body: BlockBody::Inline(runs),
            plugin: PluginMeta::heading(level),
        }
    }

    pub fn code(lang: String, text: String) -> Self {
        let meta = PluginMeta {
            block_type_name: Rc::from("code"),
            attrs: {
                let mut m = serde_json::Map::new();
                m.insert("lang".to_string(), Value::String(lang.clone()));
                m
            },
            attr_decls: Rc::from([]),
            builtin: true,
            editor: Some(Rc::from("code")),
            native: Some(Rc::from("code")),
        };
        Self {
            id: BlockId::new(),
            kind: BlockKind::Code {
                lang: Rc::from(lang),
            },
            body: BlockBody::Code(text),
            plugin: meta,
        }
    }

    pub fn list(ordered: bool, items: Vec<ListItem>) -> Self {
        let mut attrs = serde_json::Map::new();
        attrs.insert("ordered".to_string(), Value::Bool(ordered));
        Self {
            id: BlockId::new(),
            kind: BlockKind::List { ordered },
            body: BlockBody::List(items),
            plugin: PluginMeta {
                block_type_name: Rc::from("list"),
                attrs,
                attr_decls: Rc::from([]),
                builtin: true,
                editor: Some(Rc::from("list")),
                native: Some(Rc::from("list")),
            },
        }
    }

    pub fn opaque(type_name: String, value: Value) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Opaque {
                type_name: Rc::from(type_name.clone()),
            },
            body: BlockBody::Opaque(value),
            plugin: PluginMeta {
                block_type_name: Rc::from(type_name),
                attrs: serde_json::Map::new(),
                attr_decls: Rc::from([]),
                builtin: false,
                editor: None,
                native: None,
            },
        }
    }

    /// The read-more marker block: an empty-bodied plugin block carrying
    /// `PluginMeta::read_more`. The body is an empty inline run vec — the marker
    /// renders via its editor widget and serializes to an empty container.
    pub fn read_more() -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(vec![]),
            plugin: PluginMeta::read_more(),
        }
    }

    /// The separator block: an empty-bodied plugin block carrying
    /// `PluginMeta::separator`. Renders via its divider widget and serializes
    /// to a bare `---`.
    pub fn separator() -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(vec![]),
            plugin: PluginMeta::separator(),
        }
    }

    /// A table block from explicit data.
    pub fn table(data: TableData) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Table,
            body: BlockBody::Table(data),
            plugin: PluginMeta::table(),
        }
    }

    /// The default inserted table: 2 columns × 2 rows (1 header + 1 body),
    /// empty cells, alignment `none`. Used by both the slash menu and the
    /// toolbar button.
    pub fn table_default() -> Self {
        let empty_cell = || TableCell {
            id: BlockId::new(),
            runs: vec![],
        };
        let row = || TableRow {
            id: BlockId::new(),
            cells: vec![empty_cell(), empty_cell()],
        };
        Self::table(TableData {
            align: vec![Align::None, Align::None],
            rows: vec![row(), row()],
        })
    }

    /// An image block. State (src/alt/caption) lives in `PluginMeta.attrs`;
    /// the body is an empty Opaque placeholder (images have no editable
    /// text/children).
    pub fn image(src: &str, alt: &str, caption: &str) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Image,
            body: BlockBody::Opaque(Value::Null),
            plugin: PluginMeta::image(src, alt, caption),
        }
    }

    /// A fresh plugin comment-container block for insertion from the slash menu.
    ///
    /// The block is `Opaque` with an empty body (`Value::Null`) and carries
    /// `PluginMeta` with default attribute values from the inserter item.
    /// This shape round-trips through `to_core` as a `<!-- lopress:NAME {attrs} -->`
    /// / `<!-- /lopress:NAME -->` comment container.
    pub fn from_plugin_item(item: &crate::model::inserter::PluginInserterItem) -> Self {
        // Mirror exactly what `plugin_block_from_core` produces for a loaded
        // comment-container plugin block (its `editor`-less default arm): a
        // `Paragraph` kind with an empty `Inline` body. Using `Opaque`/`Opaque(Null)`
        // here would make the plugin block view render the scary "raw content,
        // can't be edited" fallback panel instead of just the attr form.
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(Vec::new()),
            plugin: PluginMeta {
                block_type_name: item.type_name.clone(),
                attrs: item.default_attrs.clone(),
                attr_decls: item.attr_decls.clone(),
                builtin: false,
                editor: None,
                native: None,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod paragraph_heading_meta_tests {
    use super::*;

    #[test]
    fn paragraph_block_carries_plugin_meta() {
        let b = EditorBlock::paragraph(vec![InlineRun::plain("hello")]);
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "paragraph");
        assert_eq!(meta.editor.as_deref(), Some("paragraph"));
        assert_eq!(meta.native.as_deref(), Some("paragraph"));
        assert!(meta.builtin);
        assert!(meta.attrs.is_empty());
    }

    #[test]
    fn heading_block_carries_plugin_meta_with_level() {
        let b = EditorBlock::heading(3, vec![InlineRun::plain("title")]);
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "heading");
        assert_eq!(meta.editor.as_deref(), Some("heading"));
        assert_eq!(meta.native.as_deref(), Some("heading"));
        assert!(meta.builtin);
        assert_eq!(meta.attrs.get("level").and_then(Value::as_u64), Some(3));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod image_ctor_tests {
    use super::*;

    #[test]
    fn image_block_carries_attrs_in_meta() {
        let b = EditorBlock::image("/images/p.jpg", "alt text", "");
        assert!(matches!(b.kind, BlockKind::Image));
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "image");
        assert_eq!(meta.editor.as_deref(), Some("image"));
        assert_eq!(meta.native.as_deref(), Some("image"));
        assert_eq!(
            meta.attrs.get("src").and_then(|v| v.as_str()),
            Some("/images/p.jpg")
        );
        assert_eq!(
            meta.attrs.get("alt").and_then(|v| v.as_str()),
            Some("alt text")
        );
        assert!(!meta.attrs.contains_key("caption"), "empty caption omitted");
        assert!(matches!(b.body, BlockBody::Opaque(serde_json::Value::Null)));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod read_more_ctor_tests {
    use super::*;

    #[test]
    fn read_more_block_has_marker_meta() {
        let b = EditorBlock::read_more();
        // The constructor always sets plugin.
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "lopress:more");
        assert_eq!(meta.editor.as_deref(), Some("more"));
        assert!(meta.builtin);
        assert!(meta.native.is_none());
        assert!(matches!(b.body, BlockBody::Inline(ref runs) if runs.is_empty()));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]
mod plugin_inserter_ctor_tests {
    use super::*;
    use crate::model::inserter::PluginInserterItem;
    use serde_json::Map;
    use std::rc::Rc;

    fn test_item() -> PluginInserterItem {
        let mut attrs = Map::new();
        attrs.insert("foo".to_string(), Value::String("bar".to_string()));
        PluginInserterItem {
            type_name: Rc::from("lopress:test"),
            title: "Test".to_string(),
            category: "Blocks".to_string(),
            attr_decls: Rc::from([]),
            default_attrs: attrs,
        }
    }

    #[test]
    fn from_plugin_item_builds_comment_container_block_with_meta() {
        let item = test_item();
        let b = EditorBlock::from_plugin_item(&item);
        // Mirrors a loaded comment-container plugin block: Paragraph kind,
        // empty Inline body, identity carried in PluginMeta.
        assert!(matches!(b.kind, BlockKind::Paragraph));
        assert!(matches!(&b.body, BlockBody::Inline(runs) if runs.is_empty()));
        let meta = &b.plugin;
        assert_eq!(&*meta.block_type_name, "lopress:test");
        assert_eq!(meta.attrs.get("foo").and_then(Value::as_str), Some("bar"));
        assert!(!meta.builtin);
        assert!(meta.editor.is_none());
        assert!(meta.native.is_none());
    }
}

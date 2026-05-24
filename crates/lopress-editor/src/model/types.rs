use lopress_plugin::AttrDecl;
use serde_json::Value;
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
    pub plugin: Option<PluginMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8), // 1..=6
    Code { lang: String },
    List { ordered: bool },
    Opaque { type_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockBody {
    Inline(Vec<InlineRun>),
    Code(String),
    List(Vec<ListItem>),
    Opaque(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub id: BlockId,
    pub runs: Vec<InlineRun>,
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
    pub block_type_name: String,
    pub attrs: serde_json::Map<String, Value>,
    pub attr_decls: Vec<AttrDecl>,
    /// True when this block is owned by a built-in base plugin. The plugin
    /// block view suppresses chrome (header strip, attr form) when set.
    pub builtin: bool,
    /// The block's editor key (manifest `editor` field). Drives `render_body`
    /// dispatch via the editor registry. `None` → generic attr-form editor.
    pub editor: Option<String>,
    /// The native core type this block claims (manifest `native` field).
    /// `Some` → `to_core` serializes it as bare native markdown of this type.
    /// `None` → `to_core` uses the comment container.
    pub native: Option<String>,
}

impl PluginMeta {
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
            block_type_name: "list".to_string(),
            attrs,
            attr_decls: Vec::new(),
            builtin: true,
            editor: Some("list".to_string()),
            native: Some("list".to_string()),
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
            block_type_name: "code".to_string(),
            attrs,
            attr_decls: Vec::new(),
            builtin: true,
            editor: Some("code".to_string()),
            native: Some("code".to_string()),
        }
    }
}

impl EditorBlock {
    pub fn paragraph(runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Paragraph,
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }

    pub fn heading(level: u8, runs: Vec<InlineRun>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Heading(level.clamp(1, 6)),
            body: BlockBody::Inline(runs),
            plugin: None,
        }
    }

    pub fn code(lang: String, text: String) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Code { lang },
            body: BlockBody::Code(text),
            plugin: None,
        }
    }

    pub fn list(ordered: bool, items: Vec<ListItem>) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::List { ordered },
            body: BlockBody::List(items),
            plugin: None,
        }
    }

    pub fn opaque(type_name: String, value: Value) -> Self {
        Self {
            id: BlockId::new(),
            kind: BlockKind::Opaque {
                type_name: type_name.clone(),
            },
            body: BlockBody::Opaque(value),
            plugin: None,
        }
    }
}

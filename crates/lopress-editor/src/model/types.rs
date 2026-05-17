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

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A parsed markdown file: front-matter plus the root block tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub front_matter: FrontMatter,
    pub blocks: Vec<Block>,
}

/// Front-matter fields. Unknown fields are captured in `extra` so plugins can
/// read them without the core having to know about them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FrontMatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

/// One node in the block tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// e.g. "paragraph", "heading", "lopress:video"
    pub r#type: String,
    /// Structured attributes. For headings: `{"level": 2}`. For custom blocks:
    /// whatever JSON the user wrote in the opening comment.
    #[serde(default = "empty_attrs")]
    pub attrs: Value,
    /// Nested blocks (for containers like `columns`, `callout`).
    #[serde(default)]
    pub children: Vec<Block>,
    /// Raw inline text for text-like blocks (paragraph, heading, code-block
    /// body). `None` for container blocks.
    #[serde(default)]
    pub text: Option<String>,
}

fn empty_attrs() -> Value {
    Value::Object(serde_json::Map::new())
}

impl Block {
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(text.into()),
        }
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        Self {
            r#type: "heading".into(),
            attrs: serde_json::json!({ "level": level }),
            children: vec![],
            text: Some(text.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_constructor_sets_text() {
        let b = Block::paragraph("hello");
        assert_eq!(b.r#type, "paragraph");
        assert_eq!(b.text.as_deref(), Some("hello"));
        assert!(b.children.is_empty());
    }

    #[test]
    fn heading_constructor_sets_level() {
        let b = Block::heading(2, "title");
        assert_eq!(b.r#type, "heading");
        assert_eq!(b.attrs, serde_json::json!({ "level": 2 }));
    }

    #[test]
    fn document_roundtrips_through_json() {
        let d = Document {
            front_matter: FrontMatter {
                title: Some("t".into()),
                ..Default::default()
            },
            blocks: vec![Block::paragraph("p")],
        };
        let s = serde_json::to_string(&d).unwrap();
        let d2: Document = serde_json::from_str(&s).unwrap();
        assert_eq!(d, d2);
    }
}

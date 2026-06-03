//! Compute the list of insertable plugin blocks from a `PluginRegistry`.
//!
//! A block is insertable when it is a comment-container plugin block:
//! it has a `template` OR a `markdown_template`, is not `builtin`,
//! and does not claim a `native` core type.

use lopress_plugin::{AttrDecl, PluginRegistry};
use serde_json::{Map, Value};
use std::rc::Rc;

/// An item offered in the slash menu as an insertable plugin block.
#[derive(Debug, Clone)]
pub struct PluginInserterItem {
    /// The block type name (e.g. `"lopress:callout"`). Used to construct
    /// `BlockKind::Opaque { type_name }` and `PluginMeta.block_type_name`.
    pub type_name: Rc<str>,
    /// Human-readable label shown in the slash menu. Derived from the
    /// manifest `title` field or, when absent, from the block `name`
    /// (stripping the `lopress:` prefix and title-casing).
    pub title: String,
    /// Category bucket for grouping in the menu. Falls back to `"Blocks"`.
    pub category: String,
    /// Attribute declarations from the manifest, in declaration order.
    pub attr_decls: Rc<[AttrDecl]>,
    /// Default attribute values: for each `AttrDecl` that has a `default`,
    /// the corresponding key→value pair. Used to seed the fresh block's
    /// `PluginMeta.attrs`.
    pub default_attrs: Map<String, Value>,
}

/// Derive a display title from a block name.
///
/// Strips a leading `lopress:` prefix (lower-cased) and title-cases the
/// first hyphen-separated segment; the rest are lower-cased.
pub fn derive_title(name: &str) -> String {
    let stripped = name.strip_prefix("lopress:").unwrap_or(name);
    let segments: Vec<&str> = stripped.split('-').collect();
    segments
        .into_iter()
        .enumerate()
        .map(|(i, seg)| {
            if i == 0 {
                // Title-case the first segment.
                let mut chars = seg.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let upper: String = first.to_uppercase().collect();
                        upper + &chars.as_str().to_lowercase()
                    }
                }
            } else {
                seg.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the default-attrs map from a list of `AttrDecl`.
///
/// For each decl that has a `default`, include the key→value pair.
fn build_default_attrs(attrs: &std::collections::BTreeMap<String, AttrDecl>) -> Map<String, Value> {
    attrs
        .iter()
        .filter_map(|(k, v)| v.default.as_ref().map(|d| (k.clone(), d.clone())))
        .collect()
}

/// Compute the list of insertable plugin blocks from the registry.
///
/// A `BlockDecl` is offered when:
///   `(template.is_some() || markdown_template.is_some()) && !builtin && native.is_none()`
///
/// Items are returned in registration order (plugin order, then block order
/// within each plugin).
pub fn inserter_items(registry: &PluginRegistry) -> Vec<PluginInserterItem> {
    registry
        .plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .manifest
                .blocks
                .iter()
                .filter(|decl| is_insertable(decl))
                .map(move |decl| make_item(plugin, decl))
        })
        .collect()
}

/// True when the block is a comment-container plugin block eligible for
/// insertion from the slash menu.
fn is_insertable(decl: &lopress_plugin::BlockDecl) -> bool {
    let has_template = decl.template.is_some() || decl.markdown_template.is_some();
    !decl.builtin && decl.native.is_none() && has_template
}

/// Build a single `PluginInserterItem` from a plugin + block decl pair.
fn make_item(_plugin: &lopress_plugin::LoadedPlugin, decl: &lopress_plugin::BlockDecl) -> PluginInserterItem {
    let type_name: Rc<str> = decl.name.clone().into();
    let title = decl
        .title
        .clone()
        .unwrap_or_else(|| derive_title(&decl.name));
    let category = decl.category.clone().unwrap_or_else(|| "Blocks".to_string());
    let attr_decls: Rc<[AttrDecl]> = Rc::from(decl.attrs.values().cloned().collect::<Vec<_>>());
    let default_attrs = build_default_attrs(&decl.attrs);

    PluginInserterItem {
        type_name,
        title,
        category,
        attr_decls,
        default_attrs,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopress_plugin::{AttrType, BlockDecl, LoadedPlugin, PluginManifest};
    use std::collections::BTreeMap;

    fn make_plugin(
        name: &str,
        blocks: Vec<BlockDecl>,
    ) -> LoadedPlugin {
        LoadedPlugin {
            root: std::path::PathBuf::from("/fake"),
            manifest: PluginManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                theme: false,
                blocks,
            },
        }
    }

    fn make_decl(
        block_name: &str,
        template: Option<&str>,
        markdown_template: Option<&str>,
        builtin: bool,
        native: Option<&str>,
        title: Option<&str>,
        category: Option<&str>,
    ) -> BlockDecl {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "foo".to_string(),
            AttrDecl {
                kind: AttrType::String,
                required: false,
                default: Some(Value::String("bar".to_string())),
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        BlockDecl {
            name: block_name.to_string(),
            template: template.map(String::from),
            markdown_template: markdown_template.map(String::from),
            attrs,
            renderer: None,
            editor: None,
            builtin,
            native: native.map(String::from),
            css: Vec::new(),
            js: Vec::new(),
            title: title.map(String::from),
            description: None,
            category: category.map(String::from),
        }
    }

    #[test]
    fn filters_out_builtin_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "base",
            vec![make_decl("list", None, None, true, Some("list"), None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "builtin/native blocks must be excluded");
    }

    #[test]
    fn filters_out_native_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "ext",
            vec![make_decl("lopress:embed", None, None, false, Some("embed"), None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "native blocks must be excluded");
    }

    #[test]
    fn includes_markdown_template_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "callout",
            vec![make_decl(
                "lopress:callout",
                None,
                Some("blocks/callout.md"),
                false,
                None,
                Some("Callout"),
                Some("Text"),
            )],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert_eq!(items.len(), 1);
        assert_eq!(&*items[0].type_name, "lopress:callout");
        assert_eq!(items[0].title, "Callout");
        assert_eq!(items[0].category, "Text");
    }

    #[test]
    fn includes_html_template_blocks() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "button",
            vec![make_decl(
                "lopress:button",
                Some("blocks/button.html"),
                None,
                false,
                None,
                None,
                None,
            )],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert_eq!(items.len(), 1);
        assert_eq!(&*items[0].type_name, "lopress:button");
        // Title derived from name: "lopress:button" → "Button"
        assert_eq!(items[0].title, "Button");
        assert_eq!(items[0].category, "Blocks");
    }

    #[test]
    fn derives_title_from_name_when_absent() {
        assert_eq!(derive_title("lopress:author-bio"), "Author bio");
        assert_eq!(derive_title("lopress:callout"), "Callout");
        assert_eq!(derive_title("lopress:pull-quote"), "Pull quote");
        assert_eq!(derive_title("standalone"), "Standalone");
    }

    #[test]
    fn default_attrs_contains_decl_defaults() {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "foo".to_string(),
            AttrDecl {
                kind: AttrType::String,
                required: false,
                default: Some(Value::String("bar".to_string())),
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        attrs.insert(
            "baz".to_string(),
            AttrDecl {
                kind: AttrType::Bool,
                required: false,
                default: None,
                ui: None,
                label: None,
                help: None,
                options: Vec::new(),
            },
        );
        let defaults = build_default_attrs(&attrs);
        assert_eq!(defaults.get("foo").and_then(Value::as_str), Some("bar"));
        assert!(!defaults.contains_key("baz"), "no default → omitted");
    }

    #[test]
    fn excludes_blocks_with_no_template() {
        let mut reg = PluginRegistry::default();
        reg.insert(make_plugin(
            "editor-only",
            vec![make_decl("lopress:foo", None, None, false, None, None, None)],
        ))
        .unwrap();
        let items = inserter_items(&reg);
        assert!(items.is_empty(), "blocks without template/markdown_template are excluded");
    }
}

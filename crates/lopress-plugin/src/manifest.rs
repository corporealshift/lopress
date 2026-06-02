use crate::error::PluginError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub theme: bool,
    #[serde(default)]
    pub blocks: Vec<BlockDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockDecl {
    pub name: String,
    /// HTML template path, relative to the plugin root. Absent for built-in
    /// ("base") plugins, which provide an editor rather than a renderer.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub attrs: BTreeMap<String, AttrDecl>,
    #[serde(default)]
    pub renderer: Option<String>,
    #[serde(default)]
    pub editor: Option<String>,
    /// When true this block ships as part of the core codebase. The editor
    /// suppresses plugin chrome (header strip, attr form) for builtin blocks.
    #[serde(default)]
    pub builtin: bool,
    /// Capability #2 — Transform. When set, this block IS a native markdown
    /// construct identified by this `lopress_core` Block type. The value is an
    /// exclusive claim (see `PluginRegistry`). Absent → comment-container form.
    #[serde(default)]
    pub native: Option<String>,
    /// Capability #3 — Assets. CSS files this block contributes to the page
    /// `<head>`. Parsed and exposed; build-side injection is a follow-up.
    #[serde(default)]
    pub css: Vec<String>,
    /// Capability #3 — Assets. JS files this block contributes to the page
    /// `<head>`. Parsed and exposed; build-side injection is a follow-up.
    #[serde(default)]
    pub js: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AttrType {
    String,
    Number,
    Bool,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttrDecl {
    #[serde(rename = "type")]
    pub kind: AttrType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

pub fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let src = std::fs::read_to_string(path).map_err(|source| PluginError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest: PluginManifest = toml::from_str(&src).map_err(|e| PluginError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    Ok(manifest)
}

/// Parse a manifest from an in-memory TOML string. Used for base plugins
/// embedded via `include_str!`, which have no path on disk.
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, contents: &str) -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn parses_minimal_theme_plugin() {
        let dir = TempDir::new().unwrap();
        let p = write(
            &dir,
            "plugin.toml",
            r#"
name = "default"
version = "0.1.0"
theme = true
"#,
        );
        let m = parse_manifest(&p).unwrap();
        assert_eq!(m.name, "default");
        assert!(m.theme);
        assert!(m.blocks.is_empty());
    }

    #[test]
    fn parses_plugin_with_blocks_and_attrs() {
        let dir = TempDir::new().unwrap();
        let p = write(
            &dir,
            "plugin.toml",
            r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }
"#,
        );
        let m = parse_manifest(&p).unwrap();
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.name, "lopress:video");
        assert!(b.attrs.contains_key("src"));
        assert_eq!(b.attrs["src"].kind, AttrType::String);
        assert!(b.attrs["src"].required);
        assert_eq!(b.attrs["src"].ui.as_deref(), Some("text"));
    }

    #[test]
    fn errors_on_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let p = write(&dir, "plugin.toml", "this is not toml = = = =");
        assert!(parse_manifest(&p).is_err());
    }

    #[test]
    fn parses_manifest_from_str_with_builtin_block() {
        let src = r#"
name = "lopress-list"
version = "0.1.0"

[[blocks]]
name    = "list"
editor  = "list"
builtin = true

[blocks.attrs]
ordered = { type = "bool", ui = "hidden" }
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.name, "lopress-list");
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.name, "list");
        assert!(b.builtin);
        assert!(b.template.is_none());
        assert_eq!(b.editor.as_deref(), Some("list"));
        assert!(b.attrs.contains_key("ordered"));
    }

    #[test]
    fn builtin_defaults_to_false() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        assert!(!m.blocks[0].builtin);
        assert_eq!(m.blocks[0].template.as_deref(), Some("blocks/video.html"));
    }

    #[test]
    fn parses_read_more_marker_manifest() {
        let src = r#"
name    = "lopress-more"
version = "0.1.0"

[[blocks]]
name    = "lopress:more"
editor  = "more"
builtin = true
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.name, "lopress-more");
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.name, "lopress:more");
        assert_eq!(b.editor.as_deref(), Some("more"));
        assert!(b.builtin);
        assert!(b.native.is_none());
        assert!(b.template.is_none());
        assert!(b.attrs.is_empty());
    }

    #[test]
    fn parses_native_and_asset_fields() {
        let src = r#"
name = "lopress-list"
version = "0.1.0"

[[blocks]]
name    = "list"
editor  = "list"
native  = "list"
builtin = true
css     = ["assets/list.css"]
js      = ["assets/list.js"]
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert_eq!(b.native.as_deref(), Some("list"));
        assert_eq!(b.css, vec!["assets/list.css".to_string()]);
        assert_eq!(b.js, vec!["assets/list.js".to_string()]);
    }

    #[test]
    fn native_and_assets_default_to_empty() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert!(b.native.is_none());
        assert!(b.css.is_empty());
        assert!(b.js.is_empty());
    }
}

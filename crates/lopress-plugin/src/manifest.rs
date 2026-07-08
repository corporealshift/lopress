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
    /// Capability #3 — Assets. Top-level CSS/JS the plugin contributes to
    /// every rendered page. This is the mechanism block-less asset plugins
    /// (syntax highlighting, toc, lightbox) use; block plugins may declare it
    /// too (e.g. `series`). Paths are relative to the plugin root.
    #[serde(default)]
    pub assets: PluginAssets,
}

/// Top-level `[assets]` table: CSS/JS a plugin injects into every page.
/// Paths are relative to the plugin root (e.g. `assets/code.css`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PluginAssets {
    #[serde(default)]
    pub css: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockDecl {
    pub name: String,
    /// HTML template path, relative to the plugin root. Absent for built-in
    /// ("base") plugins, which provide an editor widget rather than a static
    /// template.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub attrs: BTreeMap<String, AttrDecl>,
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
    /// Capability #3 — Assets. CSS files this block contributes, unioned with
    /// the plugin's top-level `[assets]` and injected into every page. Most
    /// plugins should declare assets at the top level (`PluginAssets`) instead.
    #[serde(default)]
    pub css: Vec<String>,
    /// Capability #3 — Assets. JS files this block contributes. Same handling
    /// as `css`.
    #[serde(default)]
    pub js: Vec<String>,
    /// Tera markdown-template path, relative to the plugin root.
    /// Mutually exclusive with `template`. When present, the block
    /// is a *template-form* block: form values interpolate into this
    /// markdown template, and the result flows through the md→HTML pipeline.
    #[serde(default)]
    pub markdown_template: Option<String>,
    /// Inserter menu label. When absent, the editor derives one from `name`.
    #[serde(default)]
    pub title: Option<String>,
    /// Inserter menu description / secondary line.
    #[serde(default)]
    pub description: Option<String>,
    /// Inserter grouping bucket (e.g. "Text", "Media"). Falls back to "Blocks".
    #[serde(default)]
    pub category: Option<String>,
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
    /// The field name (the TOML key under `[blocks.attrs]`).
    ///
    /// Populated at parse time from the map key; it is NOT a TOML field
    /// itself, so serde must not expect it. `#[serde(skip)]` gives it
    /// `Default::default()` (= `""`) after deserialization, which the
    /// parse functions overwrite for every attr.
    #[serde(skip)]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: AttrType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub ui: Option<String>,
    /// Human-friendly field caption. When absent, the form falls back to
    /// the attr key name.
    #[serde(default)]
    pub label: Option<String>,
    /// Helper / description text shown beneath the label.
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

/// Validate a parsed manifest for semantic constraints.
fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginError> {
    for block in &manifest.blocks {
        if block.template.is_some() && block.markdown_template.is_some() {
            return Err(PluginError::MutualExclusion {
                field1: "template".to_string(),
                field2: "markdown_template".to_string(),
            });
        }
    }
    Ok(())
}

/// Populate `AttrDecl.name` for every attr in every block.
///
/// The name is the TOML key under `[blocks.attrs]` — it is not a value
/// field and is never serialized. This must run after every deserialize
/// path so that consumers (registry, editor, tests) always see populated
/// names.
fn populate_attr_names(manifest: &mut PluginManifest) {
    for block in &mut manifest.blocks {
        for (key, decl) in &mut block.attrs {
            decl.name = key.clone();
        }
    }
}

pub fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let src = std::fs::read_to_string(path).map_err(|source| PluginError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut manifest: PluginManifest = toml::from_str(&src).map_err(|e| PluginError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    populate_attr_names(&mut manifest);
    Ok(manifest)
}

/// Parse a manifest from an in-memory TOML string. Used for base plugins
/// embedded via `include_str!`, which have no path on disk.
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    let mut manifest: PluginManifest = toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    populate_attr_names(&mut manifest);
    Ok(manifest)
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

    #[test]
    fn parses_markdown_template_field() {
        let src = r#"
name = "author-bio"
version = "0.1.0"

[[blocks]]
name = "lopress:author-bio"
markdown_template = "blocks/author-bio.md"

[blocks.attrs]
name    = { type = "string", ui = "text",     required = true,  label = "Author name" }
bio     = { type = "string", ui = "textarea",                 label = "Short bio",    help = "A short biography" }
spoiler = { type = "bool",   ui = "checkbox", default = false, label = "Mark as spoiler" }
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.markdown_template.as_deref(), Some("blocks/author-bio.md"));
        assert!(b.template.is_none());
        assert_eq!(b.attrs["name"].label.as_deref(), Some("Author name"));
        assert_eq!(b.attrs["bio"].label.as_deref(), Some("Short bio"));
        assert_eq!(b.attrs["bio"].help.as_deref(), Some("A short biography"));
        assert_eq!(b.attrs["bio"].ui.as_deref(), Some("textarea"));
    }

    #[test]
    fn errors_when_both_template_and_markdown_template_set() {
        let src = r#"
name = "bad"
version = "0.1.0"

[[blocks]]
name = "lopress:bad"
template = "blocks/bad.html"
markdown_template = "blocks/bad.md"
"#;
        let err = parse_manifest_str(src).unwrap_err();
        assert!(
            matches!(err, PluginError::MutualExclusion { field1, field2 } if field1 == "template" && field2 == "markdown_template")
        );
    }

    #[test]
    fn label_and_help_default_to_none() {
        let src = r#"
name = "minimal"
version = "0.1.0"

[[blocks]]
name = "lopress:minimal"
template = "blocks/minimal.html"

[blocks.attrs]
foo = { type = "string" }
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.blocks[0].attrs["foo"].label, None);
        assert_eq!(m.blocks[0].attrs["foo"].help, None);
    }

    #[test]
    fn markdown_template_defaults_to_none() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        assert!(m.blocks[0].markdown_template.is_none());
    }

    #[test]
    fn parses_title_description_category() {
        let src = r#"
name = "callout"
version = "0.1.0"

[[blocks]]
name              = "lopress:callout"
markdown_template = "blocks/callout.md"
title             = "Callout"
description       = "A highlighted note"
category          = "Text"
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert_eq!(b.title.as_deref(), Some("Callout"));
        assert_eq!(b.description.as_deref(), Some("A highlighted note"));
        assert_eq!(b.category.as_deref(), Some("Text"));
    }

    #[test]
    fn title_description_category_default_to_none() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert!(b.title.is_none());
        assert!(b.description.is_none());
        assert!(b.category.is_none());
    }

    #[test]
    fn parses_top_level_assets_table() {
        // Mirrors the real `code` asset-plugin manifest: block-less, declares
        // css + ordered js at the top level.
        let src = r#"
name = "code"
version = "0.1.0"

[assets]
css = ["assets/code.css"]
js  = ["assets/highlight.min.js", "assets/code.js"]
"#;
        let m = parse_manifest_str(src).unwrap();
        assert!(m.blocks.is_empty());
        assert_eq!(m.assets.css, vec!["assets/code.css".to_string()]);
        assert_eq!(
            m.assets.js,
            vec![
                "assets/highlight.min.js".to_string(),
                "assets/code.js".to_string()
            ]
        );
    }

    #[test]
    fn parses_block_plugin_with_top_level_assets() {
        // The real `series` plugin has BOTH a block and a top-level [assets]
        // table — both must survive parsing.
        let src = r#"
name = "series"
version = "0.1.0"

[[blocks]]
name = "lopress:series"
template = "blocks/series.html"

[assets]
css = ["assets/series.css"]
js  = ["assets/series.js"]
"#;
        let m = parse_manifest_str(src).unwrap();
        assert_eq!(m.blocks.len(), 1);
        assert_eq!(m.assets.css, vec!["assets/series.css".to_string()]);
        assert_eq!(m.assets.js, vec!["assets/series.js".to_string()]);
    }

    #[test]
    fn top_level_assets_default_to_empty() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"
"#;
        let m = parse_manifest_str(src).unwrap();
        assert!(m.assets.css.is_empty());
        assert!(m.assets.js.is_empty());
    }

    #[test]
    fn attr_decl_name_populated_from_toml_key() {
        let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }
"#;
        let m = parse_manifest_str(src).unwrap();
        let b = &m.blocks[0];
        assert_eq!(b.attrs["src"].name, "src");
        assert_eq!(b.attrs["autoplay"].name, "autoplay");
    }
}

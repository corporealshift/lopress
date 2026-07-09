//! Plugin asset injection.
//!
//! Plugins declare CSS/JS via the top-level `[assets]` table (and, for
//! forward-compat, per-block `css`/`js` fields). The build copies each
//! plugin's `assets/` directory to `www/assets/<plugin-name>/`; this module
//! turns the declared paths into `<link>`/`<script defer>` tags and injects
//! them into every rendered page. Only enabled plugins appear in the
//! registry, so injection is automatically scoped to enabled plugins.

use lopress_plugin::PluginRegistry;

/// Pre-rendered asset tag blocks for a build: `head` holds the `<link>`
/// stylesheet tags (injected before `</head>`), `body` holds the deferred
/// `<script>` tags (injected before `</body>`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetTags {
    pub head: String,
    pub body: String,
}

impl AssetTags {
    pub fn is_empty(&self) -> bool {
        self.head.is_empty() && self.body.is_empty()
    }
}

/// Map a declared asset path (relative to the plugin root, e.g.
/// `assets/code.css`) to its served web path (`/assets/<plugin>/code.css`).
/// The build copies `<plugin-root>/assets/` to `www/assets/<plugin>/`, so a
/// leading `assets/` segment is stripped before joining.
fn web_path(plugin_name: &str, declared: &str) -> String {
    let rel = declared.strip_prefix("assets/").unwrap_or(declared);
    format!("/assets/{plugin_name}/{rel}")
}

/// Collect a plugin's declared CSS (or JS) paths: the top-level `[assets]`
/// table unioned with every block's per-block field, order preserved,
/// deduplicated. `select` picks css vs js from each source.
fn collect<'a>(
    plugin: &'a lopress_plugin::LoadedPlugin,
    top: &'a [String],
    per_block: impl Fn(&'a lopress_plugin::BlockDecl) -> &'a [String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |paths: &[String], out: &mut Vec<String>| {
        for p in paths {
            if !out.iter().any(|e| e == p) {
                out.push(p.clone());
            }
        }
    };
    push(top, &mut out);
    for block in &plugin.manifest.blocks {
        push(per_block(block), &mut out);
    }
    out
}

/// Build the CSS/JS tag blocks for every enabled plugin in the registry.
/// Plugin order follows the registry; asset order within a plugin is
/// preserved (top-level first, then block declarations).
pub fn plugin_asset_tags(registry: &PluginRegistry) -> AssetTags {
    let mut head = String::new();
    let mut body = String::new();
    for plugin in &registry.plugins {
        let name = &plugin.manifest.name;
        for path in collect(plugin, &plugin.manifest.assets.css, |b| &b.css) {
            let web = web_path(name, &path);
            head.push_str(&format!("<link rel=\"stylesheet\" href=\"{web}\">\n"));
        }
        for path in collect(plugin, &plugin.manifest.assets.js, |b| &b.js) {
            let web = web_path(name, &path);
            body.push_str(&format!("<script defer src=\"{web}\"></script>\n"));
        }
    }
    AssetTags { head, body }
}

/// Inject the asset tags into a rendered page: CSS before the first
/// `</head>`, JS before the (last) `</body>`. If a marker is absent — a
/// malformed theme — the tags are appended so the assets still load.
pub fn inject(html: &str, tags: &AssetTags) -> String {
    let mut out = html.to_string();
    if !tags.head.is_empty() {
        out = insert_before(&out, "</head>", &tags.head);
    }
    if !tags.body.is_empty() {
        out = insert_before(&out, "</body>", &tags.body);
    }
    out
}

/// Insert `inject` immediately before the first occurrence of `marker`, or
/// append it when `marker` is absent. Avoids byte-slicing by rebuilding via
/// `replacen`.
fn insert_before(html: &str, marker: &str, inject: &str) -> String {
    match html.contains(marker) {
        true => html.replacen(marker, &format!("{inject}{marker}"), 1),
        false => format!("{html}{inject}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopress_plugin::{BlockDecl, LoadedPlugin, PluginAssets, PluginManifest, PluginRegistry};

    fn asset_plugin(name: &str, css: &[&str], js: &[&str]) -> LoadedPlugin {
        LoadedPlugin {
            root: std::path::PathBuf::from("/fake"),
            manifest: PluginManifest {
                name: name.to_string(),
                version: "0.1.0".into(),
                theme: false,
                blocks: Vec::new(),
                assets: PluginAssets {
                    css: css.iter().map(|s| s.to_string()).collect(),
                    js: js.iter().map(|s| s.to_string()).collect(),
                },
            },
        }
    }

    #[test]
    fn maps_top_level_assets_to_web_path_tags() {
        let mut reg = PluginRegistry::default();
        reg.insert(asset_plugin(
            "code",
            &["assets/code.css"],
            &["assets/highlight.min.js", "assets/code.js"],
        ))
        .unwrap();
        let tags = plugin_asset_tags(&reg);
        assert_eq!(
            tags.head,
            "<link rel=\"stylesheet\" href=\"/assets/code/code.css\">\n"
        );
        // Within-plugin JS order preserved: highlight before code.
        assert_eq!(
            tags.body,
            "<script defer src=\"/assets/code/highlight.min.js\"></script>\n\
             <script defer src=\"/assets/code/code.js\"></script>\n"
        );
    }

    #[test]
    fn empty_registry_yields_empty_tags() {
        let reg = PluginRegistry::default();
        let tags = plugin_asset_tags(&reg);
        assert!(tags.is_empty());
    }

    #[test]
    fn unions_per_block_css_with_top_level() {
        // A block plugin that ALSO declares a per-block css field: both the
        // top-level and the per-block asset must appear, deduplicated.
        let mut reg = PluginRegistry::default();
        let mut plugin = asset_plugin("series", &["assets/series.css"], &[]);
        plugin.manifest.blocks.push(BlockDecl {
            name: "lopress:series".into(),
            template: Some("blocks/series.html".into()),
            markdown_template: None,
            attrs: Default::default(),
            editor: None,
            builtin: false,
            native: None,
            css: vec!["assets/series.css".into(), "assets/extra.css".into()],
            js: Vec::new(),
            title: None,
            description: None,
            category: None,
        });
        reg.insert(plugin).unwrap();
        let tags = plugin_asset_tags(&reg);
        assert_eq!(
            tags.head,
            "<link rel=\"stylesheet\" href=\"/assets/series/series.css\">\n\
             <link rel=\"stylesheet\" href=\"/assets/series/extra.css\">\n"
        );
    }

    #[test]
    fn inject_places_tags_before_markers() {
        let tags = AssetTags {
            head: "<link href=\"a.css\">\n".into(),
            body: "<script src=\"a.js\"></script>\n".into(),
        };
        let html = "<html><head><title>t</title></head><body><p>hi</p></body></html>";
        let out = inject(html, &tags);
        assert_eq!(
            out,
            "<html><head><title>t</title><link href=\"a.css\">\n</head>\
             <body><p>hi</p><script src=\"a.js\"></script>\n</body></html>"
        );
    }

    #[test]
    fn inject_appends_when_markers_absent() {
        let tags = AssetTags {
            head: "H".into(),
            body: "B".into(),
        };
        let out = inject("<div>no markers</div>", &tags);
        assert_eq!(out, "<div>no markers</div>HB");
    }

    #[test]
    fn inject_with_empty_tags_is_identity() {
        let html = "<html><head></head><body></body></html>";
        assert_eq!(inject(html, &AssetTags::default()), html);
    }
}

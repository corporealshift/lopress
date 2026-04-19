use crate::error::BuildError;
use crate::site::Workspace;
use lopress_plugin::PluginRegistry;
use lopress_theme::ResolvedTheme;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCache {
    pub version: u32,
    #[serde(default)]
    pub config_hash: String,
    #[serde(default)]
    pub theme_hash: String,
    #[serde(default)]
    pub plugins_hash: String,
    #[serde(default)]
    pub pages: BTreeMap<String, PageEntry>,
}

impl Default for BuildCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            config_hash: String::new(),
            theme_hash: String::new(),
            plugins_hash: String::new(),
            pages: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageEntry {
    pub source_hash: String,
    pub outputs: Vec<String>, // workspace-relative, forward-slash
    pub tags: Vec<String>,
    pub is_draft: bool,
    pub title: Option<String>,
    pub date: Option<String>,
}

impl BuildCache {
    pub fn load(path: &Path) -> Result<Self, BuildError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        let parsed: Self = serde_json::from_str(&s)?;
        if parsed.version != CACHE_VERSION {
            return Ok(Self::default());
        }
        Ok(parsed)
    }

    pub fn save(&self, path: &Path) -> Result<(), BuildError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }
}

/// Workspace-relative, forward-slash path, for cache keys and output lists.
pub fn rel_key(workspace: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(workspace).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Hash a list of (relative-key, bytes) pairs, order-independent.
pub fn hash_many(items: &mut [(String, Vec<u8>)]) -> String {
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    for (k, v) in items.iter() {
        hasher.update(k.as_bytes());
        hasher.update(&[0]);
        hasher.update(v);
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

pub fn hash_config(workspace: &Workspace) -> Result<String, BuildError> {
    let bytes = std::fs::read(workspace.root.join("lopress.toml"))?;
    Ok(hash_bytes(&bytes))
}

/// Hash of every template in the resolved theme + the theme CSS.
/// For the built-in theme (`css_path` is `None`), we hash the embedded
/// templates in a stable order plus the CSS content.
pub fn hash_theme(theme: &ResolvedTheme) -> Result<String, BuildError> {
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(css_path) = &theme.css_path {
        let templates_dir = css_path.parent().unwrap().join("templates");
        if templates_dir.exists() {
            for entry in std::fs::read_dir(&templates_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("html") {
                    let name = entry
                        .path()
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    let bytes = std::fs::read(entry.path())?;
                    items.push((format!("tpl/{name}"), bytes));
                }
            }
        }
        items.push(("css".into(), std::fs::read(css_path)?));
    } else {
        for name in [
            "layout.html",
            "post.html",
            "page.html",
            "index.html",
            "tag.html",
            "404.html",
        ] {
            if let Some(src) = lopress_theme::builtin_template(name) {
                items.push((format!("tpl/{name}"), src.as_bytes().to_vec()));
            }
        }
        items.push(("css".into(), theme.css_content.as_bytes().to_vec()));
    }
    Ok(hash_many(&mut items))
}

pub fn hash_plugins(registry: &PluginRegistry) -> Result<String, BuildError> {
    let mut items: Vec<(String, Vec<u8>)> = Vec::new();
    for plugin in &registry.plugins {
        let name = &plugin.manifest.name;
        let manifest_bytes = std::fs::read(plugin.root.join("plugin.toml"))?;
        items.push((format!("{name}/plugin.toml"), manifest_bytes));
        for block in &plugin.manifest.blocks {
            let tpl_rel = &block.template;
            let tpl_bytes = std::fs::read(plugin.root.join(tpl_rel))?;
            items.push((format!("{name}/{tpl_rel}"), tpl_bytes));
        }
        let assets = plugin.root.join("assets");
        if assets.exists() {
            for entry in walkdir::WalkDir::new(&assets) {
                let entry = entry.map_err(std::io::Error::other)?;
                if entry.file_type().is_file() {
                    let rel = entry.path().strip_prefix(&assets).unwrap();
                    let key = format!(
                        "{name}/assets/{}",
                        rel.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    );
                    let bytes = std::fs::read(entry.path())?;
                    items.push((key, bytes));
                }
            }
        }
    }
    Ok(hash_many(&mut items))
}

pub fn hash_file(path: &Path) -> Result<String, BuildError> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_cache_is_version_1() {
        assert_eq!(BuildCache::default().version, 1);
    }

    #[test]
    fn roundtrip_via_json() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        let mut c = BuildCache {
            config_hash: "abc".into(),
            ..Default::default()
        };
        c.pages.insert(
            "src/posts/a.md".into(),
            PageEntry {
                source_hash: "h".into(),
                outputs: vec!["posts/a/index.html".into()],
                tags: vec!["x".into()],
                is_draft: false,
                title: Some("A".into()),
                date: None,
            },
        );
        c.save(&p).unwrap();
        let back = BuildCache::load(&p).unwrap();
        assert_eq!(back.config_hash, "abc");
        assert_eq!(back.pages.len(), 1);
    }

    #[test]
    fn version_mismatch_returns_default() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        std::fs::write(&p, r#"{"version":99,"pages":{}}"#).unwrap();
        let back = BuildCache::load(&p).unwrap();
        assert_eq!(back.version, CACHE_VERSION);
        assert!(back.pages.is_empty());
    }

    #[test]
    fn hash_many_is_order_independent() {
        let mut a = vec![("a".into(), b"1".to_vec()), ("b".into(), b"2".to_vec())];
        let mut b = vec![("b".into(), b"2".to_vec()), ("a".into(), b"1".to_vec())];
        assert_eq!(hash_many(&mut a), hash_many(&mut b));
    }

    #[test]
    fn hash_config_is_stable_and_changes_with_content() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        let ws = crate::site::Workspace::load(d.path()).unwrap();
        let h1 = hash_config(&ws).unwrap();
        let h2 = hash_config(&ws).unwrap();
        assert_eq!(h1, h2);

        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"B\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        let ws2 = crate::site::Workspace::load(d.path()).unwrap();
        assert_ne!(h1, hash_config(&ws2).unwrap());
    }
}

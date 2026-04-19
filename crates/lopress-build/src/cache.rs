use crate::error::BuildError;
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
}

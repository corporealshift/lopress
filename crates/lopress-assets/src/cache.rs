use crate::error::AssetError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Variant cache: key -> output filename (relative to www/images/).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VariantCache {
    pub entries: BTreeMap<String, String>,
}

impl VariantCache {
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), AssetError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Build a cache key from source hash, target width, and target format.
    pub fn key(source_hash: &str, width: u32, format: &str) -> String {
        format!("{source_hash}-{width}-{format}")
    }
}

/// Hash a file's contents with blake3 and return the hex string.
pub fn hash_file(path: &Path) -> Result<String, AssetError> {
    let bytes = std::fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Default variant for output filenames: `<stem>.<width>w.<ext>`
pub fn variant_filename(stem: &str, width: u32, ext: &str) -> PathBuf {
    PathBuf::from(format!("{stem}.{width}w.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cache_roundtrip_via_json() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        let mut c = VariantCache::default();
        c.entries.insert("abc-400-webp".into(), "foo.400w.webp".into());
        c.save(&p).unwrap();
        let loaded = VariantCache::load(&p).unwrap();
        assert_eq!(c.entries, loaded.entries);
    }

    #[test]
    fn hash_is_deterministic() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("x.bin");
        std::fs::write(&p, b"hello").unwrap();
        let h1 = hash_file(&p).unwrap();
        let h2 = hash_file(&p).unwrap();
        assert_eq!(h1, h2);
    }
}

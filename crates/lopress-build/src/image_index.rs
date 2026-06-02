//! A build-time index of processed images, so the renderer can emit a correct
//! responsive `srcset`. Keyed by the source file *stem* (filename without
//! extension), matching `lopress_assets::variant_filename`'s `{stem}.{w}w.{ext}`
//! naming and the `{stem}.{ext}` original copy.

use lopress_assets::ImageResult;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ImageVariant {
    pub width: u32,
    /// Filename relative to `www/images/`, e.g. `photo.800w.webp`.
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    /// The full-size original, relative to `www/images/`, e.g. `photo.jpg`.
    pub original: String,
    /// WebP variants, ascending by width.
    pub webp: Vec<ImageVariant>,
}

#[derive(Debug, Clone, Default)]
pub struct ImageIndex {
    by_stem: BTreeMap<String, ImageEntry>,
}

impl ImageIndex {
    pub fn get(&self, stem: &str) -> Option<&ImageEntry> {
        self.by_stem.get(stem)
    }

    /// Record the variants produced for `src` (the source image path).
    pub fn record(&mut self, src: &Path, result: &ImageResult) {
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();
        let ext = src
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_lowercase();
        let mut webp: Vec<ImageVariant> = result
            .files
            .iter()
            .filter(|v| v.format == "webp")
            .map(|v| ImageVariant {
                width: v.width,
                filename: v.filename.to_string_lossy().into_owned(),
            })
            .collect();
        webp.sort_by_key(|v| v.width);
        self.by_stem.insert(
            stem.clone(),
            ImageEntry {
                original: format!("{stem}.{ext}"),
                webp,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopress_assets::Variant;
    use std::path::PathBuf;

    #[test]
    fn records_and_sorts_webp_variants() {
        let mut idx = ImageIndex::default();
        let result = ImageResult {
            files: vec![
                Variant {
                    filename: PathBuf::from("photo.800w.webp"),
                    width: 800,
                    format: "webp".into(),
                },
                Variant {
                    filename: PathBuf::from("photo.400w.webp"),
                    width: 400,
                    format: "webp".into(),
                },
                Variant {
                    filename: PathBuf::from("photo.800w.jpg"),
                    width: 800,
                    format: "jpg".into(),
                },
            ],
        };
        idx.record(Path::new("/src/images/photo.jpg"), &result);
        let entry = idx.get("photo").expect("entry");
        assert_eq!(entry.original, "photo.jpg");
        assert_eq!(entry.webp.len(), 2, "only webp variants");
        assert_eq!(entry.webp[0].width, 400, "ascending by width");
        assert_eq!(entry.webp[1].width, 800);
    }
}

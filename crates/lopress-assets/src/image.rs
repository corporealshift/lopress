use crate::cache::{hash_file, variant_filename, VariantCache};
use crate::error::AssetError;
use image::{ImageReader, DynamicImage, ImageFormat};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct VariantSpec {
    pub widths: Vec<u32>,
    pub webp: bool,
    pub keep_original_format: bool,
}

impl Default for VariantSpec {
    fn default() -> Self {
        Self {
            widths: vec![400, 800, 1600],
            webp: true,
            keep_original_format: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageResult {
    /// Output files, relative to `www/images/`.
    pub files: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub filename: PathBuf,
    pub width: u32,
    pub format: String, // "webp" or original extension
}

/// Generate all variants of `src` into `www_images_dir`, consulting and updating
/// `cache`. Returns the list of variant filenames (not re-encoding cached ones).
pub fn process_image(
    src: &Path,
    www_images_dir: &Path,
    cache: &mut VariantCache,
    spec: &VariantSpec,
) -> Result<ImageResult, AssetError> {
    std::fs::create_dir_all(www_images_dir)?;

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
    let hash = hash_file(src)?;

    // Always copy the original through.
    let original_out = www_images_dir.join(format!("{stem}.{ext}"));
    if !original_out.exists() {
        std::fs::copy(src, &original_out)?;
    }

    let img = ImageReader::open(src)?.with_guessed_format()?.decode()?;
    let mut files = Vec::new();

    for &w in &spec.widths {
        // Upscaling is useless; skip variants wider than original.
        if w >= img.width() {
            continue;
        }
        if spec.webp {
            let key = VariantCache::key(&hash, w, "webp");
            let filename = variant_filename(&stem, w, "webp");
            let out_path = www_images_dir.join(&filename);
            if !cache.entries.contains_key(&key) || !out_path.exists() {
                let resized = img.thumbnail(w, u32::MAX);
                write_webp(&resized, &out_path)?;
                cache.entries.insert(key, filename.to_string_lossy().into());
            }
            files.push(Variant {
                filename,
                width: w,
                format: "webp".into(),
            });
        }
        if spec.keep_original_format {
            let key = VariantCache::key(&hash, w, &ext);
            let filename = variant_filename(&stem, w, &ext);
            let out_path = www_images_dir.join(&filename);
            if !cache.entries.contains_key(&key) || !out_path.exists() {
                let resized = img.thumbnail(w, u32::MAX);
                let format = ImageFormat::from_extension(&ext).unwrap_or(ImageFormat::Jpeg);
                resized.save_with_format(&out_path, format)?;
                cache.entries.insert(key, filename.to_string_lossy().into());
            }
            files.push(Variant {
                filename,
                width: w,
                format: ext.clone(),
            });
        }
    }

    Ok(ImageResult { files })
}

fn write_webp(img: &DynamicImage, out: &Path) -> Result<(), AssetError> {
    let rgba = img.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let encoded = encoder.encode(80.0);
    std::fs::write(out, encoded.to_vec())
        .map_err(AssetError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use tempfile::TempDir;

    fn make_image(path: &Path, w: u32, h: u32) {
        let mut img = RgbImage::new(w, h);
        for p in img.pixels_mut() { *p = Rgb([200, 100, 50]); }
        img.save(path).unwrap();
    }

    #[test]
    fn produces_expected_variants_and_caches_them() {
        let d = TempDir::new().unwrap();
        let src = d.path().join("src.jpg");
        make_image(&src, 2000, 1500);

        let www = d.path().join("www/images");
        let mut cache = VariantCache::default();
        let spec = VariantSpec::default();

        let r1 = process_image(&src, &www, &mut cache, &spec).unwrap();
        // 3 widths * (webp + original) = 6 variants.
        assert_eq!(r1.files.len(), 6);
        let before = cache.entries.len();

        // Re-run: everything cached, no new entries.
        let _r2 = process_image(&src, &www, &mut cache, &spec).unwrap();
        assert_eq!(cache.entries.len(), before);
    }

    #[test]
    fn skips_widths_wider_than_source() {
        let d = TempDir::new().unwrap();
        let src = d.path().join("small.jpg");
        make_image(&src, 500, 400); // smaller than 800 and 1600
        let www = d.path().join("www/images");
        let mut cache = VariantCache::default();
        let spec = VariantSpec::default();
        let r = process_image(&src, &www, &mut cache, &spec).unwrap();
        // Only the 400 width is narrower; expect 2 variants (webp + jpg).
        assert_eq!(r.files.len(), 2);
    }
}

//! The app's window icon, embedded from `assets/icon/lopress-256.png`.
//!
//! The exe's own icon resource is embedded separately by the root `build.rs`;
//! this one covers the live window (and platforms with no exe icon).

use floem::window::Icon;
use image::RgbaImage;

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon/lopress-256.png");

fn decode() -> Result<RgbaImage, image::ImageError> {
    Ok(image::load_from_memory(ICON_PNG)?.to_rgba8())
}

/// The window icon, or `None` (with a warning) if it fails to build. A missing
/// icon must never block startup.
pub(crate) fn window_icon() -> Option<Icon> {
    let img = match decode() {
        Ok(img) => img,
        Err(e) => {
            eprintln!("failed to decode app icon: {e}");
            return None;
        }
    };
    let (width, height) = img.dimensions();
    match Icon::from_rgba(img.into_raw(), width, height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!("failed to build app icon: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes_to_256_square() {
        assert_eq!(decode().unwrap().dimensions(), (256, 256));
    }

    #[test]
    fn window_icon_builds() {
        assert!(window_icon().is_some());
    }
}

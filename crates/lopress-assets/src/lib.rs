#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
    )
)]

pub mod cache;
pub mod error;
pub mod image;

pub use cache::{hash_file, VariantCache};
pub use error::AssetError;
pub use image::{process_image, ImageResult, Variant, VariantSpec};

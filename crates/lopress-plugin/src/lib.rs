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

pub mod error;
pub mod loader;
pub mod manifest;
pub mod registry;

pub use error::PluginError;
pub use loader::load_dir;
pub use manifest::{
    parse_manifest, parse_manifest_str, AttrDecl, AttrType, BlockDecl, PluginAssets, PluginManifest,
};
pub use registry::{LoadedPlugin, PluginRegistry};

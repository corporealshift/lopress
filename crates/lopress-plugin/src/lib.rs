#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc,
    )
)]

pub mod error;
pub mod loader;
pub mod manifest;
pub mod registry;

pub use error::PluginError;
pub use loader::load_dir;
pub use manifest::{
    parse_manifest, parse_manifest_str, AttrDecl, AttrType, BlockDecl, PluginManifest,
};
pub use registry::{LoadedPlugin, PluginRegistry};

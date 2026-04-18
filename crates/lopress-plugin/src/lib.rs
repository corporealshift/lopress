pub mod error;
pub mod loader;
pub mod manifest;
pub mod registry;

pub use error::PluginError;
pub use loader::load_dir;
pub use manifest::{AttrDecl, AttrType, BlockDecl, PluginManifest};
pub use registry::{LoadedPlugin, PluginRegistry};

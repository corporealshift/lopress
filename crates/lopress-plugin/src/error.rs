use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("manifest error at {path}: {message}")]
    Manifest { path: PathBuf, message: String },
    #[error("plugin `{name}` declares template `{template}` but file does not exist")]
    MissingTemplate { name: String, template: String },
    #[error("duplicate block name `{0}` across plugins")]
    DuplicateBlock(String),
    #[error("duplicate native claim `{0}` — two plugins claim the same core type")]
    DuplicateNative(String),
    #[error("`{field1}` and `{field2}` are mutually exclusive on the same block")]
    MutualExclusion { field1: String, field2: String },
}

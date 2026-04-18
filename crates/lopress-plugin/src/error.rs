use thiserror::Error;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("I/O error at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("manifest error at {path}: {message}")]
    Manifest { path: PathBuf, message: String },
    #[error("plugin `{name}` declares template `{template}` but file does not exist")]
    MissingTemplate { name: String, template: String },
    #[error("duplicate block name `{0}` across plugins")]
    DuplicateBlock(String),
}

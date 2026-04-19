use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("site config: {0}")]
    Config(String),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("core parse: {0}")]
    Parse(#[from] lopress_core::ParseError),
    #[error("plugin: {0}")]
    Plugin(#[from] lopress_plugin::PluginError),
    #[error("theme: {0}")]
    Theme(#[from] lopress_theme::ThemeError),
    #[error("assets: {0}")]
    Assets(#[from] lopress_assets::AssetError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("xml: {0}")]
    Xml(String),
    #[error("one or more pages failed to build: {0:?}")]
    PartialFailure(Vec<PageFailure>),
}

#[derive(Debug, Clone)]
pub struct PageFailure {
    pub path: std::path::PathBuf,
    pub message: String,
}

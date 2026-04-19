use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("template error: {0}")]
    Tera(#[from] tera::Error),
    #[error("missing template `{0}`")]
    MissingTemplate(String),
    #[error("theme `{0}` not found")]
    NotFound(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

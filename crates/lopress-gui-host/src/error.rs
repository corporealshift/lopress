use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenError {
    #[error("invalid workspace: {0}")]
    InvalidWorkspace(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error at line {line}: {message}")]
    Parse {
        raw: String,
        line: u32,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("build: {0}")]
    Build(#[from] lopress_build::BuildError),
}

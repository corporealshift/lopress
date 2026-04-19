use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("front-matter error: {0}")]
    FrontMatter(String),

    #[error("invalid YAML in front-matter: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("invalid JSON in block attrs at line {line}: {message}")]
    BlockAttrs { line: usize, message: String },

    #[error("unterminated block `{block_type}` opened at line {line}")]
    UnterminatedBlock { block_type: String, line: usize },

    #[error("mismatched block close: expected `{expected}`, got `{actual}` at line {line}")]
    MismatchedClose {
        expected: String,
        actual: String,
        line: usize,
    },
}

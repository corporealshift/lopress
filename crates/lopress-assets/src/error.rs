use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("image decode error: {0}")]
    Decode(#[from] image::ImageError),
    #[error("webp encode error: {0}")]
    Webp(String),
    #[error("json cache error: {0}")]
    Json(#[from] serde_json::Error),
}

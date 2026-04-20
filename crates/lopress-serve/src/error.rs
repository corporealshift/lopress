use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("build: {0}")]
    Build(#[from] lopress_build::BuildError),
    #[error("watch: {0}")]
    Watch(#[from] lopress_watch::WatchError),
    #[error("bind {addr}: {source}")]
    Bind {
        addr: String,
        #[source]
        source: std::io::Error,
    },
}

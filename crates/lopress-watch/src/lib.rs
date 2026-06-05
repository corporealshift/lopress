#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
    )
)]

pub mod classify;
pub mod error;
pub mod watcher;

pub use classify::{classify, Bucket};
pub use error::WatchError;
pub use watcher::{ChangeSet, Watcher};

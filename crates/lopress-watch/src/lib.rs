pub mod classify;
pub mod error;
pub mod watcher;

pub use classify::{classify, Bucket};
pub use error::WatchError;
pub use watcher::{ChangeSet, Watcher};

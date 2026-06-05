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

pub mod document;
pub mod error;
pub mod session;

pub use document::LoadedDocument;
pub use error::{LoadError, OpenError, SaveError};
pub use session::{BuildStatus, DocumentRef, ServeStatus, Session, WorkspaceSummary};

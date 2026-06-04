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

pub mod error;
pub mod http;
pub mod inject;
pub mod mime;
pub mod router;
pub mod server;
pub mod sse;

pub use error::ServeError;
pub use server::{serve, serve_in_background, ServeOptions, ServerHandle};

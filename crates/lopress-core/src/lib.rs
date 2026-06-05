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

pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod parser;
pub mod perf;
pub mod serializer;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use parser::{parse, render_markdown};
pub use serializer::serialize;
pub use types::{Block, Document, FrontMatter};

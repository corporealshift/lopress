#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::missing_panics_doc,
        clippy::missing_errors_doc,
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
pub use parser::parse;
pub use serializer::serialize;
pub use types::{Block, Document, FrontMatter};

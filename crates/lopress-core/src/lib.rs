pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod parser;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use parser::parse;
pub use types::{Block, Document, FrontMatter};

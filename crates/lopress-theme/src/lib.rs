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

pub mod builtin;
pub mod context;
pub mod engine;
pub mod error;
pub mod resolver;

pub use builtin::{builtin_template, default_css, default_engine};
pub use context::{NavItem, PageCtx, PageKind, PostSummary, RenderContext, SiteCtx};
pub use engine::ThemeEngine;
pub use error::ThemeError;
pub use resolver::{resolve, ResolvedTheme};

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

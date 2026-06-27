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

pub mod build;
pub mod cache;
pub mod cli;
pub mod error;
pub mod feed;
pub mod image_index;
pub mod not_found;
pub mod pages;
pub mod render;
pub mod robots;
pub mod scaffold;
pub mod site;
pub mod sitemap;

pub use build::{build, BuildReport};
pub use cache::{BuildCache, PageEntry};
pub use error::{BuildError, PageFailure};
pub use image_index::ImageIndex;
pub use pages::{discover, post_summaries, render_all, DiscoveredPost, RenderStats};
pub use render::render_body;
pub use site::{write_nav, Nav, NavItem, SiteConfig, Workspace};

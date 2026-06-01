//! `lopress new` site scaffolding.
//!
//! Writes a minimal-but-buildable site: `lopress.toml`, the `src/` subtree, a
//! sample post and page, and a `.gitignore`. Refuses to write into a directory
//! that already exists and is non-empty so an existing site is never clobbered.

use crate::error::BuildError;
use std::path::Path;

/// Scaffold a fresh lopress site at `dir` with the given `title` / `base_url`.
///
/// # Errors
/// - [`BuildError::Config`] if `dir` already exists and is non-empty.
/// - [`BuildError::Io`] if any file or directory write fails.
pub fn new_site(dir: &Path, title: &str, base_url: &str) -> Result<(), BuildError> {
    if dir.exists() {
        if std::fs::read_dir(dir)?.next().is_some() {
            return Err(BuildError::Config(format!(
                "refusing to scaffold into non-empty directory {}",
                dir.display()
            )));
        }
    } else {
        std::fs::create_dir_all(dir)?;
    }

    std::fs::write(
        dir.join("lopress.toml"),
        format!(
            r#"[site]
title = "{title}"
base_url = "{base_url}"

[site.nav]
items = [
  {{ label = "Home", href = "/" }},
  {{ label = "About", href = "/about/" }},
]
"#
        ),
    )?;

    for sub in ["src/posts", "src/pages", "src/images", "plugins"] {
        std::fs::create_dir_all(dir.join(sub))?;
    }

    let date = chrono::Utc::now().format("%Y-%m-%d");
    std::fs::write(
        dir.join("src/posts/hello.md"),
        format!(
            "---\ntitle: Hello\ndate: {date}\ntags: [intro]\n---\n\n# Hello\n\nWelcome to your new lopress site.\n"
        ),
    )?;
    std::fs::write(
        dir.join("src/pages/about.md"),
        "---\ntitle: About\n---\n\n# About\n\nThis is the about page.\n",
    )?;
    std::fs::write(dir.join(".gitignore"), "/www\n/.lopress-cache.json\n")?;

    Ok(())
}

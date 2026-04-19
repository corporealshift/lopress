use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "lopress",
    version,
    about = "A personal blog authoring tool with static site generation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a workspace into `www/`.
    Build {
        /// Workspace directory (contains `lopress.toml`).
        workspace: PathBuf,
    },
    /// Scaffold a new workspace.
    New {
        /// Destination directory. Must not exist, or must be empty.
        dir: PathBuf,
        #[arg(long, default_value = "Untitled")]
        title: String,
        #[arg(long, default_value = "https://example.com")]
        base_url: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { workspace } => {
            let report = lopress_build::build(&workspace)?;
            println!(
                "built {} page(s); {} failure(s)",
                report.pages_written,
                report.failures.len()
            );
            for f in &report.failures {
                eprintln!("  FAIL {}: {}", f.path.display(), f.message);
            }
            if !report.failures.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::New {
            dir,
            title,
            base_url,
        } => scaffold::new_site(&dir, &title, &base_url),
    }
}

mod scaffold {
    use anyhow::{bail, Result};
    use std::path::Path;

    pub fn new_site(dir: &Path, title: &str, base_url: &str) -> Result<()> {
        if dir.exists() {
            let non_empty = std::fs::read_dir(dir)?.next().is_some();
            if non_empty {
                bail!("target directory `{}` is not empty", dir.display());
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
        for d in ["src/posts", "src/pages", "src/images", "plugins"] {
            std::fs::create_dir_all(dir.join(d))?;
        }
        std::fs::write(
            dir.join("src/posts/hello.md"),
            r#"---
title: Hello
date: 2026-04-18
tags: [intro]
---

# Hello

Welcome to your new lopress site.
"#,
        )?;
        std::fs::write(
            dir.join("src/pages/about.md"),
            r#"---
title: About
---

# About

This is the about page.
"#,
        )?;
        std::fs::write(dir.join(".gitignore"), "/www\n/.lopress-cache.json\n")?;
        println!("created workspace at {}", dir.display());
        Ok(())
    }
}

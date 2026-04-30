// In release builds on Windows, suppress the console window so the app
// behaves as a proper GUI application when launched from File Explorer.
// Debug builds keep the console so CLI subcommands show output during dev.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Route: no args → GUI welcome; known subcommand → CLI; else treat as path → GUI
    match args.first().map(String::as_str) {
        None => launch_gui(None),
        Some("build") => cli_build(args.get(1..).unwrap_or(&[])),
        Some("new") => cli_new(args.get(1..).unwrap_or(&[])),
        Some("serve") => cli_serve(args.get(1..).unwrap_or(&[])),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        Some(path) => launch_gui(Some(PathBuf::from(path))),
    }
}

fn launch_gui(workspace: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("lopress")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "lopress",
        options,
        Box::new(move |_cc| Ok(Box::new(lopress_editor::LopressApp::new(workspace.clone())))),
    ) {
        let msg = format!("Failed to start GUI: {e}");
        // In release builds on Windows the console is suppressed, so show a
        // native dialog instead of writing to stderr.
        #[cfg(all(target_os = "windows", not(debug_assertions)))]
        rfd::MessageDialog::new()
            .set_title("lopress — startup error")
            .set_description(&msg)
            .set_level(rfd::MessageLevel::Error)
            .show();
        return Err(anyhow::anyhow!("{msg}"));
    }
    Ok(ExitCode::SUCCESS)
}

fn cli_build(args: &[String]) -> anyhow::Result<ExitCode> {
    let workspace = args
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("usage: lopress build <workspace>"))?;
    let report = lopress_build::build(&workspace)?;
    println!(
        "built {} page(s); {} failure(s)",
        report.pages_written,
        report.failures.len()
    );
    for f in &report.failures {
        eprintln!("  FAIL {}: {}", f.path.display(), f.message);
    }
    Ok(if report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn cli_new(args: &[String]) -> anyhow::Result<ExitCode> {
    let mut dir = None::<PathBuf>;
    let mut title = "Untitled".to_string();
    let mut base_url = "https://example.com".to_string();
    let mut i = 0;
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "--title" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    title = v.clone();
                }
            }
            "--base-url" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    base_url = v.clone();
                }
            }
            p => {
                dir = Some(PathBuf::from(p));
            }
        }
        i += 1;
    }
    let dir = dir.ok_or_else(|| anyhow::anyhow!("usage: lopress new <dir>"))?;
    scaffold::new_site(&dir, &title, &base_url)?;
    Ok(ExitCode::SUCCESS)
}

fn cli_serve(args: &[String]) -> anyhow::Result<ExitCode> {
    let mut workspace = None::<PathBuf>;
    let mut bind = "127.0.0.1".to_string();
    let mut port: u16 = 8080;
    let mut no_open = false;
    let mut i = 0;
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "--bind" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    bind = v.clone();
                }
            }
            "--port" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    port = v.parse().unwrap_or(8080);
                }
            }
            "--no-open" => {
                no_open = true;
            }
            p => {
                workspace = Some(PathBuf::from(p));
            }
        }
        i += 1;
    }
    let workspace = workspace.ok_or_else(|| anyhow::anyhow!("usage: lopress serve <workspace>"))?;
    lopress_serve::serve(lopress_serve::ServeOptions {
        workspace,
        bind,
        port,
        open_browser: !no_open,
        on_ready: None,
    })?;
    Ok(ExitCode::SUCCESS)
}

fn print_help() {
    println!("lopress — personal blog authoring tool\n");
    println!("USAGE:");
    println!("  lopress                  Open the GUI (welcome screen)");
    println!("  lopress <path>           Open the GUI with a workspace");
    println!("  lopress build <ws>       Build a workspace");
    println!("  lopress new <dir>        Scaffold a new workspace");
    println!("  lopress serve <ws>       Dev server with live reload");
}

mod scaffold {
    use anyhow::{bail, Result};
    use std::path::Path;

    pub(crate) fn new_site(dir: &Path, title: &str, base_url: &str) -> Result<()> {
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
            "---\ntitle: Hello\ndate: 2026-04-18\ntags: [intro]\n---\n\n# Hello\n\nWelcome to your new lopress site.\n",
        )?;
        std::fs::write(
            dir.join("src/pages/about.md"),
            "---\ntitle: About\n---\n\n# About\n\nThis is the about page.\n",
        )?;
        std::fs::write(dir.join(".gitignore"), "/www\n/.lopress-cache.json\n")?;
        println!("created workspace at {}", dir.display());
        Ok(())
    }
}

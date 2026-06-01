//! Hand-rolled argument parsing for the `lopress` binary.
//!
//! Deliberately dependency-free (no clap): the binary is GUI-first, and the
//! only subcommand is `new`. `parse` turns the raw args (everything after the
//! program name) into a [`Command`] the binary's `main` dispatches on. No
//! recognized subcommand → launch the GUI, exactly as before the CLI existed.

use std::path::PathBuf;

/// Usage text shown for `--help`-less misuse.
pub const USAGE: &str = "\
usage:
  lopress                       launch the editor GUI
  lopress new <dir> [options]   scaffold a new site in <dir>

new options:
  --title <TITLE>       site title (default: the directory name)
  --base-url <URL>      site base URL (default: http://localhost:8080)";

/// What the binary should do, decided purely from the arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Launch the editor GUI (no subcommand given).
    Gui,
    /// Scaffold a new site at `dir`.
    NewSite {
        dir: PathBuf,
        title: String,
        base_url: String,
    },
    /// Arguments were malformed; `main` prints this and exits non-zero.
    Usage(String),
}

/// Parse the arguments following the program name into a [`Command`].
///
/// The binary is GUI-first: only the `new` subcommand diverts from launching
/// the editor. No args, an unknown token, or a workspace path all fall through
/// to [`Command::Gui`] — so `lopress` and `lopress <workspace>` both open the
/// editor, exactly as before the CLI existed.
pub fn parse(args: &[String]) -> Command {
    match args.first().map(String::as_str) {
        Some("new") => parse_new(args.get(1..).unwrap_or(&[])),
        _ => Command::Gui,
    }
}

fn parse_new(args: &[String]) -> Command {
    let mut dir: Option<PathBuf> = None;
    let mut title: Option<String> = None;
    let mut base_url: Option<String> = None;

    let mut i = 0;
    while let Some(arg) = args.get(i) {
        match arg.as_str() {
            "--title" => match args.get(i + 1) {
                Some(v) => {
                    title = Some(v.clone());
                    i += 1;
                }
                None => return usage("--title requires a value"),
            },
            "--base-url" => match args.get(i + 1) {
                Some(v) => {
                    base_url = Some(v.clone());
                    i += 1;
                }
                None => return usage("--base-url requires a value"),
            },
            flag if flag.starts_with("--") => {
                return usage(&format!("unknown flag '{flag}'"));
            }
            positional => {
                if dir.is_some() {
                    return usage(&format!("unexpected argument '{positional}'"));
                }
                dir = Some(PathBuf::from(positional));
            }
        }
        i += 1;
    }

    let Some(dir) = dir else {
        return usage("new requires a target directory");
    };
    let title = title.unwrap_or_else(|| {
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });
    let base_url = base_url.unwrap_or_else(|| "http://localhost:8080".to_string());
    Command::NewSite {
        dir,
        title,
        base_url,
    }
}

fn usage(msg: &str) -> Command {
    Command::Usage(format!("lopress new: {msg}\n\n{USAGE}"))
}

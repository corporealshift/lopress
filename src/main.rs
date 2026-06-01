#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use lopress_build::cli::{parse, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Command::Gui => run_gui(),
        Command::NewSite {
            dir,
            title,
            base_url,
        } => match lopress_build::scaffold::new_site(&dir, &title, &base_url) {
            Ok(()) => {
                println!("Created lopress site at {}", dir.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("lopress new: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Usage(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn run_gui() -> ExitCode {
    match lopress_editor::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lopress: {e}");
            ExitCode::FAILURE
        }
    }
}

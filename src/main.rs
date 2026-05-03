#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    match lopress_editor::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lopress: {e}");
            ExitCode::FAILURE
        }
    }
}

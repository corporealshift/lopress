use crate::error::ServeError;
use std::path::PathBuf;

pub struct ServeOptions {
    pub workspace: PathBuf,
    pub bind: String,
    pub port: u16,
    pub open_browser: bool,
}

pub fn serve(_opts: ServeOptions) -> Result<(), ServeError> {
    unimplemented!("filled in by Task 13")
}

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECENTS: usize = 5;

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentsFile {
    paths: Vec<PathBuf>,
}

fn recents_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "lopress").map(|p| p.config_dir().join("recents.json"))
}

/// Load the recent workspaces list. Returns an empty vec on any error.
pub fn load() -> Vec<PathBuf> {
    let Some(path) = recents_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<RecentsFile>(&bytes) else {
        return Vec::new();
    };
    file.paths.into_iter().filter(|p| p.exists()).collect()
}

/// Prepend `workspace` to the recents list and persist.
pub fn push(workspace: &Path) {
    let Some(path) = recents_path() else {
        return;
    };
    let mut paths = load();
    paths.retain(|p| p != workspace);
    paths.insert(0, workspace.to_path_buf());
    paths.truncate(MAX_RECENTS);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(bytes) = serde_json::to_vec(&RecentsFile { paths }) {
        std::fs::write(&path, bytes).ok();
    }
}

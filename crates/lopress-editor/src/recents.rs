use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Canonicalize each path and return de-duplicated results in original order
/// (first occurrence wins). Falls back to the raw path when `canonicalize`
/// fails (e.g., the workspace was deleted or unmounted), so legitimate
/// recents don't disappear silently.
pub(crate) fn dedup_canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .fold(Vec::new(), |mut acc, p| {
            if !acc.contains(&p) {
                acc.push(p);
            }
            acc
        })
}

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
    let mut paths = dedup_canonical(&paths);
    paths.truncate(MAX_RECENTS);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(bytes) = serde_json::to_vec(&RecentsFile { paths }) {
        std::fs::write(&path, bytes).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::dedup_canonical;
    use std::path::PathBuf;

    #[test]
    fn dedup_removes_exact_duplicates() {
        let paths = vec![
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/b"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped.first(), Some(&PathBuf::from("/nonexistent/a")));
        assert_eq!(deduped.get(1), Some(&PathBuf::from("/nonexistent/b")));
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        let paths = vec![
            PathBuf::from("/nonexistent/b"),
            PathBuf::from("/nonexistent/a"),
            PathBuf::from("/nonexistent/b"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(
            deduped,
            vec![
                PathBuf::from("/nonexistent/b"),
                PathBuf::from("/nonexistent/a"),
            ]
        );
    }

    #[test]
    fn dedup_falls_back_to_raw_when_canonicalize_fails() {
        // Canonicalize on nonexistent paths returns Err; the fallback keeps
        // the raw paths so they still appear (and still dedup).
        let paths = vec![
            PathBuf::from("/nonexistent/x"),
            PathBuf::from("/nonexistent/x"),
        ];
        let deduped = dedup_canonical(&paths);
        assert_eq!(deduped.len(), 1);
    }
}

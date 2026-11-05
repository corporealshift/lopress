//! Per-document notes files.
//!
//! `src/posts/my-idea.md` pairs with `notes/posts/my-idea.md`. Notes live
//! outside `src/` so neither the build nor the watcher sees them.

use crate::session::atomic_write;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// The notes file for `doc`, or `None` when `doc` is not directly inside a
/// `posts` or `pages` folder.
///
/// Compares the parent folder's name rather than stripping a full prefix, so
/// paths that arrive in a different form (debug `/open`) still map.
pub fn notes_path(root: &Path, doc: &Path) -> Option<PathBuf> {
    let folder = doc.parent()?.file_name()?.to_str()?;
    if !matches!(folder, "posts" | "pages") {
        return None;
    }
    Some(root.join("notes").join(folder).join(doc.file_name()?))
}

/// Read notes. A missing file is empty notes, not an error.
///
/// # Errors
/// Returns the I/O error for anything other than a missing file.
pub fn load(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Write notes, creating folders as needed. Text that is empty or only
/// whitespace deletes the file, so an empty note leaves nothing on disk.
///
/// # Errors
/// Returns the underlying I/O error.
pub fn save(path: &Path, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, text.as_bytes())
}

/// Move a notes file. A missing source is a no-op.
///
/// # Errors
/// Returns `AlreadyExists` rather than overwriting `to` (`std::fs::rename`
/// replaces its target on Windows), or the underlying I/O error.
pub fn rename(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    std::fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn notes_path_maps_a_post() {
        let got = notes_path(Path::new("/site"), Path::new("/site/src/posts/my-idea.md"));
        assert_eq!(got, Some(PathBuf::from("/site/notes/posts/my-idea.md")));
    }

    #[test]
    fn notes_path_maps_a_page() {
        let got = notes_path(Path::new("/site"), Path::new("/site/src/pages/about.md"));
        assert_eq!(got, Some(PathBuf::from("/site/notes/pages/about.md")));
    }

    #[test]
    fn notes_path_rejects_other_folders() {
        let got = notes_path(Path::new("/site"), Path::new("/site/src/other/x.md"));
        assert_eq!(got, None);
    }

    #[test]
    fn load_of_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("nope.md")).unwrap(), "");
    }

    #[test]
    fn save_creates_folders_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes/posts/a.md");
        save(&path, "an idea\n").unwrap();
        assert_eq!(load(&path).unwrap(), "an idea\n");
    }

    #[test]
    fn save_of_blank_text_deletes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        for blank in ["", "  \n"] {
            fs::write(&path, "old").unwrap();
            save(&path, blank).unwrap();
            assert!(!path.exists(), "{blank:?} should delete the file");
        }
    }

    #[test]
    fn save_of_blank_text_with_no_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes/posts/a.md");
        save(&path, "").unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn rename_moves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("old.md");
        let to = dir.path().join("new.md");
        fs::write(&from, "notes").unwrap();
        rename(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "notes");
    }

    #[test]
    fn rename_of_a_missing_source_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("new.md");
        rename(&dir.path().join("old.md"), &to).unwrap();
        assert!(!to.exists());
    }

    #[test]
    fn rename_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("old.md");
        let to = dir.path().join("new.md");
        fs::write(&from, "mine").unwrap();
        fs::write(&to, "leftover").unwrap();
        let err = rename(&from, &to).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&from).unwrap(), "mine");
        assert_eq!(fs::read_to_string(&to).unwrap(), "leftover");
    }
}

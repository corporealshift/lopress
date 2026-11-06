//! Noticing outside changes to the open document and its notes.
//!
//! The editor keeps a [`DiskBaseline`]: the text it last read or wrote.
//! Comparing content rather than timestamps means its own saves never look
//! like outside changes. [`DiskStamp`] is the cheap check that decides when a
//! comparison is worth doing.

use crate::notes;
use std::io::{ErrorKind, Result};
use std::path::Path;
use std::time::SystemTime;

/// How the files on disk relate to the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskState {
    /// The post and notes match the baseline.
    Unchanged,
    /// The post or notes differ from the baseline.
    Changed,
    /// The post file is gone. Missing notes read as empty, so deleted notes
    /// are `Changed`, not `Missing`.
    Missing,
}

/// The text of the post and notes as last read from or written to disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskBaseline {
    pub post: String,
    pub notes: String,
}

/// Compare the files on disk with `baseline`. `notes` is `None` for a document
/// that can't have notes.
///
/// # Errors
/// Returns the I/O error for anything other than a missing file.
pub fn compare(post: &Path, notes: Option<&Path>, baseline: &DiskBaseline) -> Result<DiskState> {
    let on_disk = match std::fs::read_to_string(post) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(DiskState::Missing),
        Err(e) => return Err(e),
    };
    let notes_on_disk = match notes {
        Some(path) => notes::load(path)?,
        None => String::new(),
    };
    if on_disk == baseline.post && same_notes(&notes_on_disk, &baseline.notes) {
        Ok(DiskState::Unchanged)
    } else {
        Ok(DiskState::Changed)
    }
}

/// Saving blank notes deletes the file, so all blank text is the same notes.
fn same_notes(a: &str, b: &str) -> bool {
    a == b || (a.trim().is_empty() && b.trim().is_empty())
}

/// Modified time and size of the post and notes files. A file that can't be
/// read stamps as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskStamp {
    post: Option<(SystemTime, u64)>,
    notes: Option<(SystemTime, u64)>,
}

impl DiskStamp {
    /// Stamp the files as they are now.
    pub fn read(post: &Path, notes: Option<&Path>) -> Self {
        Self {
            post: stamp(post),
            notes: notes.and_then(stamp),
        }
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct Files {
        _dir: tempfile::TempDir,
        post: PathBuf,
        notes: PathBuf,
    }

    fn files(post: &str, notes: Option<&str>) -> Files {
        let dir = tempfile::tempdir().unwrap();
        let post_path = dir.path().join("post.md");
        let notes_path = dir.path().join("notes.md");
        fs::write(&post_path, post).unwrap();
        if let Some(text) = notes {
            fs::write(&notes_path, text).unwrap();
        }
        Files {
            _dir: dir,
            post: post_path,
            notes: notes_path,
        }
    }

    fn baseline(post: &str, notes: &str) -> DiskBaseline {
        DiskBaseline {
            post: post.to_string(),
            notes: notes.to_string(),
        }
    }

    fn state(f: &Files, base: &DiskBaseline) -> DiskState {
        compare(&f.post, Some(&f.notes), base).unwrap()
    }

    #[test]
    fn matching_files_are_unchanged() {
        let f = files("post", Some("notes"));
        assert_eq!(state(&f, &baseline("post", "notes")), DiskState::Unchanged);
    }

    #[test]
    fn a_different_post_is_changed() {
        let f = files("edited", Some("notes"));
        assert_eq!(state(&f, &baseline("post", "notes")), DiskState::Changed);
    }

    #[test]
    fn different_notes_are_changed() {
        let f = files("post", Some("edited"));
        assert_eq!(state(&f, &baseline("post", "notes")), DiskState::Changed);
    }

    #[test]
    fn a_missing_post_is_missing() {
        let f = files("post", Some("notes"));
        fs::remove_file(&f.post).unwrap();
        assert_eq!(state(&f, &baseline("post", "notes")), DiskState::Missing);
    }

    #[test]
    fn missing_notes_with_an_empty_baseline_are_unchanged() {
        let f = files("post", None);
        assert_eq!(state(&f, &baseline("post", "")), DiskState::Unchanged);
    }

    #[test]
    fn deleted_notes_are_changed() {
        let f = files("post", None);
        assert_eq!(state(&f, &baseline("post", "notes")), DiskState::Changed);
    }

    #[test]
    fn blank_notes_match_a_missing_file() {
        let f = files("post", None);
        assert_eq!(state(&f, &baseline("post", "  \n")), DiskState::Unchanged);
    }

    #[test]
    fn a_document_without_notes_ignores_them() {
        let f = files("post", Some("notes"));
        let got = compare(&f.post, None, &baseline("post", "")).unwrap();
        assert_eq!(got, DiskState::Unchanged);
    }

    #[test]
    fn a_stamp_moves_when_a_file_grows() {
        let f = files("post", Some("notes"));
        let before = DiskStamp::read(&f.post, Some(&f.notes));
        assert_eq!(before, DiskStamp::read(&f.post, Some(&f.notes)));
        fs::write(&f.notes, "longer notes").unwrap();
        assert_ne!(before, DiskStamp::read(&f.post, Some(&f.notes)));
    }

    #[test]
    fn a_stamp_moves_when_a_file_goes() {
        let f = files("post", None);
        let before = DiskStamp::read(&f.post, Some(&f.notes));
        fs::remove_file(&f.post).unwrap();
        assert_ne!(before, DiskStamp::read(&f.post, Some(&f.notes)));
    }
}

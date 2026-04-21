use lopress_core::{Block, FrontMatter};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

/// In-memory representation of an open post or page.
/// The editor owns this; call `Session::save` to flush to disk.
#[derive(Debug, Clone)]
pub struct LoadedDocument {
    /// Absolute path to the `.md` file.
    pub path: PathBuf,
    pub front_matter: FrontMatter,
    /// Full block tree. Only paragraph and heading blocks are editable
    /// in the UI; others are treated as opaque read-only placeholders.
    pub blocks: Vec<Block>,
    /// True when the in-memory state differs from the last write.
    pub dirty: bool,
    /// Timestamp of the last edit, used for the 500 ms debounce.
    pub dirty_at: Option<Instant>,
    /// Wall-clock time of the last successful write.
    pub last_written: Option<SystemTime>,
    /// Non-None when the most recent `Session::save` call failed.
    pub last_save_error: Option<String>,
}

impl LoadedDocument {
    /// Mark the document as having unsaved edits.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.dirty_at = Some(Instant::now());
    }

    /// Clear dirty state after a successful flush.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        self.dirty_at = None;
        self.last_save_error = None;
        self.last_written = Some(SystemTime::now());
    }
}

use crate::error::WatchError;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    pub sources: Vec<PathBuf>,
    pub theme: Vec<PathBuf>,
    pub plugins: Vec<PathBuf>,
    pub config: bool,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.theme.is_empty() && self.plugins.is_empty() && !self.config
    }
}

pub struct Watcher {
    _notify: notify::RecommendedWatcher,
    _thread: std::thread::JoinHandle<()>,
}

impl Watcher {
    pub fn spawn(
        _workspace: &std::path::Path,
        _on_change: impl FnMut(ChangeSet) + Send + 'static,
    ) -> Result<Self, WatchError> {
        unimplemented!("filled in by Task 3")
    }
}

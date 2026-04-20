use crate::classify::{classify, Bucket};
use crate::error::WatchError;
use notify::{Event, RecursiveMode, Watcher as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
    _thread: thread::JoinHandle<()>,
    _shutdown: Option<mpsc::Sender<()>>,
}

const DEBOUNCE: Duration = Duration::from_millis(200);

impl Watcher {
    pub fn spawn(
        workspace: &Path,
        mut on_change: impl FnMut(ChangeSet) + Send + 'static,
    ) -> Result<Self, WatchError> {
        // Canonicalize so classify() can match event paths from notify,
        // which on macOS resolves symlinks like /var -> /private/var and on
        // Windows returns \\?\-prefixed paths. Without this, every event
        // fails strip_prefix against a non-canonical workspace and gets
        // bucketed as Ignored.
        let workspace = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        let (tx, rx) = mpsc::channel::<Event>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let mut notify = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                let _ = tx.send(ev);
            }
        })?;

        // Watch the workspace root recursively. classify() filters per-event.
        if workspace.exists() {
            notify.watch(&workspace, RecursiveMode::Recursive)?;
        }

        let worker_ws = workspace.clone();
        let handle = thread::spawn(move || {
            debounce_loop(&worker_ws, rx, shutdown_rx, &mut on_change);
        });

        Ok(Self {
            _notify: notify,
            _thread: handle,
            _shutdown: Some(shutdown_tx),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(tx) = self._shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn debounce_loop(
    workspace: &Path,
    rx: mpsc::Receiver<Event>,
    shutdown: mpsc::Receiver<()>,
    on_change: &mut dyn FnMut(ChangeSet),
) {
    let mut pending: Option<ChangeSet> = None;
    let mut deadline: Option<Instant> = None;

    loop {
        if shutdown.try_recv().is_ok() {
            return;
        }
        let wait = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(50));
        match rx.recv_timeout(wait) {
            Ok(ev) => {
                let cs = pending.get_or_insert_with(ChangeSet::default);
                for path in ev.paths {
                    match classify(workspace, &path) {
                        Bucket::Source => push_unique(&mut cs.sources, path),
                        Bucket::Plugins => push_unique(&mut cs.plugins, path),
                        Bucket::Config => cs.config = true,
                        Bucket::Ignored => {}
                    }
                }
                deadline = Some(Instant::now() + DEBOUNCE);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(d) = deadline else { continue };
                let Some(cs) = pending.as_ref() else { continue };
                if cs.is_empty() {
                    pending = None;
                    deadline = None;
                } else if Instant::now() >= d {
                    if let Some(cs) = pending.take() {
                        on_change(cs);
                    }
                    deadline = None;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

use crate::document::LoadedDocument;
use crate::error::{LoadError, OpenError, SaveError};
use lopress_build::Workspace;
use lopress_core::perf;
use lopress_core::{parse, serialize, Document};
use lopress_serve::{serve_in_background, ServerHandle};
use lopress_watch::{ChangeSet, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub root: PathBuf,
    pub name: String,
    pub posts: Vec<DocumentRef>,
    pub pages: Vec<DocumentRef>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub path: PathBuf,
    pub title: String,
    pub slug: String,
    pub is_draft: bool,
    pub has_parse_error: bool,
}

#[derive(Debug, Clone)]
pub enum BuildStatus {
    Idle,
    Building,
    Ok {
        pages_rendered: usize,
        pages_skipped: usize,
        duration_ms: u64,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum ServeStatus {
    /// The preview server has not finished binding yet. Used while the
    /// background open thread is still working.
    Starting,
    Unavailable {
        reason: String,
    },
    Listening {
        url: String,
    },
}

// ── Session ──────────────────────────────────────────────────────────────────

pub struct Session {
    workspace: Arc<Workspace>,
    summary: Arc<Mutex<WorkspaceSummary>>,
    build_status: Arc<Mutex<BuildStatus>>,
    serve_status: Arc<Mutex<ServeStatus>>,
    server: Arc<Mutex<Option<Arc<ServerHandle>>>>,
    _watcher: Option<Watcher>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    /// Open a workspace. Runs an initial build and starts watch + serve.
    ///
    /// # Errors
    /// Returns `OpenError` if `lopress.toml` is missing or unparseable.
    pub fn open(workspace_root: &Path) -> Result<Self, OpenError> {
        // Synchronous: workspace load.
        let workspace = {
            let _t = perf::span("workspace.open.workspace_load");
            Workspace::load(workspace_root)
                .map_err(|e| OpenError::InvalidWorkspace(e.to_string()))?
        };
        let workspace = Arc::new(workspace);

        // Synchronous: workspace scan.
        let summary = Arc::new(Mutex::new({
            let _t = perf::span("workspace.open.scan");
            scan_workspace(&workspace)
        }));

        // Initial statuses reflect the "still working in the background" state.
        let build_status = Arc::new(Mutex::new(BuildStatus::Building));
        let serve_status = Arc::new(Mutex::new(ServeStatus::Starting));
        let server: Arc<Mutex<Option<Arc<ServerHandle>>>> = Arc::new(Mutex::new(None));

        let www_dir = workspace.www_dir();
        std::fs::create_dir_all(&www_dir).ok();

        // Background thread: initial build, then start serve.
        let ws_root_bg = workspace_root.to_path_buf();
        let build_status_bg = Arc::clone(&build_status);
        let serve_status_bg = Arc::clone(&serve_status);
        let server_bg = Arc::clone(&server);
        let summary_bg = Arc::clone(&summary);
        let workspace_bg = Arc::clone(&workspace);
        let www_dir_bg = www_dir.clone();
        std::thread::spawn(move || {
            // Initial build.
            {
                let _t = perf::span("workspace.open.initial_build");
                let t0 = std::time::Instant::now();
                match lopress_build::build(&ws_root_bg) {
                    Ok(r) => {
                        *lock(&build_status_bg) = BuildStatus::Ok {
                            pages_rendered: r.pages_rendered,
                            pages_skipped: r.pages_skipped,
                            duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                        };
                        *lock(&summary_bg) = scan_workspace(&workspace_bg);
                    }
                    Err(e) => {
                        *lock(&build_status_bg) = BuildStatus::Failed {
                            message: e.to_string(),
                        };
                    }
                }
            }
            // Start serve (try 8080, fall back to ephemeral).
            {
                let _t = perf::span("workspace.open.serve_start");
                let new_serve =
                    match serve_in_background(www_dir_bg.clone(), "127.0.0.1".into(), 8080) {
                        Ok(h) => {
                            let url = h.url.clone();
                            *lock(&server_bg) = Some(Arc::new(h));
                            ServeStatus::Listening { url }
                        }
                        Err(_) => match serve_in_background(www_dir_bg, "127.0.0.1".into(), 0) {
                            Ok(h) => {
                                let url = h.url.clone();
                                *lock(&server_bg) = Some(Arc::new(h));
                                ServeStatus::Listening { url }
                            }
                            Err(e) => ServeStatus::Unavailable {
                                reason: e.to_string(),
                            },
                        },
                    };
                *lock(&serve_status_bg) = new_serve;
            }
        });

        // Watcher: still spawned synchronously. Broadcasts via the shared server mutex.
        let ws_root_w = workspace_root.to_path_buf();
        let build_status_w = Arc::clone(&build_status);
        let summary_w = Arc::clone(&summary);
        let workspace_w = Arc::clone(&workspace);
        let server_w = Arc::clone(&server);

        let watcher = Watcher::spawn(workspace_root, move |_cs: ChangeSet| {
            *lock(&build_status_w) = BuildStatus::Building;
            let t0 = std::time::Instant::now();
            match lopress_build::build(&ws_root_w) {
                Ok(r) => {
                    *lock(&build_status_w) = BuildStatus::Ok {
                        pages_rendered: r.pages_rendered,
                        pages_skipped: r.pages_skipped,
                        duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    };
                    *lock(&summary_w) = scan_workspace(&workspace_w);
                    let srv = lock(&server_w).as_ref().map(Arc::clone);
                    if let Some(srv) = srv {
                        srv.broadcast_reload();
                    }
                }
                Err(e) => {
                    *lock(&build_status_w) = BuildStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        })
        .ok();

        Ok(Self {
            workspace,
            summary,
            build_status,
            serve_status,
            server,
            _watcher: watcher,
        })
    }

    /// Current workspace snapshot.
    pub fn workspace(&self) -> WorkspaceSummary {
        lock(&self.summary).clone()
    }

    /// Load and parse a document.
    ///
    /// # Errors
    /// Returns `LoadError` on I/O or parse failure.
    pub fn load_document(&self, path: &Path) -> Result<LoadedDocument, LoadError> {
        let raw = std::fs::read_to_string(path)?;
        match parse(&raw) {
            Ok(doc) => Ok(LoadedDocument {
                path: path.to_path_buf(),
                front_matter: doc.front_matter,
                blocks: doc.blocks,
                dirty: false,
                dirty_at: None,
                last_written: path.metadata().and_then(|m| m.modified()).ok(),
                last_save_error: None,
            }),
            Err(e) => Err(LoadError::Parse {
                raw,
                line: 0,
                message: e.to_string(),
            }),
        }
    }

    /// Serialize and atomically write a document to disk.
    ///
    /// # Errors
    /// Returns `SaveError` on I/O failure.
    pub fn save(&self, doc: &LoadedDocument) -> Result<(), SaveError> {
        let content = {
            let _t = perf::span("editor.save.serialize");
            serialize(&Document {
                front_matter: doc.front_matter.clone(),
                blocks: doc.blocks.clone(),
            })
        };
        {
            let _t = perf::span("editor.save.write");
            atomic_write(&doc.path, content.as_bytes())?;
        }
        Ok(())
    }

    /// Posts directory for this workspace. Sidebar uses this to write
    /// new-post stubs.
    pub fn posts_dir(&self) -> PathBuf {
        self.workspace.posts_dir()
    }

    /// Pages directory for this workspace.
    pub fn pages_dir(&self) -> PathBuf {
        self.workspace.pages_dir()
    }

    /// Re-scan the workspace's posts and pages directories and update the
    /// cached summary. Call this after creating a new file so the next
    /// `workspace()` call reflects it without waiting for the watcher.
    pub fn rescan(&self) -> WorkspaceSummary {
        let new_summary = scan_workspace(&self.workspace);
        *lock(&self.summary) = new_summary.clone();
        new_summary
    }

    /// Load the plugin registry for this workspace. Recomputes on each call —
    /// callers that want to cache should hold onto the returned value (e.g.
    /// `EditingState` does this once at session-open time).
    pub fn plugin_registry(&self) -> lopress_plugin::PluginRegistry {
        let plugins_dir = self.workspace.plugins_dir();
        let enabled = if self.workspace.config.plugins.enabled.is_empty() {
            None
        } else {
            Some(self.workspace.config.plugins.enabled.as_slice())
        };
        lopress_plugin::load_dir(&plugins_dir, enabled).unwrap_or_default()
    }

    /// Trigger a rebuild and SSE broadcast on a background thread.
    /// Safe to call even if a watcher-triggered rebuild is already in flight —
    /// the worst case is two sequential builds.
    pub fn rebuild(&self) {
        let build_status = Arc::clone(&self.build_status);
        let workspace_root = self.workspace.root.clone();
        let server = Arc::clone(&self.server);
        std::thread::spawn(move || {
            *lock(&build_status) = BuildStatus::Building;
            let t0 = std::time::Instant::now();
            match lopress_build::build(&workspace_root) {
                Ok(r) => {
                    *lock(&build_status) = BuildStatus::Ok {
                        pages_rendered: r.pages_rendered,
                        pages_skipped: r.pages_skipped,
                        duration_ms: t0.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    };
                    let srv = lock(&server).as_ref().map(Arc::clone);
                    if let Some(srv) = srv {
                        srv.broadcast_reload();
                    }
                }
                Err(e) => {
                    *lock(&build_status) = BuildStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        });
    }

    /// Current build status.
    pub fn build_status(&self) -> BuildStatus {
        lock(&self.build_status).clone()
    }

    /// Current serve status.
    pub fn serve_status(&self) -> ServeStatus {
        lock(&self.serve_status).clone()
    }

    /// Copy `src` into the workspace's `src/images/` and return its web path
    /// (`/images/<filename>`). On a filename collision with different bytes, a
    /// numeric suffix is appended; identical bytes reuse the existing file.
    ///
    /// # Errors
    /// Returns `SaveError` on I/O failure.
    pub fn import_image(&self, src: &Path) -> Result<String, SaveError> {
        let images_dir = self.workspace.images_dir();
        std::fs::create_dir_all(&images_dir).map_err(SaveError::Io)?;
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("bin");
        let bytes = std::fs::read(src).map_err(SaveError::Io)?;

        // Find a non-colliding name; reuse if identical bytes already present.
        let mut filename = format!("{stem}.{ext}");
        let mut n: u32 = 1;
        loop {
            let candidate = images_dir.join(&filename);
            if !candidate.exists() {
                break;
            }
            if std::fs::read(&candidate)
                .map(|b| b == bytes)
                .unwrap_or(false)
            {
                return Ok(format!("/images/{filename}"));
            }
            filename = format!("{stem}-{n}.{ext}");
            n += 1;
        }
        std::fs::write(images_dir.join(&filename), &bytes).map_err(SaveError::Io)?;
        Ok(format!("/images/{filename}"))
    }

    /// URL for the given document in the browser.
    pub fn preview_url_for(&self, doc_ref: &DocumentRef) -> Option<String> {
        let status = lock(&self.serve_status).clone();
        let url = match &status {
            ServeStatus::Listening { url } => url.clone(),
            ServeStatus::Unavailable { .. } | ServeStatus::Starting => return None,
        };
        let slug = doc_ref
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("index")
            .to_string();
        let ws_posts = self.workspace.posts_dir();
        if doc_ref.path.starts_with(&ws_posts) {
            Some(format!("{url}/posts/{slug}/"))
        } else {
            Some(format!("{url}/{slug}/"))
        }
    }

    /// Current nav items, read fresh from nav.toml on disk so repeated
    /// edits in one session reflect the latest saved state. Empty when
    /// the file doesn't exist or doesn't parse.
    pub fn nav_items(&self) -> Vec<lopress_build::NavItem> {
        let nav_path = self.workspace.root.join("nav.toml");
        let Ok(src) = std::fs::read_to_string(&nav_path) else {
            return Vec::new();
        };
        toml::from_str::<lopress_build::Nav>(&src)
            .map(|nav| nav.items)
            .unwrap_or_default()
    }

    /// Write nav items to nav.toml, then trigger a rebuild + SSE reload.
    ///
    /// # Errors
    /// Returns an error if nav.toml can't be serialized or written.
    pub fn update_nav(&self, items: Vec<lopress_build::NavItem>) -> Result<(), SaveError> {
        lopress_build::write_nav(&self.workspace.root, &items)?;
        self.rebuild();
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn scan_workspace(ws: &Workspace) -> WorkspaceSummary {
    let posts = scan_dir(&ws.posts_dir());
    let pages = scan_dir(&ws.pages_dir());

    // Collect tags from post front-matter (sorted, de-duplicated).
    let mut tags_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(ws.posts_dir()).ok().into_iter().flatten() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Ok(src) = std::fs::read_to_string(&path) {
            if let Ok(doc) = parse(&src) {
                for tag in &doc.front_matter.tags {
                    tags_set.insert(tag.clone());
                }
            }
        }
    }
    let tags: Vec<String> = tags_set.into_iter().collect();

    WorkspaceSummary {
        root: ws.root.clone(),
        name: ws.config.site.title.clone(),
        posts,
        pages,
        tags,
    }
}

fn scan_dir(dir: &Path) -> Vec<DocumentRef> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut refs: Vec<DocumentRef> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .map(|e| {
            let path = e.path();
            match std::fs::read_to_string(&path).as_deref().map(parse) {
                Ok(Ok(doc)) => {
                    let slug = doc.front_matter.slug.unwrap_or_else(|| {
                        path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("untitled")
                            .to_string()
                    });
                    DocumentRef {
                        title: doc.front_matter.title.unwrap_or_else(|| slug.clone()),
                        slug,
                        is_draft: doc.front_matter.draft,
                        has_parse_error: false,
                        path,
                    }
                }
                _ => DocumentRef {
                    title: stem(&path),
                    slug: stem(&path),
                    is_draft: false,
                    has_parse_error: true,
                    path,
                },
            }
        })
        .collect();
    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent",
        ));
    };
    let tmp = parent.join(format!(
        ".lopress-tmp-{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

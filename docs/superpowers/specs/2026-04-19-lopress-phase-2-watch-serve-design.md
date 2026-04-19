# Lopress Phase 2 — Watcher, Serve, Incremental Build

Addendum to [`2026-04-18-lopress-design.md`](./2026-04-18-lopress-design.md). This doc locks down decisions that the v1 design left implicit or punted to phase 2.

## 1. Scope

Three deliverables, one binary, no GUI yet:

1. **`lopress-watch` crate** — debounced fs watcher over a workspace, emits batched change events.
2. **`lopress-serve` crate + `lopress serve` subcommand** — static HTTP server over `www/`, watcher-driven rebuild, browser live-reload via Server-Sent Events.
3. **Incremental build** — `lopress-build::build` consults a cache at `www/.lopress-cache.json`, skips re-rendering unchanged pages, and invalidates correctly on theme/plugin/config changes.

Also: commit `Cargo.lock` (binary crate convention; eliminates transitive-MSRV drift between machines).

## 2. Watcher

### 2.1 Crate boundary

`lopress-watch` depends on `notify 6` and `lopress-build` (for the `Workspace` struct to know which dirs to watch and ignore). It does not depend on `-core`, `-theme`, `-plugin`, `-assets`.

### 2.2 Public API

```rust
pub struct Watcher { /* opaque */ }

#[derive(Debug, Clone)]
pub struct ChangeSet {
    pub sources: Vec<PathBuf>,    // under src/
    pub theme: Vec<PathBuf>,      // theme dir (workspace-local or plugin-provided)
    pub plugins: Vec<PathBuf>,    // under plugins/
    pub config: bool,             // lopress.toml touched
}

impl Watcher {
    pub fn spawn(workspace: &Path, on_change: impl FnMut(ChangeSet) + Send + 'static)
        -> Result<Watcher, WatchError>;
}
// Dropping the Watcher stops the background thread.
```

`on_change` fires on the debounce thread, not the notify callback thread.

### 2.3 Behaviour

- Watches: `lopress.toml`, `src/`, `plugins/`, and any theme directory outside those (if the active theme is a workspace-local plugin, it's already under `plugins/`; if it ever lives elsewhere, the watcher follows it).
- Ignores: `www/`, `target/`, `.git/`, `www/.lopress-cache.json`, any dotfile directories, editor swap files (`*.swp`, `*.swx`, `.#*`, `4913`, files starting with `~`).
- Debounce window: **200 ms** (per spec §4.2). Events arriving during a debounce window are coalesced into one `ChangeSet`.
- If notify reports an error (permission, too many watches), the watcher logs and keeps running; it does not crash the process.

### 2.4 Non-goals

No incrementality *inside* the watcher — it hands off paths to the build layer, which decides what to rebuild. The watcher does not deduplicate "content unchanged" writes; that's the cache's job.

## 3. Incremental build

### 3.1 Cache schema

`www/.lopress-cache.json`:

```json
{
  "version": 1,
  "config_hash": "blake3-hex...",
  "theme_hash": "blake3-hex...",
  "plugins_hash": "blake3-hex...",
  "pages": {
    "src/posts/hello.md": {
      "source_hash": "blake3-hex...",
      "outputs": ["posts/hello/index.html"],
      "tags": ["intro"],
      "is_draft": false,
      "title": "Hello",
      "date": "2026-04-18"
    }
  }
}
```

- `theme_hash` = blake3 of concatenated theme template sources in sorted order, plus theme CSS bytes.
- `plugins_hash` = blake3 of concatenated plugin manifests + block templates + asset bytes in sorted order.
- `config_hash` = blake3 of `lopress.toml` bytes.
- `pages` is keyed by workspace-relative path (always forward-slash, even on Windows).

### 3.2 Invalidation rules

Any of these forces a **full rebuild** (cache ignored, regenerated from scratch):

1. Cache file missing or `version` mismatched.
2. `config_hash` changed.
3. `theme_hash` changed.
4. `plugins_hash` changed.

Otherwise, **per-file incremental**:

- If a post/page's `source_hash` matches the cache **and all listed outputs exist on disk**, skip re-rendering. If any listed output is missing (e.g., user deleted `www/`), re-render.
- If the hash differs or the entry is missing, re-render and update the cache entry.
- Deleted post: remove its `outputs` from disk, drop its cache entry.

**Aggregate pages** (`index.html`, `feed.xml`, `sitemap.xml`, `tags/*/index.html`) are rebuilt whenever *any* post changed, was added, or was deleted, OR when full-rebuild conditions apply. Tag-archive *directories* for tags that no longer exist are removed.

**Images** keep their existing per-variant cache at `www/.lopress-image-cache.json`; no change.

### 3.3 API

`lopress_build::build(workspace)` stays the same entry point but internally:

1. Loads the cache.
2. Computes current hashes for config/theme/plugins.
3. If any differ → clears cache, proceeds as full rebuild.
4. Otherwise → per-page incremental.
5. Saves cache before returning.

`BuildReport` gains: `pages_rendered: usize`, `pages_skipped: usize` (in addition to existing `pages_written` and `failures`).

## 4. Serve command

### 4.1 CLI

```
lopress serve <workspace> [--port 8080] [--bind 127.0.0.1] [--no-open]
```

- Default bind: `127.0.0.1` (loopback only).
- Default port: `8080`. If in use, error with a clear message (no fallback).
- `--no-open`: skip opening the default browser. Default is to open `http://<bind>:<port>/`.

### 4.2 Lifecycle

1. Run a full `build(&workspace)`.
2. Bind TCP listener.
3. Spawn `Watcher` → on every `ChangeSet`: run `build(&workspace)` again, then broadcast a reload event.
4. Accept loop serves requests on per-connection threads.
5. Ctrl-C → shutdown listener, drop watcher, wait briefly for open connections, exit 0.

### 4.3 Crate boundary

`lopress-serve` depends on `lopress-build`, `lopress-watch`. Hand-rolled HTTP/1.1 only — no `axum`, `hyper`, `tiny_http`. Justification: zero new dep surface, and the feature set we need (GET, static files, SSE) is under 300 lines.

### 4.4 HTTP server

- `TcpListener::bind` + per-connection thread. OK for local dev; not a production server.
- Parse request line + headers; require `Connection: keep-alive` optional; don't bother with keep-alive, close after each response except for SSE.
- Only `GET` supported; anything else → `405`.
- Path resolution inside `www/`:
  - `/` → `www/index.html`
  - `/foo/` → `www/foo/index.html`
  - `/foo` where `www/foo/index.html` exists → `301 Location: /foo/`
  - `/foo.css` → `www/foo.css`
  - anything resolving outside `www/` (path traversal) → `403`
  - not found → `www/404.html` with status `404` if present, else plain `404 Not Found`
- MIME types: explicit match on extension. `html css js xml txt json jpg jpeg png webp svg ico woff woff2`. Default `application/octet-stream`.
- For `text/html` responses, inject reload script before `</body>` (see §4.6). If `</body>` is absent, append at end.

### 4.5 SSE endpoint

- Route: `GET /__lopress/reload`
- Response headers: `Content-Type: text/event-stream`, `Cache-Control: no-cache`, `Connection: keep-alive`.
- On connect: write `retry: 1000\n\n`.
- On build complete: server writes `event: reload\ndata: {}\n\n` to every open stream.
- Every 15 s: write `:ping\n\n` keepalive.
- Broken streams (write errors) are removed from the subscriber list.

Implementation: a `Mutex<Vec<TcpStream>>` as the subscriber registry. The request handler for `/__lopress/reload` appends its stream and parks. The watcher callback acquires the mutex, writes to each stream, and prunes dead ones.

### 4.6 Reload script

Injected verbatim:

```html
<script>
(() => {
  const es = new EventSource('/__lopress/reload');
  es.addEventListener('reload', () => location.reload());
})();
</script>
```

Injection runs on every HTML response; the script is small enough (~150 bytes) that caching it separately isn't worth the extra route.

## 5. Testing

- `lopress-watch`: integration test with `tempfile` that writes files into a workspace and asserts the debounce window coalesces rapid writes into one `ChangeSet`.
- Incremental build: integration test — full build, touch one post, build again, assert `pages_rendered == 1` and `pages_skipped > 0`. Also: delete a post, assert its output is removed.
- Invalidation: touch `lopress.toml`, assert full rebuild (all pages rendered).
- `lopress-serve`: integration test binds to port 0, GETs `/` and asserts reload script is present; GETs `/__lopress/reload`, checks headers and retry frame. Does not test the reload push itself (too flaky for CI).

## 6. CI

Existing workflow unchanged. The new `lopress-watch` and `lopress-serve` crates are picked up automatically by `cargo test --workspace`.

## 7. Out of scope

- HTTPS, auth, production-grade serving — this is a dev loop tool.
- Live-reload granularity beyond "full reload of current page" (no DOM patching, no CSS-only hot-swap).
- WebSocket transport.
- Watching outside the workspace root.
- Docker/container concerns.

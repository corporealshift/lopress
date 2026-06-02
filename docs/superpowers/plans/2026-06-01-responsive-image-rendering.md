# Responsive Image Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render core `image` blocks already in a site as responsive `<picture>` elements (WebP `srcset` + original fallback, optional caption), using the existing `lopress-assets` WebP pipeline.

**Architecture:** The image pipeline already generates `{stem}.{w}w.webp` variants from `src/images/` but runs *after* page rendering, so the renderer can't know which variants exist. This plan moves image processing **before** rendering, builds an `ImageIndex` (source stem → available variants + original), threads it into `render_body`, and rewrites the `image` render arm to emit `<picture>`. No editor changes — this is `lopress-build` / `lopress-theme` only.

**Tech Stack:** Rust, `lopress-assets` (WebP via `image`/`webp`), Tera, the workspace's strict clippy lints (`AGENTS.md`).

**Spec:** `docs/superpowers/specs/2026-06-01-image-block-design.md` (this plan implements §6 "Build: Responsive Rendering"; the editor sections are the separate authoring plan).

> **Scope boundary:** This plan handles *display on the built site* for any `image` block — i.e. `![alt](src)` markdown or a hand-authored image. Inserting/importing images and the in-editor widget are the separate authoring plan (`2026-06-01-image-block-authoring.md`).

> **Cross-plan note:** This plan adds an `image_index: &ImageIndex` parameter to `render_body`. If the read-more plan's `render_excerpt` (which also calls `write_block`/`render_body`) has already landed, give it the same parameter. Reconcile signatures if both plans are in flight.

> **Gate:** run `bash scripts/check.sh` before declaring done.

---

## Task 1: The `ImageIndex` type

**Files:**
- Create: `crates/lopress-build/src/image_index.rs`
- Modify: `crates/lopress-build/src/lib.rs` (add `mod image_index;` / re-export)
- Test: `crates/lopress-build/src/image_index.rs`

- [ ] **Step 1: Write the failing test**

`crates/lopress-build/src/image_index.rs`:

```rust
//! A build-time index of processed images, so the renderer can emit a correct
//! responsive `srcset`. Keyed by the source file *stem* (filename without
//! extension), matching `lopress_assets::variant_filename`'s `{stem}.{w}w.{ext}`
//! naming and the `{stem}.{ext}` original copy.

use lopress_assets::ImageResult;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ImageVariant {
    pub width: u32,
    /// Filename relative to `www/images/`, e.g. `photo.800w.webp`.
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    /// The full-size original, relative to `www/images/`, e.g. `photo.jpg`.
    pub original: String,
    /// WebP variants, ascending by width.
    pub webp: Vec<ImageVariant>,
}

#[derive(Debug, Clone, Default)]
pub struct ImageIndex {
    by_stem: BTreeMap<String, ImageEntry>,
}

impl ImageIndex {
    pub fn get(&self, stem: &str) -> Option<&ImageEntry> {
        self.by_stem.get(stem)
    }

    /// Record the variants produced for `src` (the source image path).
    pub fn record(&mut self, src: &Path, result: &ImageResult) {
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();
        let ext = src
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_lowercase();
        let mut webp: Vec<ImageVariant> = result
            .files
            .iter()
            .filter(|v| v.format == "webp")
            .map(|v| ImageVariant {
                width: v.width,
                filename: v.filename.to_string_lossy().into_owned(),
            })
            .collect();
        webp.sort_by_key(|v| v.width);
        self.by_stem.insert(
            stem.clone(),
            ImageEntry {
                original: format!("{stem}.{ext}"),
                webp,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopress_assets::Variant;
    use std::path::PathBuf;

    #[test]
    fn records_and_sorts_webp_variants() {
        let mut idx = ImageIndex::default();
        let result = ImageResult {
            files: vec![
                Variant { filename: PathBuf::from("photo.800w.webp"), width: 800, format: "webp".into() },
                Variant { filename: PathBuf::from("photo.400w.webp"), width: 400, format: "webp".into() },
                Variant { filename: PathBuf::from("photo.800w.jpg"), width: 800, format: "jpg".into() },
            ],
        };
        idx.record(Path::new("/src/images/photo.jpg"), &result);
        let entry = idx.get("photo").expect("entry");
        assert_eq!(entry.original, "photo.jpg");
        assert_eq!(entry.webp.len(), 2, "only webp variants");
        assert_eq!(entry.webp[0].width, 400, "ascending by width");
        assert_eq!(entry.webp[1].width, 800);
    }
}
```

(Confirm `lopress_assets::ImageResult` and `Variant` are `pub` — they are, per `crates/lopress-assets/src/image.rs`. If `ImageResult`/`Variant` aren't re-exported from the assets crate root, add `pub use` there or import the full path.)

- [ ] **Step 2: Register the module**

In `crates/lopress-build/src/lib.rs`, add `mod image_index;` and `pub use image_index::ImageIndex;` (match how other modules are declared/re-exported in that file).

- [ ] **Step 3: Run the test**

Run: `cargo test -p lopress-build records_and_sorts_webp_variants`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/src/image_index.rs crates/lopress-build/src/lib.rs
git commit -m "feat(build): add ImageIndex for responsive image rendering"
```

---

## Task 2: Rewrite the `image` render arm to emit `<picture>`

**Files:**
- Modify: `crates/lopress-build/src/render.rs` (`render_body`, `write_block`, `image` arm)
- Test: `crates/lopress-build/src/render.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/lopress-build/src/render.rs`:

```rust
#[test]
fn image_in_index_renders_picture_with_srcset() {
    use crate::image_index::{ImageEntry, ImageIndex, ImageVariant};
    let mut idx = ImageIndex::default();
    // Manually seed an entry (the real index is built by the pipeline).
    seed_index(&mut idx, "photo", "photo.jpg", &[(400, "photo.400w.webp"), (800, "photo.800w.webp")]);
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "image".into(),
            attrs: json!({ "src": "/images/photo.jpg", "alt": "A & B", "caption": "Cap" }),
            children: vec![],
            text: None,
        }],
    };
    let html = render_body_with_images(&doc, &empty_registry(), &Tera::default(), &idx).unwrap();
    assert!(html.contains("<picture>"), "got: {html}");
    assert!(html.contains(r#"type="image/webp""#));
    assert!(html.contains("/images/photo.400w.webp 400w"));
    assert!(html.contains("/images/photo.800w.webp 800w"));
    assert!(html.contains(r#"src="/images/photo.jpg""#));
    assert!(html.contains(r#"alt="A &amp; B""#), "alt escaped");
    assert!(html.contains("<figcaption>Cap</figcaption>"));
}

#[test]
fn image_not_in_index_falls_back_to_plain_img() {
    use crate::image_index::ImageIndex;
    let idx = ImageIndex::default();
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "image".into(),
            attrs: json!({ "src": "https://ex.com/x.png", "alt": "x" }),
            children: vec![],
            text: None,
        }],
    };
    let html = render_body_with_images(&doc, &empty_registry(), &Tera::default(), &idx).unwrap();
    assert!(!html.contains("<picture>"));
    assert!(html.contains(r#"<img src="https://ex.com/x.png" alt="x""#));
}

// Test helper: seed an ImageIndex without the asset pipeline.
#[cfg(test)]
fn seed_index(idx: &mut crate::image_index::ImageIndex, stem: &str, original: &str, variants: &[(u32, &str)]) {
    use lopress_assets::{ImageResult, Variant};
    use std::path::PathBuf;
    let files = variants
        .iter()
        .map(|(w, f)| Variant { filename: PathBuf::from(*f), width: *w, format: "webp".into() })
        .collect();
    idx.record(&PathBuf::from(format!("/src/images/{original}")), &ImageResult { files });
    let _ = stem; // stem is derived from `original` inside record()
}
```

(`render_body_with_images` is the new public name from Step 3. If you prefer keeping `render_body` as the name, see the note in Step 3 — the chosen approach renames the threaded function and keeps a thin `render_body` wrapper is **not** used here; callers are updated to the new signature in Task 3.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-build image_in_index_renders_picture_with_srcset image_not_in_index_falls_back_to_plain_img`
Expected: FAIL — `render_body_with_images` doesn't exist; the current `image` arm emits a bare `<img>` with no `<picture>`.

- [ ] **Step 3: Add the index parameter and rewrite the arm**

In `crates/lopress-build/src/render.rs`:

- Change `render_body` and `write_block` to thread `image_index: &ImageIndex`. Rename the public entry to `render_body_with_images` (callers updated in Task 3), or — simpler and preferred — keep the name `render_body` and add the parameter, updating all callers in Task 3. **This plan uses the added-parameter approach: keep `render_body(doc, registry, tera, image_index)`.** Update the test names in Step 1 accordingly (`render_body` instead of `render_body_with_images`).

```rust
use crate::image_index::ImageIndex;

pub fn render_body(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
    image_index: &ImageIndex,
) -> Result<String, BuildError> {
    let mut out = String::new();
    for b in &doc.blocks {
        write_block(&mut out, b, registry, tera, image_index)?;
    }
    Ok(out)
}

fn write_block(
    out: &mut String,
    b: &Block,
    registry: &PluginRegistry,
    tera: &Tera,
    image_index: &ImageIndex,
) -> Result<(), BuildError> {
    match b.r#type.as_str() {
        // ... existing arms gain `image_index` where they recurse:
        //   quote/list children → write_block(out, c, registry, tera, image_index)?
        //   render_custom(out, b, custom, registry, tera, image_index)?  (recurses)
        "image" => {
            write_image(out, b, image_index);
        }
        // ...
    }
    Ok(())
}
```

Add the `write_image` helper:

```rust
fn write_image(out: &mut String, b: &Block, image_index: &ImageIndex) {
    let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
    let alt = escape(b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or(""));
    let caption = b.attrs.get("caption").and_then(|v| v.as_str()).unwrap_or("");

    // Resolve the stem from a `/images/<file>` src; only those are in the index.
    let entry = src
        .strip_prefix("/images/")
        .and_then(|file| Path::new(file).file_stem().and_then(|s| s.to_str()))
        .and_then(|stem| image_index.get(stem));

    out.push_str("<figure>\n");
    match entry {
        Some(entry) if !entry.webp.is_empty() => {
            let srcset = entry
                .webp
                .iter()
                .map(|v| format!("/images/{} {}w", v.filename, v.width))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "<picture><source type=\"image/webp\" srcset=\"{srcset}\" sizes=\"(max-width: 800px) 100vw, 800px\"><img src=\"/images/{}\" alt=\"{alt}\" loading=\"lazy\"></picture>",
                entry.original
            );
        }
        _ => {
            let s = escape(src);
            let _ = writeln!(out, "<img src=\"{s}\" alt=\"{alt}\" loading=\"lazy\">");
        }
    }
    if !caption.is_empty() {
        let c = escape(caption);
        let _ = writeln!(out, "<figcaption>{c}</figcaption>");
    }
    out.push_str("</figure>\n");
}
```

(`Path` is from `std::path::Path` — add the import. `write!`/`writeln!` and `escape` are already in this file.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p lopress-build image_in_index_renders_picture_with_srcset image_not_in_index_falls_back_to_plain_img`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-build/src/render.rs
git commit -m "feat(build): render image blocks as responsive <picture>"
```

---

## Task 3: Thread `ImageIndex` through the page renderers

**Files:**
- Modify: `crates/lopress-build/src/pages.rs` (`render_all`, `render_one_post`, `render_one_page`)
- Test: existing tests compile-check the new signatures

- [ ] **Step 1: Update `render_one_post` / `render_one_page`**

Each calls `render_body(&post.doc, registry, tera_shared)`. Add an `image_index: &ImageIndex` parameter and pass it through:

```rust
pub fn render_one_post(
    www: &Path,
    site: &SiteCtx,
    post: &DiscoveredPost,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
    image_index: &crate::image_index::ImageIndex,
) -> Result<(), BuildError> {
    let body_html = render_body(&post.doc, registry, tera_shared, image_index)?;
    // ... unchanged ...
}
```

Same for `render_one_page`.

- [ ] **Step 2: Update `render_all`**

Add `image_index: &ImageIndex` to `render_all`'s parameters (it already has `#[allow(clippy::too_many_arguments)]`), and pass it into both `render_one_post(...)` and `render_one_page(...)` calls.

```rust
#[allow(clippy::too_many_arguments)]
pub fn render_all(
    workspace: &Workspace,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
    posts: &[DiscoveredPost],
    pages: &[DiscoveredPost],
    cache: &mut crate::cache::BuildCache,
    force_full: bool,
    image_index: &crate::image_index::ImageIndex,
) -> Result<RenderStats, BuildError> {
    // ... pass image_index to render_one_post / render_one_page ...
}
```

- [ ] **Step 3: Fix any in-crate test callers**

Run: `cargo test -p lopress-build --no-run`
Expected: compile errors only at `render_body` / `render_one_*` / `render_all` test call sites. Fix each by passing `&ImageIndex::default()` (tests that don't exercise images can use the empty index). Re-run until it compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/src/pages.rs
git commit -m "refactor(build): thread ImageIndex through the page renderers"
```

---

## Task 4: Build the index before rendering (reorder the pipeline)

**Files:**
- Modify: `crates/lopress-build/src/build.rs`

- [ ] **Step 1: Move the image pipeline ahead of `render_all` and capture the index**

In `crates/lopress-build/src/build.rs`, the image pipeline currently sits near the end (the block starting `// Image pipeline (always run …)`). Move that block to **before** the `pages::render_all(...)` call (after the `www/` wipe and Tera setup), and build an `ImageIndex` as it processes:

```rust
// Image pipeline — run before rendering so the renderer can emit a correct
// responsive srcset. Has its own per-file cache.
let mut image_index = crate::image_index::ImageIndex::default();
let mut img_cache = VariantCache::load(&ws.www_dir().join(".lopress-image-cache.json"))?;
let spec = VariantSpec {
    widths: ws.config.build.image_variants.clone(),
    ..VariantSpec::default()
};
let src_images = ws.images_dir();
let www_images = ws.www_dir().join("images");
if src_images.exists() {
    for entry in walkdir::WalkDir::new(&src_images).min_depth(1) {
        let entry = entry.map_err(std::io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        match process_image(entry.path(), &www_images, &mut img_cache, &spec) {
            Ok(result) => image_index.record(entry.path(), &result),
            Err(e) => failures.push(PageFailure {
                path: entry.path().to_path_buf(),
                message: format!("image: {e}"),
            }),
        }
    }
}
img_cache.save(&ws.www_dir().join(".lopress-image-cache.json"))?;
```

Then pass `&image_index` into `pages::render_all(...)`. Delete the old image-pipeline block from its previous location near the end. Ensure `failures` is declared before this block (it is — `discover` populates it; if the reorder puts image processing before `discover`, move the image block to just after `discover` so `failures` exists, or initialize `failures` earlier).

- [ ] **Step 2: Verify ordering against the force-full wipe**

The force-full `www/` wipe (the `if force_full { … }` block) preserves `.lopress-image-cache.json` but deletes `www/images`. Image processing must run **after** that wipe so variants are regenerated into the fresh `www/`. Confirm the moved block sits after the wipe block and before `render_all`. `process_image` regenerates any missing files (`!out_path.exists()`), so a wiped `www/images` is repopulated.

- [ ] **Step 3: Build to confirm it compiles**

Run: `cargo build -p lopress-build`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/src/build.rs
git commit -m "refactor(build): process images before rendering to feed the ImageIndex"
```

---

## Task 5: Build-integration test over the `with-images` fixture

**Files:**
- Modify: `crates/lopress-build/tests/fixtures/with-images/src/posts/album.md` (ensure it references an image at `/images/<file>`)
- Test: `crates/lopress-build/tests/build_integration.rs`

- [ ] **Step 1: Inspect the fixture**

Run: `ls crates/lopress-build/tests/fixtures/with-images/src/images/ 2>/dev/null; cat crates/lopress-build/tests/fixtures/with-images/src/posts/album.md`
Note the image filename(s) present and how `album.md` references them. If `src/images/` has no real image file, add a small PNG fixture (a few px) so `process_image` produces variants. Ensure `album.md` contains a standalone `![alt](/images/<file>)` referencing it.

- [ ] **Step 2: Write the test**

In `crates/lopress-build/tests/build_integration.rs`, following the file's existing fixture-build harness (it copies a fixture to a temp dir, runs `lopress_build::build`, reads `www/`):

```rust
#[test]
fn images_render_as_responsive_picture() {
    // Build the with-images fixture into a temp www and assert the post HTML
    // contains a <picture> with a webp srcset and an <img> fallback.
    // (Use the same fixture-copy + build harness as the other tests here.)
    let www = /* build the with-images fixture */;
    let post = std::fs::read_to_string(www.join("posts/album/index.html")).unwrap();
    assert!(post.contains("<picture>"), "expected responsive picture, got:\n{post}");
    assert!(post.contains("image/webp"));
    assert!(post.contains(".webp"));
}
```

Fill the build harness portion the same way neighbouring tests do (slug may differ — adjust `posts/album/index.html` to the fixture's actual post slug). If the fixture image is smaller than all configured widths (400/800/1600), `process_image` skips webp variants and the renderer falls back to `<img>` — use a fixture image at least 401px wide so at least the 400w webp variant exists, making the `<picture>` assertion valid.

- [ ] **Step 3: Run it**

Run: `cargo test -p lopress-build images_render_as_responsive_picture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/tests/build_integration.rs crates/lopress-build/tests/fixtures/with-images/
git commit -m "test(build): responsive picture rendering over the with-images fixture"
```

---

## Task 6: Figure/figcaption CSS

**Files:**
- Modify: `crates/lopress-theme/assets/default-theme/theme.css`

- [ ] **Step 1: Add styles**

Append to `theme.css`:

```css
figure { margin: 1.5rem 0; }
figure img { max-width: 100%; height: auto; display: block; }
figcaption { font-size: 0.85rem; color: #666; margin-top: 0.35rem; text-align: center; }
```

- [ ] **Step 2: Confirm the theme test still passes**

Run: `cargo test -p lopress-theme default_css_is_non_empty`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-theme/assets/default-theme/theme.css
git commit -m "feat(theme): style figures and captions"
```

---

## Task 7: Full gate

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace` all pass. Fix clippy per `AGENTS.md` (no `unwrap`/`expect`/`panic`, no lossy `as` — note `Variant.width` is `u32` and used in string formatting, no cast needed). Stage formatting changes.

- [ ] **Step 2: Manual build check (optional, recommended)**

Run a real build over a workspace that has an image in `src/images/` and open the generated `posts/<slug>/index.html` to confirm the `<picture>`/`srcset` and that the image displays in a browser via the preview server.

- [ ] **Step 3: Commit any gate fixes**

```bash
git add -A
git commit -m "chore: gate pass for responsive image rendering"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage (§6):** ImageIndex (Task 1), `<picture>` arm + external fallback + figcaption (Task 2), threading (Task 3), build reorder + force-full ordering (Task 4), integration test (Task 5), CSS (Task 6).
- **No editor coupling:** every change is in `lopress-build` / `lopress-theme`. `render_body`'s new `image_index` parameter is the only signature change; Task 3 fixes all callers.
- **Cross-plan:** if read-more's `render_excerpt` exists, add `image_index` to it too (it calls `write_block`).
- **Type consistency:** `ImageIndex`/`ImageEntry`/`ImageVariant` defined in Task 1 are used unchanged in Tasks 2–4; `record` keys by stem everywhere; the `/images/` prefix strip in `write_image` matches the `original`/`filename` values stored by `record`.

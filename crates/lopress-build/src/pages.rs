use crate::cache::{self, PageEntry};
use crate::error::{BuildError, PageFailure};
use crate::render::render_body;
use crate::site::Workspace;
use lopress_core::{parse, Document};
use lopress_plugin::PluginRegistry;
use lopress_theme::{PageCtx, PageKind, PostSummary, RenderContext, SiteCtx, ThemeEngine};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct DiscoveredPost {
    pub source_path: PathBuf,
    pub slug: String,
    pub doc: Document,
}

pub struct RenderStats {
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub post_set_changed: bool,
    pub failures: Vec<PageFailure>,
}

/// Walk `dir` and return all `*.md` files paired with their parsed Document
/// and computed slug. `kind` is only used for error messages.
pub fn discover(
    dir: &Path,
    kind: &str,
) -> Result<(Vec<DiscoveredPost>, Vec<PageFailure>), BuildError> {
    let mut ok = Vec::new();
    let mut failures = Vec::new();
    if !dir.exists() {
        return Ok((ok, failures));
    }
    for entry in WalkDir::new(dir).min_depth(1).max_depth(1) {
        let entry = entry.map_err(std::io::Error::other)?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let src = std::fs::read_to_string(entry.path())?;
        let doc = match parse(&src) {
            Ok(d) => d,
            Err(e) => {
                failures.push(PageFailure {
                    path: entry.path().to_path_buf(),
                    message: format!("{kind} parse: {e}"),
                });
                continue;
            }
        };
        let slug = doc.front_matter.slug.clone().unwrap_or_else(|| {
            entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string()
        });
        ok.push(DiscoveredPost {
            source_path: entry.path().to_path_buf(),
            slug,
            doc,
        });
    }
    Ok((ok, failures))
}

/// Build the list of PostSummary objects used by index/tag templates and feed.
pub fn post_summaries(posts: &[DiscoveredPost], _base_url: &str) -> Vec<PostSummary> {
    let mut out: Vec<PostSummary> = posts
        .iter()
        .filter(|p| !p.doc.front_matter.draft)
        .map(|p| {
            let slug = p.slug.clone();
            let url = format!("/posts/{slug}/");
            PostSummary {
                title: p
                    .doc
                    .front_matter
                    .title
                    .clone()
                    .unwrap_or_else(|| slug.clone()),
                slug,
                url,
                date: p.doc.front_matter.date,
                tags: p.doc.front_matter.tags.clone(),
                description: p.doc.front_matter.description.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    out
}

/// Render every post/page into www/ via the theme engine, consulting the
/// cache to skip unchanged files. Returns per-file stats.
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
) -> Result<RenderStats, BuildError> {
    let www = workspace.www_dir();
    std::fs::create_dir_all(&www)?;
    let summaries = post_summaries(posts, &workspace.config.site.base_url);

    let site_ctx = SiteCtx {
        title: workspace.config.site.title.clone(),
        base_url: workspace.config.site.base_url.clone(),
        nav: workspace
            .config
            .site
            .nav
            .items
            .iter()
            .map(|n| lopress_theme::NavItem {
                label: n.label.clone(),
                href: n.href.clone(),
            })
            .collect(),
        posts: summaries.clone(),
    };

    let mut failures = Vec::new();
    let mut pages_rendered: usize = 0;
    let mut pages_skipped: usize = 0;
    // Flip whenever a post/page's aggregate-visible metadata changes or a
    // new/removed source is detected — drives index/feed/sitemap/tag regen.
    let mut post_set_changed = false;

    // Build the set of live keys (all posts + pages, including drafts) for
    // orphan detection.
    let mut live_keys: BTreeSet<String> = BTreeSet::new();

    // --- Posts ---
    for p in posts {
        let key = cache::rel_key(&workspace.root, &p.source_path);
        live_keys.insert(key.clone());

        let is_draft = p.doc.front_matter.draft;
        let source_hash = cache::hash_file(&p.source_path)?;
        let new_outputs = if is_draft {
            vec![]
        } else {
            vec![format!("posts/{}/index.html", p.slug)]
        };
        let old = cache.pages.get(&key).cloned();

        if is_draft {
            // Don't render body, but clean up any prior outputs so a
            // published-then-drafted post doesn't keep being served.
            if let Some(ref old) = old {
                remove_stale_outputs(&www, &old.outputs, &new_outputs);
            }
            let new_entry = build_entry(source_hash, new_outputs, is_draft, &p.doc.front_matter);
            if aggregate_metadata_changed(old.as_ref(), &new_entry) {
                post_set_changed = true;
            }
            cache.pages.insert(key, new_entry);
            continue;
        }

        let should_skip = !force_full
            && old.as_ref().is_some_and(|e| {
                e.source_hash == source_hash
                    && !e.is_draft
                    && e.outputs == new_outputs
                    && e.outputs.iter().all(|o| www.join(o).exists())
            });

        if should_skip {
            pages_skipped += 1;
        } else {
            match render_one_post(&www, &site_ctx, p, registry, theme, tera_shared) {
                Ok(()) => {
                    if let Some(ref old) = old {
                        remove_stale_outputs(&www, &old.outputs, &new_outputs);
                    }
                    let new_entry =
                        build_entry(source_hash, new_outputs, is_draft, &p.doc.front_matter);
                    if aggregate_metadata_changed(old.as_ref(), &new_entry) {
                        post_set_changed = true;
                    }
                    cache.pages.insert(key, new_entry);
                    pages_rendered += 1;
                }
                Err(e) => {
                    failures.push(PageFailure {
                        path: p.source_path.clone(),
                        message: e.to_string(),
                    });
                }
            }
        }
    }

    // --- Pages ---
    for p in pages {
        let key = cache::rel_key(&workspace.root, &p.source_path);
        live_keys.insert(key.clone());

        let is_draft = p.doc.front_matter.draft;
        let source_hash = cache::hash_file(&p.source_path)?;
        let new_outputs = if is_draft {
            vec![]
        } else {
            vec![format!("{}/index.html", p.slug)]
        };
        let old = cache.pages.get(&key).cloned();

        if is_draft {
            if let Some(ref old) = old {
                remove_stale_outputs(&www, &old.outputs, &new_outputs);
            }
            let new_entry = build_entry(source_hash, new_outputs, is_draft, &p.doc.front_matter);
            if aggregate_metadata_changed(old.as_ref(), &new_entry) {
                post_set_changed = true;
            }
            cache.pages.insert(key, new_entry);
            continue;
        }

        let should_skip = !force_full
            && old.as_ref().is_some_and(|e| {
                e.source_hash == source_hash
                    && !e.is_draft
                    && e.outputs == new_outputs
                    && e.outputs.iter().all(|o| www.join(o).exists())
            });

        if should_skip {
            pages_skipped += 1;
        } else {
            match render_one_page(&www, &site_ctx, p, registry, theme, tera_shared) {
                Ok(()) => {
                    if let Some(ref old) = old {
                        remove_stale_outputs(&www, &old.outputs, &new_outputs);
                    }
                    let new_entry =
                        build_entry(source_hash, new_outputs, is_draft, &p.doc.front_matter);
                    if aggregate_metadata_changed(old.as_ref(), &new_entry) {
                        post_set_changed = true;
                    }
                    cache.pages.insert(key, new_entry);
                    pages_rendered += 1;
                }
                Err(e) => {
                    failures.push(PageFailure {
                        path: p.source_path.clone(),
                        message: e.to_string(),
                    });
                }
            }
        }
    }

    let pruned_anything = prune_orphans(workspace, cache, &live_keys)?;
    if pruned_anything {
        post_set_changed = true;
    }

    Ok(RenderStats {
        pages_rendered,
        pages_skipped,
        post_set_changed,
        failures,
    })
}

fn build_entry(
    source_hash: String,
    outputs: Vec<String>,
    is_draft: bool,
    fm: &lopress_core::FrontMatter,
) -> PageEntry {
    PageEntry {
        source_hash,
        outputs,
        tags: fm.tags.clone(),
        is_draft,
        title: fm.title.clone(),
        date: fm.date.map(|d| d.to_string()),
    }
}

/// Delete files listed in `old` that are not in `new`. Used when a
/// post/page's slug changes or it transitions to draft, so stale HTML
/// doesn't continue to be served.
fn remove_stale_outputs(www: &Path, old: &[String], new: &[String]) {
    for output in old {
        if new.iter().any(|n| n == output) {
            continue;
        }
        let p = www.join(output);
        let _ = std::fs::remove_file(&p);
        if let Some(parent) = p.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

/// Aggregate pages (index/feed/sitemap/tags) depend on the post set's
/// visible metadata — not the body. Compare the fields that surface in
/// those views so draft flips, slug/title/date/tag edits, and new/removed
/// entries trigger regeneration.
fn aggregate_metadata_changed(old: Option<&PageEntry>, new: &PageEntry) -> bool {
    let Some(old) = old else { return true };
    old.is_draft != new.is_draft
        || old.outputs != new.outputs
        || old.tags != new.tags
        || old.title != new.title
        || old.date != new.date
}

/// Remove cache entries (and their output files) for source files that no
/// longer exist. Returns `true` if anything was pruned.
pub fn prune_orphans(
    workspace: &Workspace,
    cache: &mut crate::cache::BuildCache,
    live_keys: &BTreeSet<String>,
) -> Result<bool, BuildError> {
    let stale: Vec<String> = cache
        .pages
        .keys()
        .filter(|k| !live_keys.contains(*k))
        .cloned()
        .collect();
    let changed = !stale.is_empty();
    for key in stale {
        if let Some(entry) = cache.pages.remove(&key) {
            for output in &entry.outputs {
                let p = workspace.www_dir().join(output);
                let _ = std::fs::remove_file(&p);
                if let Some(parent) = p.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
    }
    Ok(changed)
}

pub fn render_one_post(
    www: &Path,
    site: &SiteCtx,
    post: &DiscoveredPost,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
) -> Result<(), BuildError> {
    let body_html = render_body(&post.doc, registry, tera_shared)?;
    let slug = &post.slug;
    let url = format!("/posts/{slug}/");
    let canonical = join_url(&site.base_url, &url);
    let page = PageCtx {
        kind: PageKind::Post,
        title: post
            .doc
            .front_matter
            .title
            .clone()
            .unwrap_or_else(|| slug.clone()),
        slug: slug.clone(),
        url,
        canonical,
        description: post.doc.front_matter.description.clone(),
        og_image: post.doc.front_matter.image.clone(),
        date: post.doc.front_matter.date,
        tags: post.doc.front_matter.tags.clone(),
        body_html,
        posts: vec![],
        tag: None,
    };
    let html = theme.render("post.html", &RenderContext { site, page: &page })?;
    write_page(www, &format!("posts/{slug}"), &html)
}

pub fn render_one_page(
    www: &Path,
    site: &SiteCtx,
    p: &DiscoveredPost,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
) -> Result<(), BuildError> {
    let body_html = render_body(&p.doc, registry, tera_shared)?;
    let slug = &p.slug;
    let url = format!("/{slug}/");
    let canonical = join_url(&site.base_url, &url);
    let page = PageCtx {
        kind: PageKind::Page,
        title: p
            .doc
            .front_matter
            .title
            .clone()
            .unwrap_or_else(|| slug.clone()),
        slug: slug.clone(),
        url,
        canonical,
        description: p.doc.front_matter.description.clone(),
        og_image: p.doc.front_matter.image.clone(),
        date: p.doc.front_matter.date,
        tags: p.doc.front_matter.tags.clone(),
        body_html,
        posts: vec![],
        tag: None,
    };
    let html = theme.render("page.html", &RenderContext { site, page: &page })?;
    write_page(www, slug, &html)
}

pub fn render_index(www: &Path, site: &SiteCtx, theme: &ThemeEngine) -> Result<(), BuildError> {
    let page = PageCtx {
        kind: PageKind::Index,
        title: site.title.clone(),
        slug: String::new(),
        url: "/".into(),
        canonical: join_url(&site.base_url, "/"),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: site.posts.clone(),
        tag: None,
    };
    let html = theme.render("index.html", &RenderContext { site, page: &page })?;
    std::fs::write(www.join("index.html"), html)?;
    Ok(())
}

pub fn render_tag(
    www: &Path,
    site: &SiteCtx,
    tag: &str,
    posts: &[PostSummary],
    theme: &ThemeEngine,
) -> Result<(), BuildError> {
    let url = format!("/tags/{tag}/");
    let page = PageCtx {
        kind: PageKind::Tag,
        title: format!("Tagged: {tag}"),
        slug: tag.to_string(),
        url: url.clone(),
        canonical: join_url(&site.base_url, &url),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: posts.to_vec(),
        tag: Some(tag.to_string()),
    };
    let html = theme.render("tag.html", &RenderContext { site, page: &page })?;
    write_page(www, &format!("tags/{tag}"), &html)
}

/// Build a tag → posts map from a summaries slice.
pub fn build_tag_map(summaries: &[PostSummary]) -> BTreeMap<String, Vec<PostSummary>> {
    let mut by_tag: BTreeMap<String, Vec<PostSummary>> = BTreeMap::new();
    for s in summaries {
        for t in &s.tags {
            by_tag.entry(t.clone()).or_default().push(s.clone());
        }
    }
    by_tag
}

fn write_page(www: &Path, rel_dir: &str, html: &str) -> Result<(), BuildError> {
    let dir = www.join(rel_dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("index.html"), html)?;
    Ok(())
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}{path}")
}

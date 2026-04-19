use crate::error::{BuildError, PageFailure};
use crate::render::render_body;
use crate::site::Workspace;
use lopress_core::{parse, Document};
use lopress_plugin::PluginRegistry;
use lopress_theme::{PageCtx, PageKind, PostSummary, RenderContext, SiteCtx, ThemeEngine};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct DiscoveredPost {
    pub source_path: PathBuf,
    pub slug: String,
    pub doc: Document,
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

/// Render every post/page into www/ via the theme engine. Returns page
/// failures; successful pages have already been written.
pub fn render_all(
    workspace: &Workspace,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
    posts: &[DiscoveredPost],
    pages: &[DiscoveredPost],
) -> Result<Vec<PageFailure>, BuildError> {
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

    for p in posts {
        if p.doc.front_matter.draft {
            continue;
        }
        if let Err(e) = render_one_post(&www, &site_ctx, p, registry, theme, tera_shared) {
            failures.push(PageFailure {
                path: p.source_path.clone(),
                message: e.to_string(),
            });
        }
    }

    for p in pages {
        if let Err(e) = render_one_page(&www, &site_ctx, p, registry, theme, tera_shared) {
            failures.push(PageFailure {
                path: p.source_path.clone(),
                message: e.to_string(),
            });
        }
    }

    if let Err(e) = render_index(&www, &site_ctx, theme) {
        failures.push(PageFailure {
            path: www.join("index.html"),
            message: e.to_string(),
        });
    }

    let mut by_tag: BTreeMap<String, Vec<PostSummary>> = BTreeMap::new();
    for s in &summaries {
        for t in &s.tags {
            by_tag.entry(t.clone()).or_default().push(s.clone());
        }
    }
    for (tag, tag_posts) in by_tag {
        if let Err(e) = render_tag(&www, &site_ctx, &tag, &tag_posts, theme) {
            failures.push(PageFailure {
                path: www.join(format!("tags/{tag}/index.html")),
                message: e.to_string(),
            });
        }
    }

    Ok(failures)
}

fn render_one_post(
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

fn render_one_page(
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

fn render_index(
    www: &Path,
    site: &SiteCtx,
    theme: &ThemeEngine,
) -> Result<(), BuildError> {
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

fn render_tag(
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

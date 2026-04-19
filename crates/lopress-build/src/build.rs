use crate::error::{BuildError, PageFailure};
use crate::feed;
use crate::not_found;
use crate::pages;
use crate::robots;
use crate::site::Workspace;
use crate::sitemap;
use lopress_assets::{process_image, VariantCache, VariantSpec};
use lopress_plugin::load_dir;
use lopress_theme::{resolve, SiteCtx};
use std::path::Path;
use tera::Tera;

pub struct BuildReport {
    pub pages_written: usize,
    pub failures: Vec<PageFailure>,
}

pub fn build(workspace: &Path) -> Result<BuildReport, BuildError> {
    let ws = Workspace::load(workspace)?;
    std::fs::create_dir_all(ws.www_dir())?;

    // Plugins
    let registry = load_dir(
        &ws.plugins_dir(),
        if ws.config.plugins.enabled.is_empty() {
            None
        } else {
            Some(ws.config.plugins.enabled.as_slice())
        },
    )?;

    // Theme
    let theme = resolve(&registry, &ws.config.site.theme)?;

    // Build a shared Tera that knows every theme template and every plugin
    // block template, namespaced by plugin name.
    let mut tera = Tera::default();
    for (name, content) in theme_templates(&ws, &theme)? {
        tera.add_raw_template(&name, &content)
            .map_err(|e| BuildError::Config(format!("theme template `{name}`: {e}")))?;
    }
    for plugin in &registry.plugins {
        for block in &plugin.manifest.blocks {
            let plugin_name = &plugin.manifest.name;
            let template = &block.template;
            let key = format!("{plugin_name}::{template}");
            let src = std::fs::read_to_string(plugin.root.join(&block.template))?;
            tera.add_raw_template(&key, &src)
                .map_err(|e| BuildError::Config(format!("plugin template `{key}`: {e}")))?;
        }
    }

    // Discover content
    let (posts, mut failures) = pages::discover(&ws.posts_dir(), "post")?;
    let (pages_src, page_failures) = pages::discover(&ws.pages_dir(), "page")?;
    failures.extend(page_failures);

    let summaries = pages::post_summaries(&posts, &ws.config.site.base_url);

    let site_ctx = SiteCtx {
        title: ws.config.site.title.clone(),
        base_url: ws.config.site.base_url.clone(),
        nav: ws
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

    let render_failures =
        pages::render_all(&ws, &registry, &theme.engine, &tera, &posts, &pages_src)?;
    failures.extend(render_failures);

    feed::write(&ws.www_dir(), &site_ctx)?;
    let page_urls: Vec<String> = pages_src
        .iter()
        .map(|p| {
            let slug = &p.slug;
            format!("/{slug}/")
        })
        .collect();
    let tag_urls: Vec<String> = tag_urls_for(&summaries);
    sitemap::write(&ws.www_dir(), &site_ctx, &page_urls, &tag_urls)?;
    robots::write(&ws.www_dir(), &ws.config)?;
    not_found::write(&ws.www_dir(), &site_ctx, &theme.engine)?;

    write_theme_css(&ws, &theme)?;

    for plugin in &registry.plugins {
        let assets = plugin.root.join("assets");
        if assets.exists() {
            let target = ws.www_dir().join("assets").join(&plugin.manifest.name);
            copy_dir(&assets, &target)?;
        }
    }

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
            if let Err(e) = process_image(entry.path(), &www_images, &mut img_cache, &spec) {
                failures.push(PageFailure {
                    path: entry.path().to_path_buf(),
                    message: format!("image: {e}"),
                });
            }
        }
    }
    img_cache.save(&ws.www_dir().join(".lopress-image-cache.json"))?;

    let pages_written = posts.iter().filter(|p| !p.doc.front_matter.draft).count()
        + pages_src.len()
        + tag_urls.len()
        + 1;

    Ok(BuildReport {
        pages_written,
        failures,
    })
}

fn theme_templates(
    _ws: &Workspace,
    theme: &lopress_theme::ResolvedTheme,
) -> Result<Vec<(String, String)>, BuildError> {
    let mut out = Vec::new();
    if let Some(css_path) = &theme.css_path {
        let templates_dir = css_path.parent().unwrap().join("templates");
        for entry in std::fs::read_dir(templates_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("html") {
                let name = entry.path().file_name().unwrap().to_string_lossy().into();
                let contents = std::fs::read_to_string(entry.path())?;
                out.push((name, contents));
            }
        }
    } else {
        for name in [
            "layout.html",
            "post.html",
            "page.html",
            "index.html",
            "tag.html",
            "404.html",
        ] {
            if let Some(src) = lopress_theme::builtin_template(name) {
                out.push((name.into(), src.into()));
            }
        }
    }
    Ok(out)
}

fn write_theme_css(ws: &Workspace, theme: &lopress_theme::ResolvedTheme) -> Result<(), BuildError> {
    let target = ws.www_dir().join("assets").join("theme.css");
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::write(target, &theme.css_content)?;
    Ok(())
}

fn tag_urls_for(posts: &[lopress_theme::PostSummary]) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut tags = BTreeSet::new();
    for p in posts {
        for t in &p.tags {
            tags.insert(t.clone());
        }
    }
    tags.into_iter().map(|t| format!("/tags/{t}/")).collect()
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), BuildError> {
    std::fs::create_dir_all(to)?;
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.map_err(std::io::Error::other)?;
        let rel = entry.path().strip_prefix(from).unwrap();
        let dst = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dst)?;
        } else {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dst)?;
        }
    }
    Ok(())
}

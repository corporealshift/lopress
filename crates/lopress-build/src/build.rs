use crate::cache::{self, BuildCache};
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
    pub pages_rendered: usize,
    pub pages_skipped: usize,
    pub failures: Vec<PageFailure>,
}

pub fn build(workspace: &Path) -> Result<BuildReport, BuildError> {
    let ws = Workspace::load(workspace)?;

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

    // Load cache and compute global hashes
    let mut build_cache = BuildCache::load(&ws.cache_path())?;
    let cfg_hash = cache::hash_config(&ws)?;
    let theme_hash = cache::hash_theme(&theme)?;
    let plugins_hash = cache::hash_plugins(&registry)?;

    let force_full = build_cache.config_hash != cfg_hash
        || build_cache.theme_hash != theme_hash
        || build_cache.plugins_hash != plugins_hash;

    // On a forced full rebuild: wipe page cache entries and clear www/
    if force_full {
        build_cache.pages.clear();
        if ws.www_dir().exists() {
            for entry in std::fs::read_dir(ws.www_dir())? {
                let entry = entry?;
                let name = entry.file_name();
                if name == ".lopress-image-cache.json" {
                    continue;
                }
                let p = entry.path();
                if p.is_dir() {
                    std::fs::remove_dir_all(&p)?;
                } else {
                    std::fs::remove_file(&p)?;
                }
            }
        }
        std::fs::create_dir_all(ws.www_dir())?;
    }

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

            // Register HTML template
            if let Some(template) = &block.template {
                let key = format!("{plugin_name}::{template}");
                let src = std::fs::read_to_string(plugin.root.join(template))?;
                tera.add_raw_template(&key, &src)
                    .map_err(|e| BuildError::Config(format!("plugin template `{key}`: {e}")))?;
            }

            // Register markdown template
            if let Some(md_template) = &block.markdown_template {
                let key = format!("{plugin_name}::{md_template}");
                let src = std::fs::read_to_string(plugin.root.join(md_template))?;
                tera.add_raw_template(&key, &src).map_err(|e| {
                    BuildError::Config(format!("plugin markdown template `{key}`: {e}"))
                })?;
            }
        }
    }

    // Discover content
    let (posts, mut failures) = pages::discover(&ws.posts_dir(), "post")?;
    let (pages_src, page_failures) = pages::discover(&ws.pages_dir(), "page")?;
    failures.extend(page_failures);

    // Image pipeline — run before rendering so the renderer can emit a
    // correct responsive srcset. Has its own per-file cache.
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

    // Render posts and pages (cache-aware)
    let stats = pages::render_all(
        &ws,
        &registry,
        &theme.engine,
        &tera,
        &posts,
        &pages_src,
        &mut build_cache,
        force_full,
        &image_index,
    )?;
    failures.extend(stats.failures.iter().cloned());

    // Build summaries for aggregate pages
    let summaries = pages::post_summaries(&posts, &registry, &tera, &image_index);

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

    // Regenerate aggregate pages only when content changed or forced
    let regen_aggregates = force_full || stats.post_set_changed;
    let tag_map = pages::build_tag_map(&summaries);
    let tag_count = tag_map.len();

    if regen_aggregates {
        feed::write(&ws.www_dir(), &site_ctx)?;
        let page_urls: Vec<String> = pages_src
            .iter()
            .filter(|p| !p.doc.front_matter.draft)
            .map(|p| {
                let slug = &p.slug;
                format!("/{slug}/")
            })
            .collect();
        let tag_urls: Vec<String> = tag_urls_for(&summaries);
        sitemap::write(&ws.www_dir(), &site_ctx, &page_urls, &tag_urls)?;
        robots::write(&ws.www_dir(), &ws.config)?;
        not_found::write(&ws.www_dir(), &site_ctx, &theme.engine)?;

        if let Err(e) = pages::render_index(&ws.www_dir(), &site_ctx, &theme.engine) {
            failures.push(PageFailure {
                path: ws.www_dir().join("index.html"),
                message: e.to_string(),
            });
        }

        // Tag archives are regenerated wholesale: wipe `www/tags/` first so
        // archives for tags that no longer exist (last post removed or
        // retagged) don't linger. `tag_map` below recreates the surviving
        // ones.
        let tags_dir = ws.www_dir().join("tags");
        if tags_dir.exists() {
            std::fs::remove_dir_all(&tags_dir)?;
        }

        for (tag, tag_posts) in &tag_map {
            if let Err(e) =
                pages::render_tag(&ws.www_dir(), &site_ctx, tag, tag_posts, &theme.engine)
            {
                failures.push(PageFailure {
                    path: ws.www_dir().join(format!("tags/{tag}/index.html")),
                    message: e.to_string(),
                });
            }
        }
    }

    // Theme assets: only on full rebuild
    if force_full {
        write_theme_css(&ws, &theme)?;
        for plugin in &registry.plugins {
            let assets = plugin.root.join("assets");
            if assets.exists() {
                let target = ws.www_dir().join("assets").join(&plugin.manifest.name);
                copy_dir(&assets, &target)?;
            }
        }
    }

    // Update and persist cache
    build_cache.config_hash = cfg_hash;
    build_cache.theme_hash = theme_hash;
    build_cache.plugins_hash = plugins_hash;
    build_cache.save(&ws.cache_path())?;

    // Count pages_written: non-draft content pages + tag archives + index
    let pages_written = build_cache
        .pages
        .values()
        .filter(|e| !e.is_draft)
        .map(|e| e.outputs.len())
        .sum::<usize>()
        + tag_count
        + 1; // index.html

    Ok(BuildReport {
        pages_written,
        pages_rendered: stats.pages_rendered,
        pages_skipped: stats.pages_skipped,
        failures,
    })
}

fn theme_templates(
    _ws: &Workspace,
    theme: &lopress_theme::ResolvedTheme,
) -> Result<Vec<(String, String)>, BuildError> {
    let mut out = Vec::new();
    if let Some(css_path) = &theme.css_path {
        let Some(css_parent) = css_path.parent() else {
            return Err(BuildError::Config(format!(
                "theme css path has no parent: {}",
                css_path.display()
            )));
        };
        let templates_dir = css_parent.join("templates");
        for entry in std::fs::read_dir(templates_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("html") {
                let Some(name_os) = entry.path().file_name().map(|s| s.to_owned()) else {
                    continue;
                };
                let name = name_os.to_string_lossy().into_owned();
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
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
        let Ok(rel) = entry.path().strip_prefix(from) else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest, PluginRegistry};

    #[test]
    fn markdown_template_field_is_accessible() {
        // Minimal test: the struct compiles with the new field and it's
        // readable. Full Tera registration coverage lives in Task 3.
        let mut reg = PluginRegistry::default();
        reg.insert(LoadedPlugin {
            root: std::path::PathBuf::from("/does/not/exist"),
            manifest: PluginManifest {
                name: "demo".into(),
                version: "0.1.0".into(),
                theme: false,
                blocks: vec![BlockDecl {
                    name: "lopress:demo".into(),
                    template: None,
                    markdown_template: Some("blocks/demo.md".into()),
                    attrs: Default::default(),
                    editor: None,
                    builtin: false,
                    native: None,
                    css: Vec::new(),
                    js: Vec::new(),
                    title: None,
                    description: None,
                    category: None,
                }],
            },
        })
        .unwrap();
        let block = &reg.plugins[0].manifest.blocks[0];
        assert_eq!(block.markdown_template.as_deref(), Some("blocks/demo.md"));
    }
}

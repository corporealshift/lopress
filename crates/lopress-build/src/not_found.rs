use crate::error::BuildError;
use lopress_theme::{PageCtx, PageKind, RenderContext, SiteCtx, ThemeEngine};
use std::path::Path;

pub fn write(www: &Path, site: &SiteCtx, theme: &ThemeEngine) -> Result<(), BuildError> {
    let base = site.base_url.trim_end_matches('/');
    let page = PageCtx {
        kind: PageKind::NotFound,
        title: "Not found".into(),
        slug: "404".into(),
        url: "/404.html".into(),
        canonical: format!("{base}/404.html"),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: vec![],
        tag: None,
    };
    let html = theme.render("404.html", &RenderContext { site, page: &page })?;
    std::fs::write(www.join("404.html"), html)?;
    Ok(())
}

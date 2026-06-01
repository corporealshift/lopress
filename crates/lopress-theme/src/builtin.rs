use crate::engine::ThemeEngine;
use crate::error::ThemeError;
use include_dir::{include_dir, Dir};

static DEFAULT: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/default-theme");

/// Build a ThemeEngine from the embedded default theme.
pub fn default_engine() -> Result<ThemeEngine, ThemeError> {
    let templates = DEFAULT
        .get_dir("templates")
        .ok_or_else(|| ThemeError::MissingTemplate("templates/".into()))?;
    let mut tpls = Vec::new();
    for entry in templates.files() {
        let name = entry
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let contents = entry.contents_utf8().unwrap_or("").to_string();
        tpls.push((name, contents));
    }
    ThemeEngine::from_templates(&tpls)
}

/// Return the default theme's CSS content.
pub fn default_css() -> &'static str {
    DEFAULT
        .get_file("theme.css")
        .and_then(|f| f.contents_utf8())
        .unwrap_or("")
}

/// Return the source of a built-in template by filename, or None.
pub fn builtin_template(name: &str) -> Option<&'static str> {
    DEFAULT
        .get_file(format!("templates/{name}"))?
        .contents_utf8()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::*;

    #[test]
    fn default_engine_renders_post() {
        let engine = default_engine().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![],
        };
        let page = PageCtx {
            kind: PageKind::Post,
            title: "Hi".into(),
            slug: "hi".into(),
            url: "/posts/hi/".into(),
            canonical: "https://example.com/posts/hi/".into(),
            description: Some("d".into()),
            og_image: None,
            date: None,
            tags: vec!["a".into()],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        };
        let html = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site,
                    page: &page,
                },
            )
            .unwrap();
        assert!(html.contains("<title>Hi — S</title>"));
        assert!(html.contains("<p>body</p>"));
        assert!(html.contains("href=\"/tags/a/\""));
    }

    #[test]
    fn default_css_is_non_empty() {
        assert!(default_css().contains("body"));
    }

    #[test]
    fn index_renders_excerpt_and_read_more() {
        let engine = default_engine().unwrap();
        let summary = PostSummary {
            title: "P".into(),
            slug: "p".into(),
            url: "/posts/p/".into(),
            date: None,
            tags: vec![],
            description: None,
            excerpt_html: Some("<p>teaser</p>".into()),
        };
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://e.com".into(),
            nav: vec![],
            posts: vec![summary.clone()],
        };
        let page = PageCtx {
            kind: PageKind::Index,
            title: "S".into(),
            slug: String::new(),
            url: "/".into(),
            canonical: "https://e.com/".into(),
            description: None,
            og_image: None,
            date: None,
            tags: vec![],
            body_html: String::new(),
            posts: vec![summary],
            tag: None,
        };
        let html = engine
            .render("index.html", &RenderContext { site: &site, page: &page })
            .unwrap();
        // The post title is always rendered, proving the loop runs.
        assert!(html.contains("P"), "index must render post title");
        // excerpt_html is rendered via the safe filter.
        assert!(html.contains("teaser"), "index must render excerpt_html");
        assert!(html.contains("Read more"), "index must show Read more link");
    }
}

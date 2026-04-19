use crate::context::RenderContext;
use crate::error::ThemeError;
use tera::Tera;

pub struct ThemeEngine {
    tera: Tera,
}

impl ThemeEngine {
    pub fn from_templates(templates: &[(String, String)]) -> Result<Self, ThemeError> {
        let mut tera = Tera::default();
        tera.add_raw_templates(
            templates
                .iter()
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .collect::<Vec<_>>(),
        )?;
        Ok(Self { tera })
    }

    pub fn render(&self, template: &str, ctx: &RenderContext) -> Result<String, ThemeError> {
        let mut t = tera::Context::new();
        t.insert("site", ctx.site);
        t.insert("page", ctx.page);
        self.tera.render(template, &t).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::*;

    fn site() -> SiteCtx {
        SiteCtx {
            title: "T".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![],
        }
    }

    fn page() -> PageCtx {
        PageCtx {
            kind: PageKind::Post,
            title: "P".into(),
            slug: "p".into(),
            url: "/posts/p/".into(),
            canonical: "https://example.com/posts/p/".into(),
            description: None,
            og_image: None,
            date: None,
            tags: vec![],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        }
    }

    #[test]
    fn renders_minimal_template() {
        let engine = ThemeEngine::from_templates(&[(
            "post.html".into(),
            "<title>{{ page.title }}</title>{{ page.body_html | safe }}".into(),
        )])
        .unwrap();
        let s = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site(),
                    page: &page(),
                },
            )
            .unwrap();
        assert_eq!(s, "<title>P</title><p>body</p>");
    }
}

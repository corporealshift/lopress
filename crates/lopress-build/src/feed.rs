use crate::error::BuildError;
use lopress_theme::{PostSummary, SiteCtx};
use std::fmt::Write;
use std::path::Path;

/// Write `www/feed.xml` — Atom feed of non-draft posts in reverse-chron order.
pub fn write(www: &Path, site: &SiteCtx) -> Result<(), BuildError> {
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    buf.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    let title = escape(&site.title);
    let _ = writeln!(buf, "  <title>{title}</title>");
    let base = site.base_url.trim_end_matches('/');
    let _ = writeln!(buf, "  <link href=\"{base}/feed.xml\" rel=\"self\"/>");
    let _ = writeln!(buf, "  <link href=\"{base}/\" />");
    let _ = writeln!(buf, "  <id>{base}/</id>");
    if let Some(latest) = site.posts.first().and_then(|p| p.date) {
        let _ = writeln!(buf, "  <updated>{latest}T00:00:00Z</updated>");
    }
    for p in &site.posts {
        entry(&mut buf, p, &site.base_url);
    }
    buf.push_str("</feed>\n");
    std::fs::write(www.join("feed.xml"), buf)?;
    Ok(())
}

fn entry(buf: &mut String, p: &PostSummary, base_url: &str) {
    let base = base_url.trim_end_matches('/');
    let path = &p.url;
    let url = format!("{base}{path}");
    buf.push_str("  <entry>\n");
    let title = escape(&p.title);
    let _ = writeln!(buf, "    <title>{title}</title>");
    let _ = writeln!(buf, "    <link href=\"{url}\"/>");
    let _ = writeln!(buf, "    <id>{url}</id>");
    if let Some(d) = p.date {
        let _ = writeln!(buf, "    <updated>{d}T00:00:00Z</updated>");
    }
    if let Some(desc) = &p.description {
        let esc = escape(desc);
        let _ = writeln!(buf, "    <summary>{esc}</summary>");
    }
    buf.push_str("  </entry>\n");
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use lopress_theme::PostSummary;
    use tempfile::TempDir;

    #[test]
    fn writes_atom_feed_with_entries() {
        let d = TempDir::new().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            favicon: None,
            posts: vec![PostSummary {
                title: "Hi".into(),
                slug: "hi".into(),
                url: "/posts/hi/".into(),
                date: Some(NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()),
                tags: vec![],
                description: Some("d".into()),
                excerpt_html: None,
            }],
        };
        write(d.path(), &site).unwrap();
        let s = std::fs::read_to_string(d.path().join("feed.xml")).unwrap();
        assert!(s.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(s.contains("https://example.com/posts/hi/"));
        assert!(s.contains("<title>Hi</title>"));
    }
}

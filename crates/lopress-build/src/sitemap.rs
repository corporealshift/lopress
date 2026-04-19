use crate::error::BuildError;
use lopress_theme::SiteCtx;
use std::fmt::Write;
use std::path::Path;

pub fn write(
    www: &Path,
    site: &SiteCtx,
    page_urls: &[String],
    tag_urls: &[String],
) -> Result<(), BuildError> {
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    buf.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    let base = site.base_url.trim_end_matches('/');

    let _ = writeln!(buf, "  <url><loc>{base}/</loc></url>");
    for p in &site.posts {
        let u = &p.url;
        let _ = writeln!(buf, "  <url><loc>{base}{u}</loc></url>");
    }
    for u in page_urls {
        let _ = writeln!(buf, "  <url><loc>{base}{u}</loc></url>");
    }
    for u in tag_urls {
        let _ = writeln!(buf, "  <url><loc>{base}{u}</loc></url>");
    }
    buf.push_str("</urlset>\n");
    std::fs::write(www.join("sitemap.xml"), buf)?;
    Ok(())
}

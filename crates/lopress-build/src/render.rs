use crate::error::BuildError;
use crate::image_index::ImageIndex;
use lopress_core::{Block, Document};
use lopress_plugin::PluginRegistry;
use std::fmt::Write;
use std::path::Path;
use tera::Tera;

/// Render the body of a Document into HTML. `tera` may be shared with the
/// theme engine but must also know the plugin templates (the builder inserts
/// them at startup).
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

/// Render the blocks that precede the first `lopress:more` marker to HTML.
/// Returns `None` when the document has no marker.
pub fn render_excerpt(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
    image_index: &ImageIndex,
) -> Result<Option<String>, BuildError> {
    if !doc.blocks.iter().any(|b| b.r#type == "lopress:more") {
        return Ok(None);
    }
    let mut out = String::new();
    for b in &doc.blocks {
        if b.r#type == "lopress:more" {
            break;
        }
        write_block(&mut out, b, registry, tera, image_index)?;
    }
    Ok(Some(out))
}

fn write_block(
    out: &mut String,
    b: &Block,
    registry: &PluginRegistry,
    tera: &Tera,
    image_index: &ImageIndex,
) -> Result<(), BuildError> {
    match b.r#type.as_str() {
        "paragraph" => {
            let text = escape(b.text.as_deref().unwrap_or(""));
            let _ = writeln!(out, "<p>{text}</p>");
        }
        "heading" => {
            let level = b.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1);
            let text = escape(b.text.as_deref().unwrap_or(""));
            let _ = writeln!(out, "<h{level}>{text}</h{level}>");
        }
        "quote" => {
            out.push_str("<blockquote>\n");
            for c in &b.children {
                write_block(out, c, registry, tera, image_index)?;
            }
            out.push_str("</blockquote>\n");
        }
        "code" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            let class = if lang.is_empty() {
                String::new()
            } else {
                let l = escape(lang);
                format!(" class=\"language-{l}\"")
            };
            let text = escape(b.text.as_deref().unwrap_or(""));
            let _ = writeln!(out, "<pre><code{class}>{text}</code></pre>");
        }
        "list" => {
            let ordered = b
                .attrs
                .get("ordered")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let tag = if ordered { "ol" } else { "ul" };
            let _ = writeln!(out, "<{tag}>");
            for item in &b.children {
                out.push_str("<li>");
                for c in &item.children {
                    write_block(out, c, registry, tera, image_index)?;
                }
                out.push_str("</li>\n");
            }
            let _ = writeln!(out, "</{tag}>");
        }
        "image" => {
            write_image(out, b, image_index);
        }
        "lopress:more" => {
            // The read-more marker is invisible on the full page; the excerpt
            // boundary is handled by `render_excerpt`.
        }
        custom if custom.starts_with("lopress:") => {
            render_custom(out, b, custom, registry, tera, image_index)?;
        }
        other => {
            let o = escape(other);
            let _ = writeln!(out, "<!-- unknown block: {o} -->");
        }
    }
    Ok(())
}

fn write_image(out: &mut String, b: &Block, image_index: &ImageIndex) {
    let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
    let alt = escape(b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or(""));
    let caption = b
        .attrs
        .get("caption")
        .and_then(|v| v.as_str())
        .unwrap_or("");

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

fn render_custom(
    out: &mut String,
    b: &Block,
    full_name: &str,
    registry: &PluginRegistry,
    tera: &Tera,
    image_index: &ImageIndex,
) -> Result<(), BuildError> {
    let Some((plugin, decl)) = registry.block(full_name) else {
        let n = escape(full_name);
        let _ = writeln!(out, "<!-- missing plugin for {n} -->");
        return Ok(());
    };
    // Render inner children first.
    let mut inner_html = String::new();
    for c in &b.children {
        write_block(&mut inner_html, c, registry, tera, image_index)?;
    }
    let plugin_name = &plugin.manifest.name;

    // HTML template path (existing behavior).
    if let Some(template_name) = &decl.template {
        let template_key = format!("{plugin_name}::{template_name}");
        let mut ctx = tera::Context::new();
        ctx.insert("attrs", &b.attrs);
        ctx.insert("inner_html", &inner_html);
        let rendered = tera
            .render(&template_key, &ctx)
            .map_err(|e| BuildError::Config(format!("template {template_key}: {e}")))?;
        out.push_str(&rendered);
        if !rendered.ends_with('\n') {
            out.push('\n');
        }
        return Ok(());
    }

    // Markdown template path (new behavior).
    if let Some(md_template_name) = &decl.markdown_template {
        let template_key = format!("{plugin_name}::{md_template_name}");
        let mut ctx = tera::Context::new();
        // Insert each attr at the top level so templates can use bare field
        // names like `{{ name }}` alongside `{{ attrs.name }}`.
        if let Some(obj) = b.attrs.as_object() {
            for (k, v) in obj {
                ctx.insert(k, v);
            }
        }
        let rendered = tera
            .render(&template_key, &ctx)
            .map_err(|e| BuildError::Config(format!("markdown template {template_key}: {e}")))?;
        // Render the Tera-interpolated markdown to HTML.
        let md_html = lopress_core::render_markdown(&rendered);
        out.push_str(&md_html);
        if !md_html.ends_with('\n') {
            out.push('\n');
        }
        return Ok(());
    }

    // Base (built-in) block with no template — emit inner HTML directly.
    out.push_str(&inner_html);
    if !inner_html.is_empty() && !inner_html.ends_with('\n') {
        out.push('\n');
    }
    Ok(())
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopress_assets::{ImageResult, Variant};
    use lopress_core::FrontMatter;
    use serde_json::json;
    use std::path::PathBuf;
    use tera::Tera;

    fn empty_registry() -> PluginRegistry {
        PluginRegistry::default()
    }

    fn seed_index(idx: &mut ImageIndex, original: &str, variants: &[(u32, &str)]) {
        let files = variants
            .iter()
            .map(|(w, f)| Variant {
                filename: PathBuf::from(*f),
                width: *w,
                format: "webp".into(),
            })
            .collect();
        idx.record(
            &PathBuf::from(format!("/src/images/{original}")),
            &ImageResult { files },
        );
    }

    #[test]
    fn marker_renders_to_nothing_in_body() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![
                Block::paragraph("before"),
                Block {
                    r#type: "lopress:more".into(),
                    attrs: json!({}),
                    children: vec![],
                    text: None,
                },
                Block::paragraph("after"),
            ],
        };
        let html = render_body(
            &doc,
            &empty_registry(),
            &Tera::default(),
            &ImageIndex::default(),
        )
        .unwrap();
        assert_eq!(html, "<p>before</p>\n<p>after</p>\n");
    }

    #[test]
    fn image_in_index_renders_picture_with_srcset() {
        let mut idx = ImageIndex::default();
        seed_index(
            &mut idx,
            "photo.jpg",
            &[(400, "photo.400w.webp"), (800, "photo.800w.webp")],
        );
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "image".into(),
                attrs: json!({ "src": "/images/photo.jpg", "alt": "A & B", "caption": "Cap" }),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &empty_registry(), &Tera::default(), &idx).unwrap();
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
        let html = render_body(&doc, &empty_registry(), &Tera::default(), &idx).unwrap();
        assert!(!html.contains("<picture>"));
        assert!(html.contains(r#"<img src="https://ex.com/x.png" alt="x""#));
    }

    #[test]
    fn excerpt_is_blocks_before_marker() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![
                Block::paragraph("teaser"),
                Block {
                    r#type: "lopress:more".into(),
                    attrs: json!({}),
                    children: vec![],
                    text: None,
                },
                Block::paragraph("hidden"),
            ],
        };
        let ex = render_excerpt(
            &doc,
            &empty_registry(),
            &Tera::default(),
            &ImageIndex::default(),
        )
        .unwrap();
        assert_eq!(ex.as_deref(), Some("<p>teaser</p>\n"));
    }

    #[test]
    fn excerpt_is_none_without_marker() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::paragraph("only")],
        };
        let ex = render_excerpt(
            &doc,
            &empty_registry(),
            &Tera::default(),
            &ImageIndex::default(),
        )
        .unwrap();
        assert!(ex.is_none());
    }

    #[test]
    fn renders_paragraph_and_heading() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::heading(2, "Hi"), Block::paragraph("body")],
        };
        let tera = Tera::default();
        let html = render_body(&doc, &empty_registry(), &tera, &ImageIndex::default()).unwrap();
        assert_eq!(html, "<h2>Hi</h2>\n<p>body</p>\n");
    }

    #[test]
    fn unknown_custom_block_emits_comment() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:missing".into(),
                attrs: json!({}),
                children: vec![],
                text: None,
            }],
        };
        let tera = Tera::default();
        let html = render_body(&doc, &empty_registry(), &tera, &ImageIndex::default()).unwrap();
        assert!(html.contains("missing plugin for lopress:missing"));
    }

    #[test]
    fn known_custom_block_renders_via_template() {
        use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
        let mut reg = PluginRegistry::default();
        reg.insert(LoadedPlugin {
            root: std::path::PathBuf::from("/does/not/exist"),
            manifest: PluginManifest {
                name: "demo".into(),
                version: "0.1.0".into(),
                theme: false,
                blocks: vec![BlockDecl {
                    name: "lopress:demo".into(),
                    template: Some("blocks/demo.html".into()),
                    markdown_template: None,
                    attrs: Default::default(),
                    renderer: None,
                    editor: None,
                    builtin: false,
                    native: None,
                    css: Vec::new(),
                    js: Vec::new(),
                }],
            },
        })
        .unwrap();

        let mut tera = Tera::default();
        tera.add_raw_template(
            "demo::blocks/demo.html",
            "<figure data-x=\"{{ attrs.x }}\">{{ inner_html | safe }}</figure>",
        )
        .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:demo".into(),
                attrs: json!({"x":"v"}),
                children: vec![Block::paragraph("inner")],
                text: None,
            }],
        };
        let html = render_body(&doc, &reg, &tera, &ImageIndex::default()).unwrap();
        assert!(html.contains("data-x=\"v\""));
        assert!(html.contains("<p>inner</p>"));
    }

    #[test]
    fn markdown_template_interpolates_and_presents_as_html() {
        use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
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
                    renderer: None,
                    editor: None,
                    builtin: false,
                    native: None,
                    css: Vec::new(),
                    js: Vec::new(),
                }],
            },
        })
        .unwrap();

        let mut tera = Tera::default();
        tera.add_raw_template("demo::blocks/demo.md", "**About {{ name }}**\n\n{{ bio }}")
            .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:demo".into(),
                attrs: json!({"name":"Jane","bio":"Loves **Rust**"}),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &reg, &tera, &ImageIndex::default()).unwrap();
        assert!(
            html.contains("<strong>About Jane</strong>"),
            "name interpolated: {html}"
        );
        assert!(
            html.contains("<strong>Rust</strong>"),
            "markdown in field value renders: {html}"
        );
    }

    #[test]
    fn checkbox_attr_drives_if_conditional_in_markdown_template() {
        use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
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
                    renderer: None,
                    editor: None,
                    builtin: false,
                    native: None,
                    css: Vec::new(),
                    js: Vec::new(),
                }],
            },
        })
        .unwrap();

        let mut tera = Tera::default();
        tera.add_raw_template(
            "demo::blocks/demo.md",
            "{{ name }}\n{% if spoiler %}\n> ⚠️ Contains spoilers.\n{% endif %}",
        )
        .unwrap();

        // spoiler = true
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:demo".into(),
                attrs: json!({"name":"Jane","spoiler":true}),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &reg, &tera, &ImageIndex::default()).unwrap();
        assert!(
            html.contains("<blockquote>"),
            "spoiler blockquote present: {html}"
        );

        // spoiler = false
        let doc2 = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:demo".into(),
                attrs: json!({"name":"Jane","spoiler":false}),
                children: vec![],
                text: None,
            }],
        };
        let html2 = render_body(&doc2, &reg, &tera, &ImageIndex::default()).unwrap();
        assert!(
            !html2.contains("<blockquote>"),
            "no spoiler blockquote: {html2}"
        );
    }

    #[test]
    fn author_bio_plugin_renders_markdown_template_through_pipeline() {
        use lopress_plugin::load_dir;

        // Load the author-bio fixture from the test fixtures.
        let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("with-plugin")
            .join("plugins");
        let registry = load_dir(&fixtures_dir, None).unwrap();

        // The author-bio plugin should be registered.
        let (_, decl) = registry
            .block("lopress:author-bio")
            .expect("author-bio block should be registered");
        assert_eq!(
            decl.markdown_template.as_deref(),
            Some("blocks/author-bio.md")
        );

        // Build a Tera that knows the author-bio markdown template.
        let mut tera = Tera::default();
        let plugin_dir = fixtures_dir.join("author-bio");
        let md_src = std::fs::read_to_string(plugin_dir.join("blocks/author-bio.md"))
            .expect("markdown template file exists");
        tera.add_raw_template("author-bio::blocks/author-bio.md", &md_src)
            .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:author-bio".into(),
                attrs: json!({
                    "name": "Jane",
                    "bio": "Loves **Rust** and **Rust**",
                    "spoiler": true
                }),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &registry, &tera, &ImageIndex::default()).unwrap();

        // Check that the markdown template was interpolated AND the result
        // was rendered through the md→HTML pipeline.
        assert!(
            html.contains("<strong>About Jane</strong>"),
            "name rendered: {html}"
        );
        assert!(
            html.contains("<strong>Rust</strong>"),
            "markdown in bio rendered: {html}"
        );
        assert!(html.contains("<blockquote>"), "spoiler blockquote: {html}");
        assert!(html.contains("Contains spoilers"), "spoiler text: {html}");
    }

    #[test]
    fn bundled_callout_plugin_renders_div_wrapped_markdown() {
        use lopress_plugin::load_dir;

        // Load the bundled callout plugin from the repo `plugins/` dir
        // (CARGO_MANIFEST_DIR is `<repo>/crates/lopress-build`).
        let plugins_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins");
        let registry = load_dir(&plugins_dir, None).unwrap();

        let (_, decl) = registry
            .block("lopress:callout")
            .expect("callout block should be registered");
        assert_eq!(decl.markdown_template.as_deref(), Some("blocks/callout.md"));

        let mut tera = Tera::default();
        let md_src = std::fs::read_to_string(plugins_dir.join("callout/blocks/callout.md"))
            .expect("callout template file exists");
        tera.add_raw_template("callout::blocks/callout.md", &md_src)
            .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:callout".into(),
                attrs: json!({
                    "variant": "warning",
                    "title": "Heads up",
                    "body": "Be **careful** here.",
                }),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &registry, &tera, &ImageIndex::default()).unwrap();

        // The variant drives the CSS class, and both the title and the body
        // field's own markdown render through the md→HTML pipeline.
        assert!(
            html.contains(r#"class="callout callout-warning""#),
            "variant class present: {html}"
        );
        assert!(
            html.contains("<strong>Heads up</strong>"),
            "title rendered: {html}"
        );
        assert!(
            html.contains("<strong>careful</strong>"),
            "body markdown rendered: {html}"
        );
    }

    #[test]
    fn bundled_button_plugin_renders_html_template() {
        use lopress_plugin::load_dir;

        let plugins_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins");
        let registry = load_dir(&plugins_dir, None).unwrap();

        let (_, decl) = registry
            .block("lopress:button")
            .expect("button block should be registered");
        assert_eq!(decl.template.as_deref(), Some("blocks/button.html"));

        let mut tera = Tera::default();
        let html_src = std::fs::read_to_string(plugins_dir.join("button/blocks/button.html"))
            .expect("button template file exists");
        tera.add_raw_template("button::blocks/button.html", &html_src)
            .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:button".into(),
                attrs: json!({
                    "text": "Click me",
                    "url": "/go",
                    "variant": "primary",
                    "new_tab": true,
                }),
                children: vec![],
                text: None,
            }],
        };
        let html = render_body(&doc, &registry, &tera, &ImageIndex::default()).unwrap();

        assert!(
            html.contains(r#"class="btn btn-primary""#),
            "variant class: {html}"
        );
        // Tera auto-escapes `.html` templates, so `/` in the URL renders as the
        // `&#x2F;` entity (valid — browsers decode it). Assert the escaped form.
        assert!(html.contains(r#"href="&#x2F;go""#), "url: {html}");
        assert!(html.contains("Click me"), "label: {html}");
        assert!(
            html.contains(r#"target="_blank""#),
            "new_tab opens blank: {html}"
        );
    }
}

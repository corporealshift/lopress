use crate::error::BuildError;
use lopress_core::{Block, Document};
use lopress_plugin::PluginRegistry;
use std::fmt::Write;
use tera::Tera;

/// Render the body of a Document into HTML. `tera` may be shared with the
/// theme engine but must also know the plugin templates (the builder inserts
/// them at startup).
pub fn render_body(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<String, BuildError> {
    let mut out = String::new();
    for b in &doc.blocks {
        write_block(&mut out, b, registry, tera)?;
    }
    Ok(out)
}

fn write_block(
    out: &mut String,
    b: &Block,
    registry: &PluginRegistry,
    tera: &Tera,
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
                write_block(out, c, registry, tera)?;
            }
            out.push_str("</blockquote>\n");
        }
        "code_block" => {
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
                    write_block(out, c, registry, tera)?;
                }
                out.push_str("</li>\n");
            }
            let _ = writeln!(out, "</{tag}>");
        }
        "image" => {
            let src = escape(b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or(""));
            let alt = escape(b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or(""));
            let _ = writeln!(out, "<img src=\"{src}\" alt=\"{alt}\">");
        }
        custom if custom.starts_with("lopress:") => {
            render_custom(out, b, custom, registry, tera)?;
        }
        other => {
            let o = escape(other);
            let _ = writeln!(out, "<!-- unknown block: {o} -->");
        }
    }
    Ok(())
}

fn render_custom(
    out: &mut String,
    b: &Block,
    full_name: &str,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<(), BuildError> {
    let Some((plugin, decl)) = registry.block(full_name) else {
        let n = escape(full_name);
        let _ = writeln!(out, "<!-- missing plugin for {n} -->");
        return Ok(());
    };
    // Render inner children first.
    let mut inner_html = String::new();
    for c in &b.children {
        write_block(&mut inner_html, c, registry, tera)?;
    }
    let plugin_name = &plugin.manifest.name;
    let Some(template_name) = &decl.template else {
        // Base (built-in) block with no HTML template — handled by the
        // editor, not the static renderer. Emit the inner HTML directly.
        out.push_str(&inner_html);
        if !inner_html.is_empty() && !inner_html.ends_with('\n') {
            out.push('\n');
        }
        return Ok(());
    };
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
    use lopress_core::FrontMatter;
    use serde_json::json;
    use tera::Tera;

    fn empty_registry() -> PluginRegistry {
        PluginRegistry::default()
    }

    #[test]
    fn renders_paragraph_and_heading() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::heading(2, "Hi"), Block::paragraph("body")],
        };
        let tera = Tera::default();
        let html = render_body(&doc, &empty_registry(), &tera).unwrap();
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
        let html = render_body(&doc, &empty_registry(), &tera).unwrap();
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
        let html = render_body(&doc, &reg, &tera).unwrap();
        assert!(html.contains("data-x=\"v\""));
        assert!(html.contains("<p>inner</p>"));
    }
}

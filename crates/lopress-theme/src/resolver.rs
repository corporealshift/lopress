use crate::builtin::default_engine;
use crate::engine::ThemeEngine;
use crate::error::ThemeError;
use lopress_plugin::PluginRegistry;
use std::path::PathBuf;

pub struct ResolvedTheme {
    pub engine: ThemeEngine,
    /// Path on disk to the theme's `theme.css`, or None if using the built-in.
    pub css_path: Option<PathBuf>,
    /// Raw CSS content (either from disk or the built-in).
    pub css_content: String,
}

impl std::fmt::Debug for ResolvedTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedTheme")
            .field("css_path", &self.css_path)
            .field("css_content_len", &self.css_content.len())
            .finish()
    }
}

pub fn resolve(
    registry: &PluginRegistry,
    theme_name: &str,
) -> Result<ResolvedTheme, ThemeError> {
    // If a plugin with this name exists and is a theme, use it (overriding
    // the built-in when name == "default").
    if let Some(plugin) = registry.theme(theme_name) {
        let templates_dir = plugin.root.join("templates");
        let mut tpls = Vec::new();
        for entry in std::fs::read_dir(&templates_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) != Some("html") {
                continue;
            }
            let name = entry
                .path()
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let contents = std::fs::read_to_string(entry.path())?;
            tpls.push((name, contents));
        }
        let engine = ThemeEngine::from_templates(&tpls)?;
        let css_path = plugin.root.join("theme.css");
        let css_content = std::fs::read_to_string(&css_path).unwrap_or_default();
        return Ok(ResolvedTheme {
            engine,
            css_path: Some(css_path),
            css_content,
        });
    }

    // Fallback: built-in default theme. Only valid when theme_name == "default".
    if theme_name == "default" {
        Ok(ResolvedTheme {
            engine: default_engine()?,
            css_path: None,
            css_content: crate::builtin::default_css().to_string(),
        })
    } else {
        Err(ThemeError::NotFound(theme_name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_name_resolves_to_builtin_when_no_plugin() {
        let reg = PluginRegistry::default();
        let t = resolve(&reg, "default").unwrap();
        assert!(t.css_path.is_none());
        assert!(t.css_content.contains("body"));
    }

    #[test]
    fn unknown_name_errors() {
        let reg = PluginRegistry::default();
        let err = resolve(&reg, "mystery").unwrap_err();
        assert!(matches!(err, ThemeError::NotFound(_)));
    }
}

use crate::error::PluginError;
use crate::manifest::parse_manifest;
use crate::registry::{LoadedPlugin, PluginRegistry};
use std::path::Path;

pub fn load_dir(dir: &Path, enabled: Option<&[String]>) -> Result<PluginRegistry, PluginError> {
    let mut reg = PluginRegistry::default();
    if !dir.exists() {
        return Ok(reg);
    }
    for entry in std::fs::read_dir(dir).map_err(|source| PluginError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| PluginError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let manifest_path = root.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = parse_manifest(&manifest_path)?;
        if let Some(list) = enabled {
            if !list.iter().any(|n| n == &manifest.name) {
                continue;
            }
        }
        for block in &manifest.blocks {
            if !root.join(&block.template).exists() {
                return Err(PluginError::MissingTemplate {
                    name: block.name.clone(),
                    template: block.template.clone(),
                });
            }
        }
        reg.insert(LoadedPlugin { root, manifest })?;
    }
    Ok(reg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_plugin(root: &std::path::Path, name: &str, block: Option<&str>) {
        std::fs::create_dir_all(root).unwrap();
        let mut toml_src = format!("name = \"{name}\"\nversion = \"0.1.0\"\n");
        if let Some(b) = block {
            toml_src.push_str(&format!(
                "\n[[blocks]]\nname = \"{b}\"\ntemplate = \"blocks/x.html\"\n"
            ));
            let tpl = root.join("blocks");
            std::fs::create_dir_all(&tpl).unwrap();
            std::fs::write(tpl.join("x.html"), "<div>x</div>").unwrap();
        }
        std::fs::write(root.join("plugin.toml"), toml_src).unwrap();
    }

    #[test]
    fn missing_plugins_dir_returns_empty_registry() {
        let d = TempDir::new().unwrap();
        let reg = load_dir(&d.path().join("plugins"), None).unwrap();
        assert!(reg.plugins.is_empty());
    }

    #[test]
    fn loads_plugins_and_indexes_blocks() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        make_plugin(&plugins.join("a"), "a", Some("lopress:a-block"));
        make_plugin(&plugins.join("b"), "b", Some("lopress:b-block"));
        let reg = load_dir(&plugins, None).unwrap();
        assert_eq!(reg.plugins.len(), 2);
        assert!(reg.block("lopress:a-block").is_some());
        assert!(reg.block("lopress:b-block").is_some());
    }

    #[test]
    fn respects_enabled_allowlist() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        make_plugin(&plugins.join("a"), "a", None);
        make_plugin(&plugins.join("b"), "b", None);
        let enabled = vec!["a".to_string()];
        let reg = load_dir(&plugins, Some(&enabled)).unwrap();
        assert_eq!(reg.plugins.len(), 1);
        assert_eq!(reg.plugins[0].manifest.name, "a");
    }

    #[test]
    fn missing_template_is_error() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        let root = plugins.join("bad");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            r#"name="bad"
version="0.1.0"

[[blocks]]
name="lopress:x"
template="blocks/missing.html"
"#,
        )
        .unwrap();
        let err = load_dir(&plugins, None).unwrap_err();
        assert!(matches!(err, PluginError::MissingTemplate { .. }));
    }
}

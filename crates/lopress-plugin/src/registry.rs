use crate::error::PluginError;
use crate::manifest::{parse_manifest_str, BlockDecl, PluginManifest};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub root: PathBuf,
    pub manifest: PluginManifest,
}

#[derive(Debug, Default, Clone)]
pub struct PluginRegistry {
    pub plugins: Vec<LoadedPlugin>,
    pub block_index: BTreeMap<String, (usize, usize)>,
    pub native_index: BTreeMap<String, (usize, usize)>,
    pub theme_index: BTreeMap<String, usize>,
}

impl PluginRegistry {
    pub fn insert(&mut self, plugin: LoadedPlugin) -> Result<(), PluginError> {
        let pi = self.plugins.len();
        if plugin.manifest.theme {
            self.theme_index.insert(plugin.manifest.name.clone(), pi);
        }
        for (bi, block) in plugin.manifest.blocks.iter().enumerate() {
            if self.block_index.contains_key(&block.name) {
                return Err(PluginError::DuplicateBlock(block.name.clone()));
            }
            if let Some(native) = &block.native {
                if self.native_index.contains_key(native) {
                    return Err(PluginError::DuplicateNative(native.clone()));
                }
            }
            self.block_index.insert(block.name.clone(), (pi, bi));
            if let Some(native) = &block.native {
                self.native_index.insert(native.clone(), (pi, bi));
            }
        }
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn block(&self, name: &str) -> Option<(&LoadedPlugin, &BlockDecl)> {
        let (pi, bi) = *self.block_index.get(name)?;
        let plugin = self.plugins.get(pi)?;
        let decl = plugin.manifest.blocks.get(bi)?;
        Some((plugin, decl))
    }

    /// Look up the block that exclusively claims a native `core_type`.
    pub fn native_block(&self, core_type: &str) -> Option<(&LoadedPlugin, &BlockDecl)> {
        let (pi, bi) = *self.native_index.get(core_type)?;
        let plugin = self.plugins.get(pi)?;
        let decl = plugin.manifest.blocks.get(bi)?;
        Some((plugin, decl))
    }

    pub fn theme(&self, name: &str) -> Option<&LoadedPlugin> {
        let pi = *self.theme_index.get(name)?;
        self.plugins.get(pi)
    }

    /// Register the built-in ("base") plugins shipped in the core codebase.
    /// Their manifests are embedded at compile time, so they are present
    /// regardless of the workspace's `plugins/` directory and cannot be
    /// removed by the user. Call this before loading user plugins so base
    /// blocks win any name collision.
    pub fn load_base_plugins(&mut self) -> Result<(), PluginError> {
        const BASE_MANIFESTS: &[&str] = &[include_str!("../../../base_plugins/list/manifest.toml")];
        for src in BASE_MANIFESTS {
            let manifest = parse_manifest_str(src)?;
            self.insert(LoadedPlugin {
                root: PathBuf::new(),
                manifest,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_base_plugins_registers_the_list_block() {
        let mut reg = PluginRegistry::default();
        reg.load_base_plugins().unwrap();
        let (_, decl) = reg.block("list").expect("list block registered");
        assert!(decl.builtin);
        assert_eq!(decl.editor.as_deref(), Some("list"));
        assert_eq!(decl.native.as_deref(), Some("list"));
        assert!(decl.attrs.contains_key("ordered"));
        let (_, native_decl) = reg.native_block("list").expect("list claims native list");
        assert_eq!(native_decl.name, "list");
    }

    #[test]
    fn native_block_looks_up_by_core_type() {
        let mut reg = PluginRegistry::default();
        let m = parse_manifest_str(
            r#"
name = "x"
version = "0.1.0"

[[blocks]]
name   = "x:list"
native = "list"
"#,
        )
        .unwrap();
        reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: m,
        })
        .unwrap();
        let (_, decl) = reg.native_block("list").expect("list core type claimed");
        assert_eq!(decl.name, "x:list");
        assert!(reg.native_block("heading").is_none());
    }

    #[test]
    fn duplicate_native_claim_is_an_error() {
        let mut reg = PluginRegistry::default();
        let one = parse_manifest_str(
            r#"
name = "a"
version = "0.1.0"

[[blocks]]
name   = "a:list"
native = "list"
"#,
        )
        .unwrap();
        reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: one,
        })
        .unwrap();
        let two = parse_manifest_str(
            r#"
name = "b"
version = "0.1.0"

[[blocks]]
name   = "b:list"
native = "list"
"#,
        )
        .unwrap();
        let err = reg.insert(LoadedPlugin {
            root: PathBuf::new(),
            manifest: two,
        });
        assert!(matches!(err, Err(PluginError::DuplicateNative(s)) if s == "list"));
    }
}

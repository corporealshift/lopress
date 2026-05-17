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
            self.block_index.insert(block.name.clone(), (pi, bi));
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
        const LIST_MANIFEST: &str =
            include_str!("../../../base_plugins/list/manifest.toml");
        for src in [LIST_MANIFEST] {
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
        assert!(decl.attrs.contains_key("ordered"));
    }
}

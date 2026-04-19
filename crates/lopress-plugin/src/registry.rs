use crate::error::PluginError;
use crate::manifest::{BlockDecl, PluginManifest};
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
        let plugin = &self.plugins[pi];
        let decl = &plugin.manifest.blocks[bi];
        Some((plugin, decl))
    }

    pub fn theme(&self, name: &str) -> Option<&LoadedPlugin> {
        let pi = *self.theme_index.get(name)?;
        Some(&self.plugins[pi])
    }
}

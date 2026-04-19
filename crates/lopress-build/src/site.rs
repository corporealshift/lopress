use crate::error::BuildError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SiteConfig {
    pub site: Site,
    #[serde(default)]
    pub plugins: Plugins,
    #[serde(default)]
    pub build: BuildSettings,
    #[serde(default)]
    pub robots: Option<RobotsConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Site {
    pub title: String,
    pub base_url: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub nav: Nav,
    #[serde(default)]
    pub og_image: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Nav {
    #[serde(default)]
    pub items: Vec<NavItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NavItem {
    pub label: String,
    pub href: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Plugins {
    #[serde(default)]
    pub enabled: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildSettings {
    #[serde(default = "default_image_widths")]
    pub image_variants: Vec<u32>,
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            image_variants: default_image_widths(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RobotsConfig {
    pub body: String,
}

fn default_theme() -> String {
    "default".into()
}

fn default_image_widths() -> Vec<u32> {
    vec![400, 800, 1600]
}

/// Describes the on-disk workspace layout.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub config: SiteConfig,
}

impl Workspace {
    pub fn load(root: &Path) -> Result<Self, BuildError> {
        let config_path = root.join("lopress.toml");
        if !config_path.exists() {
            return Err(BuildError::Config(format!(
                "no lopress.toml at {}",
                config_path.display()
            )));
        }
        let src = std::fs::read_to_string(&config_path)?;
        let config: SiteConfig = toml::from_str(&src)?;
        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    pub fn src_dir(&self) -> PathBuf { self.root.join("src") }
    pub fn posts_dir(&self) -> PathBuf { self.src_dir().join("posts") }
    pub fn pages_dir(&self) -> PathBuf { self.src_dir().join("pages") }
    pub fn images_dir(&self) -> PathBuf { self.src_dir().join("images") }
    pub fn plugins_dir(&self) -> PathBuf { self.root.join("plugins") }
    pub fn www_dir(&self) -> PathBuf { self.root.join("www") }
    pub fn cache_path(&self) -> PathBuf { self.www_dir().join(".lopress-cache.json") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn loads_minimal_config() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"
[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();
        let w = Workspace::load(d.path()).unwrap();
        assert_eq!(w.config.site.theme, "default");
        assert_eq!(w.config.build.image_variants, vec![400, 800, 1600]);
    }

    #[test]
    fn missing_config_is_error() {
        let d = TempDir::new().unwrap();
        assert!(Workspace::load(d.path()).is_err());
    }
}

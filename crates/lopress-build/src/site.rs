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
    pub nav: Nav,
    pub warnings: Vec<String>,
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

        // Load nav from nav.toml (empty if absent).
        let nav_path = root.join("nav.toml");
        let nav = if nav_path.exists() {
            let nav_src = std::fs::read_to_string(&nav_path)?;
            let nav: Nav = toml::from_str(&nav_src)?;
            nav
        } else {
            Nav::default()
        };

        // Detect leftover [site.nav] in lopress.toml via raw toml::Value peek.
        let raw_value: toml::Value = toml::from_str(&src)?;
        let mut warnings = Vec::new();
        if let Some(site) = raw_value.get("site").and_then(|v| v.as_table()) {
            if site.contains_key("nav") {
                warnings.push(
                    "[site.nav] in lopress.toml is no longer supported and is ignored — move the items to nav.toml and delete the old block.".into(),
                );
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            config,
            nav,
            warnings,
        })
    }

    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }
    pub fn posts_dir(&self) -> PathBuf {
        self.src_dir().join("posts")
    }
    pub fn pages_dir(&self) -> PathBuf {
        self.src_dir().join("pages")
    }
    pub fn images_dir(&self) -> PathBuf {
        self.src_dir().join("images")
    }
    pub fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }
    pub fn www_dir(&self) -> PathBuf {
        self.root.join("www")
    }
    pub fn cache_path(&self) -> PathBuf {
        self.www_dir().join(".lopress-cache.json")
    }

    /// Find the favicon in `src/` by priority order (svg → png → ico).
    ///
    /// Returns `(source_path, web_path)` — e.g. `(…/src/favicon.png,
    /// "/favicon.png")` — or `None` when no favicon file exists.
    pub fn favicon(&self) -> Option<(PathBuf, String)> {
        let src = self.src_dir();
        for ext in ["svg", "png", "ico"] {
            let path = src.join(format!("favicon.{ext}"));
            if path.exists() {
                return Some((path, format!("/favicon.{ext}")));
            }
        }
        None
    }
}

/// Serialize `items` to TOML and write atomically to `nav.toml` at `root`.
///
/// Items with an empty `label` or empty `href` are dropped before writing.
/// An empty `items` list writes `items = []`.
pub fn write_nav(root: &Path, items: &[NavItem]) -> Result<(), BuildError> {
    // Drop rows with empty label or href.
    let filtered: Vec<NavItem> = items
        .iter()
        .filter(|n| !n.label.is_empty() && !n.href.is_empty())
        .cloned()
        .collect();

    let nav = Nav { items: filtered };

    // `BuildError` has no `From<toml::ser::Error>` — map into Config.
    let serialized =
        toml::to_string(&nav).map_err(|e| BuildError::Config(format!("nav.toml: {e}")))?;

    // Atomic write: temp file + rename.
    let tmp = root.join(".nav.toml.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, root.join("nav.toml"))?;
    Ok(())
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

    #[test]
    fn write_nav_creates_nav_toml() {
        let d = TempDir::new().unwrap();
        let items = vec![
            NavItem {
                label: "Home".into(),
                href: "/".into(),
            },
            NavItem {
                label: "About".into(),
                href: "/about/".into(),
            },
        ];
        write_nav(d.path(), &items).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("items"));
        assert!(content.contains("Home"));
        assert!(content.contains("/about/"));
    }

    #[test]
    fn write_nav_drops_empty_rows() {
        let d = TempDir::new().unwrap();
        let items = vec![
            NavItem {
                label: "Home".into(),
                href: "/".into(),
            },
            NavItem {
                label: "".into(),
                href: "/empty/".into(),
            },
            NavItem {
                label: "X".into(),
                href: "".into(),
            },
        ];
        write_nav(d.path(), &items).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("Home"));
        assert!(!content.contains("/empty/"));
        assert!(!content.contains("X"));
    }

    #[test]
    fn write_nav_empty_items_writes_empty_array() {
        let d = TempDir::new().unwrap();
        write_nav(d.path(), &[]).unwrap();
        let content = std::fs::read_to_string(d.path().join("nav.toml")).unwrap();
        assert!(content.contains("items = []"));
    }

    #[test]
    fn workspace_loads_nav_from_nav_toml() {
        let d = TempDir::new().unwrap();
        // Write minimal config.
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();
        // Write nav.toml.
        write_nav(
            d.path(),
            &[
                NavItem {
                    label: "Home".into(),
                    href: "/".into(),
                },
                NavItem {
                    label: "About".into(),
                    href: "/about/".into(),
                },
            ],
        )
        .unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert_eq!(ws.nav.items.len(), 2);
        assert_eq!(ws.nav.items[0].label, "Home");
        assert_eq!(ws.nav.items[1].href, "/about/");
        assert!(ws.warnings.is_empty());
    }

    #[test]
    fn workspace_has_empty_nav_when_no_nav_toml() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"
"#,
        )
        .unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert!(ws.nav.items.is_empty());
        assert!(ws.warnings.is_empty());
    }

    #[test]
    fn workspace_warns_on_leftover_site_nav() {
        let d = TempDir::new().unwrap();
        // Write config WITH [site.nav] — this is the legacy format.
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Old", href = "/old/" }]
"#,
        )
        .unwrap();
        // No nav.toml — the old block should trigger a warning.

        let ws = Workspace::load(d.path()).unwrap();
        assert!(ws.nav.items.is_empty()); // nav.toml doesn't exist
        assert!(!ws.warnings.is_empty());
        assert!(ws.warnings[0].contains("[site.nav]"));
        assert!(ws.warnings[0].contains("ignored"));
    }

    #[test]
    fn workspace_warns_even_when_nav_toml_exists() {
        let d = TempDir::new().unwrap();
        std::fs::write(
            d.path().join("lopress.toml"),
            r#"[site]
title = "S"
base_url = "https://example.com"

[site.nav]
items = [{ label = "Old", href = "/old/" }]
"#,
        )
        .unwrap();
        write_nav(
            d.path(),
            &[NavItem {
                label: "New".into(),
                href: "/new/".into(),
            }],
        )
        .unwrap();

        let ws = Workspace::load(d.path()).unwrap();
        assert_eq!(ws.nav.items.len(), 1);
        assert_eq!(ws.nav.items[0].label, "New");
        // Warning still fires because [site.nav] is present in lopress.toml.
        assert!(!ws.warnings.is_empty());
        assert!(ws.warnings[0].contains("[site.nav]"));
    }

    fn favicon_workspace(d: &TempDir) -> Workspace {
        std::fs::write(
            d.path().join("lopress.toml"),
            "[site]\ntitle = \"A\"\nbase_url = \"https://a\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        Workspace::load(d.path()).unwrap()
    }

    #[test]
    fn favicon_returns_svg_when_present() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.svg"), b"<svg/>").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.svg"));
        assert_eq!(web, "/favicon.svg");
    }

    #[test]
    fn favicon_prefers_svg_over_png() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.svg"), b"<svg/>").unwrap();
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.svg"));
        assert_eq!(web, "/favicon.svg");
    }

    #[test]
    fn favicon_falls_back_to_png() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.png"), b"PNG").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.png"));
        assert_eq!(web, "/favicon.png");
    }

    #[test]
    fn favicon_falls_back_to_ico() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        std::fs::write(d.path().join("src/favicon.ico"), b"ICO").unwrap();
        let (path, web) = ws.favicon().unwrap();
        assert!(path.ends_with("favicon.ico"));
        assert_eq!(web, "/favicon.ico");
    }

    #[test]
    fn favicon_returns_none_when_no_file_exists() {
        let d = TempDir::new().unwrap();
        let ws = favicon_workspace(&d);
        assert!(ws.favicon().is_none());
    }
}

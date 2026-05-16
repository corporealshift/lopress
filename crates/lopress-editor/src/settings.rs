use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Settings {
    #[serde(default)]
    pub recents: Vec<PathBuf>,
    #[serde(default)]
    pub window: WindowSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowSettings {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub maximized: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            x: 100.0,
            y: 100.0,
            maximized: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
}

impl Settings {
    /// Load settings from `path`; returns default if file does not exist.
    ///
    /// # Errors
    /// Returns `SettingsError::Io` for non-not-found I/O errors,
    /// `SettingsError::Parse` for malformed JSON.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Save settings to `path`, creating parent dirs as needed.
    ///
    /// # Errors
    /// Returns `SettingsError::Io` on write failure or `SettingsError::Parse` on serialization failure.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Load `settings_path`. If it does not exist but `legacy_recents_path` does,
    /// migrate the recents list into a fresh settings file and delete the legacy file.
    ///
    /// # Errors
    /// Returns `SettingsError::Io` for I/O errors, `SettingsError::Parse` for malformed JSON in `settings_path`.
    /// Malformed JSON in the legacy `recents.json` is treated as an empty list rather than an error,
    /// since the legacy format is a best-effort migration.
    pub fn load_or_migrate(
        settings_path: &Path,
        legacy_recents_path: &Path,
    ) -> Result<Self, SettingsError> {
        if settings_path.exists() {
            return Self::load_from(settings_path);
        }
        if legacy_recents_path.exists() {
            #[derive(Deserialize, Default)]
            struct LegacyRecentsFile {
                #[serde(default)]
                paths: Vec<PathBuf>,
            }
            let raw = std::fs::read_to_string(legacy_recents_path)?;
            let legacy: LegacyRecentsFile = serde_json::from_str(&raw).unwrap_or_default();
            let s = Self {
                recents: legacy.paths,
                ..Self::default()
            };
            s.save_to(settings_path)?;
            std::fs::remove_file(legacy_recents_path)?;
            return Ok(s);
        }
        Ok(Self::default())
    }
}

/// Resolve the platform-standard settings file path under the lopress config dir.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "lopress").map(|d| d.config_dir().join("settings.json"))
}

/// Resolve the legacy `recents.json` path for migration purposes.
pub fn legacy_recents_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "lopress").map(|d| d.config_dir().join("recents.json"))
}

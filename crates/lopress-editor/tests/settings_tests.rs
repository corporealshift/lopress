#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use lopress_editor::settings::{Settings, WindowSettings};
use std::path::PathBuf;
use tempfile::TempDir;

fn dir() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn loads_default_when_missing() {
    let d = dir();
    let path = d.path().join("settings.json");
    let s = Settings::load_from(&path).unwrap();
    assert!(s.recents.is_empty());
    assert_eq!(s.window.width, 1200.0);
    assert_eq!(s.window.height, 800.0);
}

#[test]
fn round_trip() {
    let d = dir();
    let path = d.path().join("settings.json");
    let mut s = Settings::default();
    s.recents.push(PathBuf::from("/some/workspace"));
    s.window.width = 1400.0;
    s.window.height = 900.0;
    s.save_to(&path).unwrap();

    let loaded = Settings::load_from(&path).unwrap();
    assert_eq!(loaded.recents, vec![PathBuf::from("/some/workspace")]);
    assert_eq!(loaded.window.width, 1400.0);
}

#[test]
fn ignores_unknown_fields() {
    let d = dir();
    let path = d.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"recents":[],"window":{"width":1200.0,"height":800.0,"x":0.0,"y":0.0,"maximized":false},"ui_zoom":1.5}"#,
    )
    .unwrap();
    let _ = Settings::load_from(&path).unwrap(); // must not error on ui_zoom
}

#[test]
fn migrates_recents_json() {
    let d = dir();
    let recents_path = d.path().join("recents.json");
    let settings_path = d.path().join("settings.json");
    std::fs::write(&recents_path, r#"["/old/workspace"]"#).unwrap();

    let s = Settings::load_or_migrate(&settings_path, &recents_path).unwrap();
    assert_eq!(s.recents, vec![PathBuf::from("/old/workspace")]);
    assert!(!recents_path.exists(), "old recents.json should be deleted");
    assert!(settings_path.exists());
}

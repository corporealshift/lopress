// A failed read is a failed test, so unwrap is the right tool here.
#![allow(clippy::unwrap_used)]

//! Colours live in `ui/theme.rs`. This scans `src/ui` so they stay there.

use std::fs;
use std::path::{Path, PathBuf};

const LITERALS: &[&str] = &[
    "Color::rgb",
    "Color::rgba",
    "Color::WHITE",
    "Color::BLACK",
    "rgb8(",
    "rgba8(",
];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn colour_literals_stay_in_the_theme() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    let mut stray = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "theme.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        if LITERALS.iter().any(|literal| source.contains(literal)) {
            stray.push(relative);
        }
    }

    assert!(
        stray.is_empty(),
        "colour literals outside ui/theme.rs: {stray:?}"
    );
}

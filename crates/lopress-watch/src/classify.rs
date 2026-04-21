use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Source,
    Plugins,
    Config,
    Ignored,
}

/// Classify `path` (absolute or workspace-relative) against the workspace
/// root. Ignored paths include `www/`, `target/`, dot-directories, editor
/// swap files, and anything outside the workspace.
pub fn classify(workspace: &Path, path: &Path) -> Bucket {
    // Canonicalization is expensive; we rely on the caller giving us a
    // path from notify, which is already absolute on every platform we
    // support. If it's relative, resolve against workspace.
    let abs: std::path::PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    };
    let rel = match abs.strip_prefix(workspace) {
        Ok(r) => r,
        Err(_) => return Bucket::Ignored,
    };
    // Any dot component anywhere (e.g. `.git`, `src/.obsidian`,
    // `plugins/foo/.cache`, `src/posts/.a.md.swp`) is off-limits.
    for comp in rel.components() {
        if let std::path::Component::Normal(c) = comp {
            if c.to_string_lossy().starts_with('.') {
                return Bucket::Ignored;
            }
        }
    }

    let mut comps = rel.components();
    let first = match comps.next() {
        Some(std::path::Component::Normal(c)) => c.to_string_lossy().into_owned(),
        _ => return Bucket::Ignored,
    };

    if is_editor_noise(rel) {
        return Bucket::Ignored;
    }
    match first.as_str() {
        "www" | "target" => Bucket::Ignored,
        "lopress.toml" => Bucket::Config,
        "src" => Bucket::Source,
        "plugins" => Bucket::Plugins,
        _ => Bucket::Ignored,
    }
}

fn is_editor_noise(rel: &Path) -> bool {
    let name = match rel.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name.starts_with(".#") || name.starts_with('~') || name == "4913" {
        return true;
    }
    if let Some(ext) = rel.extension().and_then(|s| s.to_str()) {
        matches!(ext, "swp" | "swx" | "swo" | "tmp")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        PathBuf::from("/ws")
    }

    #[test]
    fn source_under_src() {
        assert_eq!(
            classify(&ws(), &ws().join("src/posts/a.md")),
            Bucket::Source
        );
    }

    #[test]
    fn plugin_file() {
        assert_eq!(
            classify(&ws(), &ws().join("plugins/callout/plugin.toml")),
            Bucket::Plugins
        );
    }

    #[test]
    fn top_level_config() {
        assert_eq!(classify(&ws(), &ws().join("lopress.toml")), Bucket::Config);
    }

    #[test]
    fn www_is_ignored() {
        assert_eq!(
            classify(&ws(), &ws().join("www/index.html")),
            Bucket::Ignored
        );
    }

    #[test]
    fn dotdirs_are_ignored() {
        assert_eq!(classify(&ws(), &ws().join(".git/HEAD")), Bucket::Ignored);
    }

    #[test]
    fn nested_dotdirs_are_ignored() {
        assert_eq!(
            classify(&ws(), &ws().join("src/.obsidian/workspace.json")),
            Bucket::Ignored
        );
        assert_eq!(
            classify(&ws(), &ws().join("plugins/foo/.cache/x")),
            Bucket::Ignored
        );
    }

    #[test]
    fn editor_swap_is_ignored() {
        assert_eq!(
            classify(&ws(), &ws().join("src/posts/.a.md.swp")),
            Bucket::Ignored
        );
    }

    #[test]
    fn emacs_lockfile_ignored() {
        assert_eq!(
            classify(&ws(), &ws().join("src/posts/.#a.md")),
            Bucket::Ignored
        );
    }

    #[test]
    fn outside_workspace_ignored() {
        assert_eq!(classify(&ws(), Path::new("/etc/passwd")), Bucket::Ignored);
    }
}

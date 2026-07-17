//! Keep a document's `.md` filename synchronized with its slug.
//!
//! The *effective slug* is `slugify(front_matter.slug)` when that is
//! non-empty, else `slugify(front_matter.title)`. The filename stem is kept
//! equal to it. Front matter is never mutated — only the file moves.

use lopress_core::{slugify, FrontMatter};
use std::path::{Path, PathBuf};

/// The slug a document's filename should track, or empty when neither the
/// slug field nor the title yields anything usable.
fn effective_slug(fm: &FrontMatter) -> String {
    let from_slug = fm.slug.as_deref().map(slugify).filter(|s| !s.is_empty());
    from_slug.unwrap_or_else(|| slugify(fm.title.as_deref().unwrap_or("")))
}

/// Resolve a unique `{base}.md` / `{base}-N.md` path within `dir`, treating
/// `exclude` (the file's own current path) as available so a file never
/// collides with itself. `exists` reports whether a candidate is already
/// taken — injected so this is testable without touching disk.
pub fn unique_stem(
    dir: &Path,
    base: &str,
    exclude: Option<&Path>,
    exists: &impl Fn(&Path) -> bool,
) -> PathBuf {
    let first = dir.join(format!("{base}.md"));
    if Some(first.as_path()) == exclude || !exists(&first) {
        return first;
    }
    for n in 2..=9999u32 {
        let cand = dir.join(format!("{base}-{n}.md"));
        if Some(cand.as_path()) == exclude || !exists(&cand) {
            return cand;
        }
    }
    // Defensive: 9998 same-named files in one dir is not a real workflow.
    dir.join(format!("{base}-{}.md", u32::MAX))
}

/// The path `current` should be renamed to, or `None` when no rename is
/// needed (empty effective slug, or the current stem already matches).
pub fn resolve_target(
    fm: &FrontMatter,
    current: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let base = effective_slug(fm);
    if base.is_empty() {
        return None;
    }
    let dir = current.parent()?;
    let target = unique_stem(dir, &base, Some(current), &exists);
    if target.as_path() == current {
        None
    } else {
        Some(target)
    }
}

/// Rename `current` on disk to match its slug. Returns the new path, or `None`
/// when no rename was needed.
///
/// # Errors
/// Returns the underlying I/O error if the rename fails.
pub fn rename_to_slug(fm: &FrontMatter, current: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(target) = resolve_target(fm, current, |p| p.exists()) else {
        return Ok(None);
    };
    std::fs::rename(current, &target)?;
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(title: Option<&str>, slug: Option<&str>) -> FrontMatter {
        FrontMatter {
            title: title.map(str::to_string),
            slug: slug.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn derives_target_from_title() {
        let cur = Path::new("/posts/untitled-1.md");
        let got = resolve_target(&fm(Some("My First Post"), None), cur, |_| false);
        assert_eq!(got, Some(PathBuf::from("/posts/my-first-post.md")));
    }

    #[test]
    fn explicit_slug_wins_over_title() {
        let cur = Path::new("/posts/untitled-1.md");
        let got = resolve_target(&fm(Some("My First Post"), Some("intro")), cur, |_| false);
        assert_eq!(got, Some(PathBuf::from("/posts/intro.md")));
    }

    #[test]
    fn stem_already_matches_is_noop() {
        let cur = Path::new("/posts/my-first-post.md");
        assert_eq!(
            resolve_target(&fm(Some("My First Post"), None), cur, |_| false),
            None
        );
    }

    #[test]
    fn empty_effective_slug_is_noop() {
        let cur = Path::new("/posts/untitled-1.md");
        assert_eq!(resolve_target(&fm(None, None), cur, |_| false), None);
        assert_eq!(
            resolve_target(&fm(Some("!!!"), Some("!!!")), cur, |_| false),
            None
        );
    }

    #[test]
    fn collision_appends_suffix() {
        let cur = Path::new("/posts/untitled-1.md");
        // "hello.md" is taken by a different file; "hello-2.md" is free.
        let taken = Path::new("/posts/hello.md");
        let got = resolve_target(&fm(Some("Hello"), None), cur, |p| p == taken);
        assert_eq!(got, Some(PathBuf::from("/posts/hello-2.md")));
    }

    #[test]
    fn current_path_counts_as_available() {
        // File already named hello-2.md; hello.md taken by someone else.
        let cur = Path::new("/posts/hello-2.md");
        let taken = Path::new("/posts/hello.md");
        assert_eq!(
            resolve_target(&fm(Some("Hello"), None), cur, |p| p == taken),
            None
        );
    }

    #[test]
    fn rename_to_slug_moves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("untitled-1.md");
        std::fs::write(&old, "x").unwrap();
        let new = rename_to_slug(&fm(Some("My First Post"), None), &old)
            .unwrap()
            .unwrap();
        assert_eq!(new, dir.path().join("my-first-post.md"));
        assert!(new.exists());
        assert!(!old.exists());
    }

    #[test]
    fn rename_to_slug_suffixes_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.md"), "a").unwrap();
        let old = dir.path().join("untitled-1.md");
        std::fs::write(&old, "b").unwrap();
        let new = rename_to_slug(&fm(Some("Hello"), None), &old)
            .unwrap()
            .unwrap();
        assert_eq!(new, dir.path().join("hello-2.md"));
    }
}

//! Slug derivation: turn a human title/slug string into a filesystem- and
//! URL-safe stem (lowercase ASCII alphanumerics separated by single hyphens).

/// Lowercase, collapse every run of non-ASCII-alphanumeric characters into a
/// single `-`, and strip leading/trailing `-`. The result may be empty (e.g.
/// the input was all punctuation) — callers must guard for that.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_cases() {
        assert_eq!(slugify("My First Post"), "my-first-post");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("already-slug"), "already-slug");
        assert_eq!(slugify("a---b"), "a-b");
        assert_eq!(slugify("!!!"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_is_idempotent() {
        let once = slugify("My First Post");
        assert_eq!(slugify(&once), once);
    }
}

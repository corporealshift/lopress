# Lopress Phase 1 — Core and Headless Build CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A CLI `lopress` binary with `build <workspace>` and `new <dir>` subcommands that reads a workspace of markdown posts/pages and produces a complete static site in `www/` (HTML via an active theme, responsive image variants, feed/sitemap/robots/404), with no GUI.

**Architecture:** Rust cargo workspace with five internal crates — `lopress-core` (pure types + markdown parse/serialize), `lopress-plugin` (manifests, registry), `lopress-theme` (Tera engine + built-in default theme), `lopress-assets` (image pipeline), `lopress-build` (orchestrator) — plus a thin `lopress` binary crate. Dependencies flow one direction: core has no deps on anything else in the workspace; editor/preview (added in phase 3) sit on top.

**Tech Stack:**
- Rust 2021 edition, MSRV 1.75
- `pulldown-cmark` 0.10 — CommonMark parser
- `tera` 1.19 — template engine
- `serde` 1, `serde_yaml` 0.9, `serde_json` 1, `toml` 0.8 — data formats
- `image` 0.25, `webp` 0.3 — image pipeline
- `blake3` 1.5 — content hashing
- `clap` 4 — CLI
- `thiserror` 1 — library errors
- `anyhow` 1 — top-level errors
- `proptest` 1 — property tests
- `tempfile` 3 — test fixtures
- `walkdir` 2 — directory traversal
- `chrono` 0.4 — dates for feed/sitemap
- `quick-xml` 0.31 — feed and sitemap generation
- `include_dir` 0.7 — embedding the default theme in the binary

---

## Reference: spec
Implementation plan for the spec at `docs/superpowers/specs/2026-04-18-lopress-design.md`. When a task says "per spec §X.Y", read that spec section for the design intent.

---

## File Structure

```
Cargo.toml                                      # workspace root
rust-toolchain.toml                             # pin MSRV
.gitignore                                      # target/, www/
crates/
  lopress-core/
    Cargo.toml
    src/
      lib.rs                                    # public API re-exports
      types.rs                                  # Document, Block, FrontMatter
      frontmatter.rs                            # parse ---yaml--- header
      parser.rs                                 # md -> Document
      serializer.rs                             # Document -> md
      delimiter.rs                              # HTML-comment block tokenizer
      error.rs                                  # ParseError, etc.
    tests/
      roundtrip.rs                              # golden + proptest
      fixtures/                                 # *.md inputs
  lopress-plugin/
    Cargo.toml
    src/
      lib.rs
      manifest.rs                               # plugin.toml types + parser
      registry.rs                               # collection of loaded plugins
      loader.rs                                 # scan plugins/ dir
      error.rs
    tests/
      fixtures/
        valid-plugin/
        invalid-plugin/
  lopress-theme/
    Cargo.toml
    src/
      lib.rs
      engine.rs                                 # tera wrapper, context building
      builtin.rs                                # default theme via include_dir!
      resolver.rs                               # active theme resolution
      context.rs                                # SiteContext, PageContext types
      error.rs
    assets/
      default-theme/                            # embedded at compile time
        plugin.toml
        templates/
          layout.html
          post.html
          page.html
          index.html
          tag.html
          404.html
        theme.css
  lopress-assets/
    Cargo.toml
    src/
      lib.rs
      image.rs                                  # variant generation
      cache.rs                                  # hash-keyed variant cache
      error.rs
  lopress-build/
    Cargo.toml
    src/
      lib.rs
      site.rs                                   # lopress.toml loader + SiteConfig
      build.rs                                  # top-level build(workspace) entry
      render.rs                                 # block tree -> HTML
      pages.rs                                  # post/page/index/tag rendering
      feed.rs                                   # feed.xml generator
      sitemap.rs                                # sitemap.xml generator
      robots.rs                                 # robots.txt generator
      meta.rs                                   # OG/Twitter/canonical tags
      cache.rs                                  # .lopress-cache.json
      error.rs
    tests/
      fixtures/
        minimal/
        with-plugin/
        with-images/
        with-draft/
      expected/
        minimal/
        with-plugin/
        with-images/
        with-draft/
      build_integration.rs
src/
  main.rs                                       # clap CLI: build, new
```

---

## Task 1: Scaffold cargo workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Write workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/lopress-core",
    "crates/lopress-plugin",
    "crates/lopress-theme",
    "crates/lopress-assets",
    "crates/lopress-build",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "TBD"
repository = "https://github.com/kdiedrick/lopress"

[workspace.dependencies]
anyhow = "1"
blake3 = "1.5"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
image = "0.25"
include_dir = "0.7"
proptest = "1"
pulldown-cmark = "0.10"
quick-xml = "0.31"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tempfile = "3"
tera = "1.19"
thiserror = "1"
toml = "0.8"
walkdir = "2"
webp = "0.3"

[workspace.package.authors]
```

Also create `src/main.rs` binary at workspace root. Add to workspace `Cargo.toml`:

```toml
[package]
name = "lopress"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "lopress"
path = "src/main.rs"

[dependencies]
anyhow = { workspace = true }
clap = { workspace = true }
lopress-build = { path = "crates/lopress-build" }
```

- [ ] **Step 2: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.75"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Update `.gitignore`**

Append to existing `.gitignore`:

```
/target
**/*.rs.bk
Cargo.lock
/www
/**/www
/.superpowers
```

(Keep existing `.superpowers/` entry if present — `replace_all` is fine on that line.)

- [ ] **Step 4: Create stub `src/main.rs`**

```rust
fn main() {
    println!("lopress");
}
```

- [ ] **Step 5: Create each crate's stub**

For each of `lopress-core`, `lopress-plugin`, `lopress-theme`, `lopress-assets`, `lopress-build`, create `crates/<name>/Cargo.toml`:

```toml
[package]
name = "lopress-core"                # replace per crate
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# filled in per task

[dev-dependencies]
# filled in per task
```

And `crates/<name>/src/lib.rs`:

```rust
// stub — contents added in later tasks
```

- [ ] **Step 6: Verify it builds**

Run: `cargo build --workspace`
Expected: compiles with zero errors and zero warnings.

Run: `cargo test --workspace`
Expected: `0 tests` for every crate; all pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore src/main.rs crates/
git commit -m "scaffold cargo workspace with five internal crates"
```

---

## Task 2: lopress-core — core types

**Files:**
- Modify: `crates/lopress-core/Cargo.toml`
- Create: `crates/lopress-core/src/types.rs`
- Create: `crates/lopress-core/src/error.rs`
- Modify: `crates/lopress-core/src/lib.rs`

- [ ] **Step 1: Add dependencies to `crates/lopress-core/Cargo.toml`**

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
pulldown-cmark = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
```

- [ ] **Step 2: Write `crates/lopress-core/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("front-matter error: {0}")]
    FrontMatter(String),

    #[error("invalid YAML in front-matter: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("invalid JSON in block attrs at line {line}: {message}")]
    BlockAttrs { line: usize, message: String },

    #[error("unterminated block `{block_type}` opened at line {line}")]
    UnterminatedBlock { block_type: String, line: usize },

    #[error("mismatched block close: expected `{expected}`, got `{actual}` at line {line}")]
    MismatchedClose { expected: String, actual: String, line: usize },
}
```

- [ ] **Step 3: Write `crates/lopress-core/src/types.rs`**

```rust
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A parsed markdown file: front-matter plus the root block tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub front_matter: FrontMatter,
    pub blocks: Vec<Block>,
}

/// Front-matter fields. Unknown fields are captured in `extra` so plugins can
/// read them without the core having to know about them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FrontMatter {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

/// One node in the block tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// e.g. "paragraph", "heading", "lopress:video"
    pub r#type: String,
    /// Structured attributes. For headings: `{"level": 2}`. For custom blocks:
    /// whatever JSON the user wrote in the opening comment.
    #[serde(default = "empty_attrs")]
    pub attrs: Value,
    /// Nested blocks (for containers like `columns`, `callout`).
    #[serde(default)]
    pub children: Vec<Block>,
    /// Raw inline text for text-like blocks (paragraph, heading, code-block
    /// body). `None` for container blocks.
    #[serde(default)]
    pub text: Option<String>,
}

fn empty_attrs() -> Value {
    Value::Object(serde_json::Map::new())
}

impl Block {
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self {
            r#type: "paragraph".into(),
            attrs: empty_attrs(),
            children: vec![],
            text: Some(text.into()),
        }
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        Self {
            r#type: "heading".into(),
            attrs: serde_json::json!({ "level": level }),
            children: vec![],
            text: Some(text.into()),
        }
    }
}
```

- [ ] **Step 4: Wire up `crates/lopress-core/src/lib.rs`**

```rust
pub mod error;
pub mod types;

pub use error::ParseError;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 5: Write a smoke test**

Add to `crates/lopress-core/src/types.rs` at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_constructor_sets_text() {
        let b = Block::paragraph("hello");
        assert_eq!(b.r#type, "paragraph");
        assert_eq!(b.text.as_deref(), Some("hello"));
        assert!(b.children.is_empty());
    }

    #[test]
    fn heading_constructor_sets_level() {
        let b = Block::heading(2, "title");
        assert_eq!(b.r#type, "heading");
        assert_eq!(b.attrs, serde_json::json!({ "level": 2 }));
    }

    #[test]
    fn document_roundtrips_through_json() {
        let d = Document {
            front_matter: FrontMatter {
                title: Some("t".into()),
                ..Default::default()
            },
            blocks: vec![Block::paragraph("p")],
        };
        let s = serde_json::to_string(&d).unwrap();
        let d2: Document = serde_json::from_str(&s).unwrap();
        assert_eq!(d, d2);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p lopress-core`
Expected: 3 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: Document, Block, FrontMatter types"
```

---

## Task 3: lopress-core — front-matter parser

**Files:**
- Create: `crates/lopress-core/src/frontmatter.rs`
- Modify: `crates/lopress-core/src/lib.rs`

- [ ] **Step 1: Write failing tests in `crates/lopress-core/src/frontmatter.rs`**

```rust
use crate::error::ParseError;
use crate::types::FrontMatter;

/// Split `(front_matter, body)` from raw markdown. Returns the parsed
/// front-matter and the body content with leading `---\n...---\n` removed.
/// If there is no front-matter block, returns the default FrontMatter and
/// the input unchanged.
pub fn split(input: &str) -> Result<(FrontMatter, &str), ParseError> {
    // Implementation below; tests first.
    let _ = input;
    Err(ParseError::FrontMatter("not implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_front_matter_returns_default_and_full_body() {
        let (fm, body) = split("# hello\n").unwrap();
        assert_eq!(fm, FrontMatter::default());
        assert_eq!(body, "# hello\n");
    }

    #[test]
    fn parses_title_and_tags() {
        let input = "---\ntitle: Hi\ntags: [a, b]\n---\n# body\n";
        let (fm, body) = split(input).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Hi"));
        assert_eq!(fm.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(body, "# body\n");
    }

    #[test]
    fn parses_draft_and_date() {
        let input = "---\ndraft: true\ndate: 2026-04-18\n---\nbody\n";
        let (fm, body) = split(input).unwrap();
        assert!(fm.draft);
        assert_eq!(fm.date.map(|d| d.to_string()), Some("2026-04-18".into()));
        assert_eq!(body, "body\n");
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        let input = "---\ntitle: oops\n# body\n";
        assert!(split(input).is_err());
    }

    #[test]
    fn extra_fields_captured_in_extra() {
        let input = "---\ntitle: t\ncustom: value\n---\nbody\n";
        let (fm, _) = split(input).unwrap();
        assert_eq!(fm.extra.get("custom").and_then(|v| v.as_str()), Some("value"));
    }
}
```

- [ ] **Step 2: Add `pub mod frontmatter;` to `crates/lopress-core/src/lib.rs`**

Edit `crates/lopress-core/src/lib.rs`:

```rust
pub mod error;
pub mod frontmatter;
pub mod types;

pub use error::ParseError;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 3: Run to confirm failures**

Run: `cargo test -p lopress-core frontmatter`
Expected: 5 tests, all fail with "not implemented" or similar.

- [ ] **Step 4: Implement `split`**

Replace the stub `pub fn split` in `crates/lopress-core/src/frontmatter.rs`:

```rust
pub fn split(input: &str) -> Result<(FrontMatter, &str), ParseError> {
    // A front-matter block starts with a line that is exactly "---" and ends
    // with a subsequent line that is exactly "---". The content between is YAML.
    let trimmed = input.strip_prefix('\u{FEFF}').unwrap_or(input); // BOM tolerance
    if !trimmed.starts_with("---\n") && trimmed != "---" {
        return Ok((FrontMatter::default(), input));
    }
    let after_open = &trimmed[4..]; // skip "---\n"
    let close = after_open
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1; // assume trailing \n
            Some((start, line))
        })
        .find(|(_, line)| *line == "---")
        .ok_or_else(|| ParseError::FrontMatter("unterminated front-matter".into()))?;

    let (close_offset, _) = close;
    let yaml_src = &after_open[..close_offset];
    let fm: FrontMatter = if yaml_src.trim().is_empty() {
        FrontMatter::default()
    } else {
        serde_yaml::from_str(yaml_src)?
    };
    let body_start = 4 + close_offset + "---\n".len();
    let body = if body_start >= trimmed.len() {
        ""
    } else {
        &trimmed[body_start..]
    };
    Ok((fm, body))
}
```

Note on correctness: `lines()` does not include the terminating `\n`, so the scan logic adds `line.len() + 1` per line, which is correct for `\n`-terminated input. For CRLF input we'd need a richer tokenizer; v1 assumes LF. If tests are run on Windows and fail because of CRLF, normalize line endings in the parser — but keep v1 LF-only behaviour; fix in a later iteration if needed.

- [ ] **Step 5: Run tests**

Run: `cargo test -p lopress-core frontmatter`
Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: front-matter parser"
```

---

## Task 4: lopress-core — HTML-comment block delimiter pre-pass

**Files:**
- Create: `crates/lopress-core/src/delimiter.rs`
- Modify: `crates/lopress-core/src/lib.rs`

Per spec §3.2, custom blocks are delimited by HTML comments of the form `<!-- lopress:<name> <json>? -->` and `<!-- /lopress:<name> -->`. This task implements a tokenizer that recognizes those delimiters and leaves everything else alone. The markdown body is later parsed into "segments" between delimiters.

- [ ] **Step 1: Write tests first**

Create `crates/lopress-core/src/delimiter.rs`:

```rust
use crate::error::ParseError;

/// A delimiter token found in source.
#[derive(Debug, Clone, PartialEq)]
pub enum Delim {
    Open { name: String, attrs_json: String, span: (usize, usize) },
    Close { name: String, span: (usize, usize) },
}

/// Scan `src` for lopress block delimiters. Returns them in source order.
/// Non-lopress HTML comments are ignored.
pub fn scan(src: &str) -> Result<Vec<Delim>, ParseError> {
    let _ = src;
    Err(ParseError::FrontMatter("not implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_delimiters_in_plain_markdown() {
        assert!(scan("# hello\n\nparagraph\n").unwrap().is_empty());
    }

    #[test]
    fn self_closing_block_produces_open_and_close() {
        let src = r#"<!-- lopress:video {"src":"a.mp4"} -->
<!-- /lopress:video -->"#;
        let ds = scan(src).unwrap();
        assert_eq!(ds.len(), 2);
        match &ds[0] {
            Delim::Open { name, attrs_json, .. } => {
                assert_eq!(name, "video");
                assert_eq!(attrs_json, r#"{"src":"a.mp4"}"#);
            }
            _ => panic!("expected Open"),
        }
        match &ds[1] {
            Delim::Close { name, .. } => assert_eq!(name, "video"),
            _ => panic!("expected Close"),
        }
    }

    #[test]
    fn open_without_attrs_parses_cleanly() {
        let src = "<!-- lopress:callout -->\nhi\n<!-- /lopress:callout -->";
        let ds = scan(src).unwrap();
        assert_eq!(ds.len(), 2);
        if let Delim::Open { name, attrs_json, .. } = &ds[0] {
            assert_eq!(name, "callout");
            assert_eq!(attrs_json, "");
        } else {
            panic!("expected Open");
        }
    }

    #[test]
    fn non_lopress_comments_ignored() {
        let src = "<!-- just a comment -->\ntext\n<!-- another -->";
        assert!(scan(src).unwrap().is_empty());
    }

    #[test]
    fn nested_delimiters_preserved_in_order() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let ds = scan(src).unwrap();
        let names: Vec<_> = ds
            .iter()
            .map(|d| match d {
                Delim::Open { name, .. } => format!("+{}", name),
                Delim::Close { name, .. } => format!("-{}", name),
            })
            .collect();
        assert_eq!(
            names,
            vec!["+columns", "+column", "-column", "+column", "-column", "-columns"]
        );
    }
}
```

- [ ] **Step 2: Add `pub mod delimiter;` to `crates/lopress-core/src/lib.rs`**

```rust
pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 3: Run to confirm failures**

Run: `cargo test -p lopress-core delimiter`
Expected: 5 tests, all fail.

- [ ] **Step 4: Implement `scan`**

Replace the stub in `crates/lopress-core/src/delimiter.rs`:

```rust
pub fn scan(src: &str) -> Result<Vec<Delim>, ParseError> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let rest = &src[i + 4..];
            let end_off = match rest.find("-->") {
                Some(o) => o,
                None => break, // unterminated comment; leave for pulldown-cmark
            };
            let inner = rest[..end_off].trim();
            let span = (i, i + 4 + end_off + 3);

            if let Some(after_lop) = inner.strip_prefix("lopress:") {
                let (name, attrs_json) = split_name_and_attrs(after_lop);
                out.push(Delim::Open {
                    name,
                    attrs_json,
                    span,
                });
            } else if let Some(after_slash) = inner.strip_prefix("/lopress:") {
                let name = after_slash.trim().to_string();
                if name.is_empty() {
                    return Err(ParseError::FrontMatter(format!(
                        "empty close delimiter at byte {}",
                        i
                    )));
                }
                out.push(Delim::Close { name, span });
            }
            i = span.1;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

/// Split `"<name> [<json>]"` into the name and the JSON string (empty if absent).
fn split_name_and_attrs(s: &str) -> (String, String) {
    let s = s.trim();
    match s.find(|c: char| c.is_whitespace() || c == '{') {
        Some(split) if s.as_bytes()[split] == b'{' => {
            // `foo {...}` — name ends at the space or brace
            let name = s[..split].trim().to_string();
            let attrs = s[split..].trim().to_string();
            (name, attrs)
        }
        Some(split) => {
            let name = s[..split].to_string();
            let attrs = s[split..].trim().to_string();
            (name, attrs)
        }
        None => (s.to_string(), String::new()),
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p lopress-core delimiter`
Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: HTML-comment block delimiter scanner"
```

---

## Task 5: lopress-core — markdown parser for standard blocks

**Files:**
- Create: `crates/lopress-core/src/parser.rs`
- Modify: `crates/lopress-core/src/lib.rs`

This task parses plain CommonMark (paragraphs, headings, lists, code, quotes, images/links) into the `Block` tree. Custom-block handling comes in Task 6.

- [ ] **Step 1: Write failing tests**

Create `crates/lopress-core/src/parser.rs`:

```rust
use crate::delimiter;
use crate::error::ParseError;
use crate::frontmatter;
use crate::types::{Block, Document, FrontMatter};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::{json, Value};

/// Parse a markdown source (with optional front-matter) into a Document.
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let _ = src;
    Ok(Document {
        front_matter: FrontMatter::default(),
        blocks: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|b| b.r#type.as_str()).collect()
    }

    #[test]
    fn parses_front_matter_and_single_paragraph() {
        let d = parse("---\ntitle: X\n---\nhello\n").unwrap();
        assert_eq!(d.front_matter.title.as_deref(), Some("X"));
        assert_eq!(types(&d.blocks), vec!["paragraph"]);
        assert_eq!(d.blocks[0].text.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_heading_level() {
        let d = parse("## H2 heading\n").unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].r#type, "heading");
        assert_eq!(d.blocks[0].attrs, json!({"level": 2}));
        assert_eq!(d.blocks[0].text.as_deref(), Some("H2 heading"));
    }

    #[test]
    fn parses_unordered_list() {
        let d = parse("- one\n- two\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["list"]);
        assert_eq!(d.blocks[0].attrs, json!({"ordered": false}));
        assert_eq!(d.blocks[0].children.len(), 2);
        assert_eq!(d.blocks[0].children[0].r#type, "list_item");
    }

    #[test]
    fn parses_fenced_code_block_with_language() {
        let d = parse("```rust\nfn main() {}\n```\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["code_block"]);
        assert_eq!(d.blocks[0].attrs, json!({"lang": "rust"}));
        assert_eq!(d.blocks[0].text.as_deref(), Some("fn main() {}\n"));
    }

    #[test]
    fn parses_blockquote_containing_paragraph() {
        let d = parse("> quoted\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["quote"]);
        assert_eq!(d.blocks[0].children.len(), 1);
        assert_eq!(d.blocks[0].children[0].r#type, "paragraph");
    }

    #[test]
    fn parses_image_block_from_standalone_markdown_image() {
        // A paragraph whose only content is an image becomes an image block.
        let d = parse("![alt](foo.jpg)\n").unwrap();
        assert_eq!(types(&d.blocks), vec!["image"]);
        assert_eq!(d.blocks[0].attrs, json!({"src": "foo.jpg", "alt": "alt"}));
    }
}
```

- [ ] **Step 2: Add `pub mod parser;` to `crates/lopress-core/src/lib.rs`** and re-export:

```rust
pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod parser;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use parser::parse;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 3: Run to confirm failures**

Run: `cargo test -p lopress-core parser`
Expected: 6 failures.

- [ ] **Step 4: Implement `parse`**

Replace the stub with a full implementation:

```rust
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let (front_matter, body) = frontmatter::split(src)?;
    let delims = delimiter::scan(body)?;

    // Phase 1 (this task): treat delimiters as opaque HTML and rely on
    // pulldown-cmark to pass them through. Task 6 takes over the body and
    // splits it into segments around the delimiter spans so custom blocks
    // become first-class Block nodes. For now we just ignore delimiters.
    let _ = delims;

    let mut parser = Parser::new(body);
    let blocks = parse_blocks(&mut parser, None)?;
    Ok(Document {
        front_matter,
        blocks,
    })
}

/// Build Blocks from events until `stop` appears (or end of stream).
fn parse_blocks(
    parser: &mut Parser<'_>,
    stop: Option<TagEnd>,
) -> Result<Vec<Block>, ParseError> {
    let mut blocks = Vec::new();
    while let Some(event) = parser.next() {
        if let Event::End(ref end) = event {
            if Some(end) == stop.as_ref() {
                return Ok(blocks);
            }
        }
        if let Some(block) = parse_one(event, parser)? {
            blocks.push(block);
        }
    }
    Ok(blocks)
}

fn parse_one(
    event: Event<'_>,
    parser: &mut Parser<'_>,
) -> Result<Option<Block>, ParseError> {
    Ok(Some(match event {
        Event::Start(Tag::Paragraph) => {
            let (text, image) = consume_inline(parser, TagEnd::Paragraph);
            if let Some(img) = image {
                img
            } else {
                Block {
                    r#type: "paragraph".into(),
                    attrs: json!({}),
                    children: vec![],
                    text: Some(text),
                }
            }
        }
        Event::Start(Tag::Heading { level, .. }) => {
            let lvl = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            };
            let (text, _) = consume_inline(parser, TagEnd::Heading(level));
            Block {
                r#type: "heading".into(),
                attrs: json!({ "level": lvl }),
                children: vec![],
                text: Some(text),
            }
        }
        Event::Start(Tag::BlockQuote(_)) => {
            let children = parse_blocks(parser, Some(TagEnd::BlockQuote))?;
            Block {
                r#type: "quote".into(),
                attrs: json!({}),
                children,
                text: None,
            }
        }
        Event::Start(Tag::CodeBlock(kind)) => {
            let lang = match kind {
                CodeBlockKind::Fenced(l) => l.to_string(),
                CodeBlockKind::Indented => String::new(),
            };
            let mut body = String::new();
            while let Some(ev) = parser.next() {
                match ev {
                    Event::Text(t) => body.push_str(&t),
                    Event::End(TagEnd::CodeBlock) => break,
                    _ => {}
                }
            }
            Block {
                r#type: "code_block".into(),
                attrs: if lang.is_empty() {
                    json!({})
                } else {
                    json!({ "lang": lang })
                },
                children: vec![],
                text: Some(body),
            }
        }
        Event::Start(Tag::List(first)) => {
            let ordered = first.is_some();
            let children = parse_blocks(parser, Some(TagEnd::List(ordered)))?;
            Block {
                r#type: "list".into(),
                attrs: json!({ "ordered": ordered }),
                children,
                text: None,
            }
        }
        Event::Start(Tag::Item) => {
            let children = parse_blocks(parser, Some(TagEnd::Item))?;
            Block {
                r#type: "list_item".into(),
                attrs: json!({}),
                children,
                text: None,
            }
        }
        Event::Html(_) | Event::InlineHtml(_) | Event::Text(_) | Event::Code(_)
        | Event::SoftBreak | Event::HardBreak | Event::Rule | Event::TaskListMarker(_)
        | Event::FootnoteReference(_) | Event::Start(_) | Event::End(_) => {
            // Not a block-level start — ignored at this level for v1.
            return Ok(None);
        }
    }))
}

/// Walk inline events until `end`, collecting the text. If the paragraph
/// contains exactly one image and nothing else, return it as an image block.
fn consume_inline(parser: &mut Parser<'_>, end: TagEnd) -> (String, Option<Block>) {
    let mut text = String::new();
    let mut only_image: Option<Block> = None;
    let mut other_text = false;
    while let Some(ev) = parser.next() {
        match ev {
            Event::Text(t) => {
                other_text = other_text || !t.trim().is_empty();
                text.push_str(&t);
            }
            Event::Code(t) => {
                other_text = true;
                text.push('`');
                text.push_str(&t);
                text.push('`');
            }
            Event::SoftBreak => text.push('\n'),
            Event::HardBreak => text.push('\n'),
            Event::Start(Tag::Image { dest_url, title: _, id: _, .. }) => {
                let src = dest_url.to_string();
                // consume alt text events until Image end
                let mut alt = String::new();
                while let Some(inner) = parser.next() {
                    match inner {
                        Event::Text(t) => alt.push_str(&t),
                        Event::End(TagEnd::Image) => break,
                        _ => {}
                    }
                }
                only_image = Some(Block {
                    r#type: "image".into(),
                    attrs: json!({ "src": src, "alt": alt }),
                    children: vec![],
                    text: None,
                });
            }
            Event::Start(Tag::Link { .. }) => {
                other_text = true;
                // preserve [text](url) as-is in the text by rebuilding later;
                // for v1 we capture just the link text.
                while let Some(inner) = parser.next() {
                    match inner {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::Link) => break,
                        _ => {}
                    }
                }
            }
            Event::Start(Tag::Emphasis) => text.push('*'),
            Event::End(TagEnd::Emphasis) => text.push('*'),
            Event::Start(Tag::Strong) => text.push_str("**"),
            Event::End(TagEnd::Strong) => text.push_str("**"),
            Event::End(ref e) if *e == end => break,
            _ => {}
        }
    }
    if !other_text && only_image.is_some() {
        (text, only_image)
    } else {
        (text, None)
    }
}
```

Note: this is a v1 implementation that preserves bold/italic markers in-text and ignores tables, footnotes, and strikethrough. Extending the inline model is a later-phase concern; for now `paragraph.text` is the authoritative markdown-ish string for that paragraph.

- [ ] **Step 5: Run tests**

Run: `cargo test -p lopress-core parser`
Expected: 6 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: markdown parser for standard blocks"
```

---

## Task 6: lopress-core — custom-block segmentation

Wire the delimiter scanner into the parser: before invoking `pulldown-cmark` on the full body, cut the body into segments divided by lopress delimiters, parse each segment independently, and assemble a nested block tree.

**Files:**
- Modify: `crates/lopress-core/src/parser.rs`

- [ ] **Step 1: Add failing tests**

Append to the `mod tests` block in `crates/lopress-core/src/parser.rs`:

```rust
    #[test]
    fn parses_self_closing_custom_block() {
        let src = r#"before

<!-- lopress:video {"src":"a.mp4"} -->
<!-- /lopress:video -->

after
"#;
        let d = parse(src).unwrap();
        let names: Vec<&str> = d.blocks.iter().map(|b| b.r#type.as_str()).collect();
        assert_eq!(names, vec!["paragraph", "lopress:video", "paragraph"]);
        assert_eq!(
            d.blocks[1].attrs,
            json!({"src":"a.mp4"})
        );
        assert!(d.blocks[1].children.is_empty());
    }

    #[test]
    fn parses_custom_block_with_inner_markdown() {
        let src = "<!-- lopress:callout {\"kind\":\"warning\"} -->\nbody para\n<!-- /lopress:callout -->\n";
        let d = parse(src).unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].r#type, "lopress:callout");
        assert_eq!(d.blocks[0].attrs, json!({"kind": "warning"}));
        assert_eq!(d.blocks[0].children.len(), 1);
        assert_eq!(d.blocks[0].children[0].r#type, "paragraph");
    }

    #[test]
    fn parses_nested_columns() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let d = parse(src).unwrap();
        assert_eq!(d.blocks.len(), 1);
        let cols = &d.blocks[0];
        assert_eq!(cols.r#type, "lopress:columns");
        assert_eq!(cols.children.len(), 2);
        for col in &cols.children {
            assert_eq!(col.r#type, "lopress:column");
            assert_eq!(col.children.len(), 1);
        }
    }

    #[test]
    fn mismatched_close_is_error() {
        let src = "<!-- lopress:a -->\n<!-- /lopress:b -->\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn unterminated_open_is_error() {
        let src = "<!-- lopress:a -->\nhi\n";
        assert!(parse(src).is_err());
    }
```

- [ ] **Step 2: Run to confirm new failures**

Run: `cargo test -p lopress-core parser`
Expected: 5 new failures; the original 6 still pass.

- [ ] **Step 3: Replace `parse` with the segmenting implementation**

In `crates/lopress-core/src/parser.rs`:

```rust
pub fn parse(src: &str) -> Result<Document, ParseError> {
    let (front_matter, body) = frontmatter::split(src)?;
    let blocks = parse_body(body)?;
    Ok(Document {
        front_matter,
        blocks,
    })
}

fn parse_body(body: &str) -> Result<Vec<Block>, ParseError> {
    let delims = delimiter::scan(body)?;
    if delims.is_empty() {
        return parse_plain_markdown(body);
    }

    // Walk delimiters and build a nested tree. Between delimiters, parse the
    // slice as plain markdown.
    build_tree(body, &delims, &mut 0, None)
}

/// Recursive tree builder.
///
/// `cursor` tracks the current byte offset in `body`. When `expected_close` is
/// `Some("foo")`, the function returns when it hits that closing delimiter
/// (consuming it); otherwise it returns at the end of the slice.
fn build_tree(
    body: &str,
    delims: &[delimiter::Delim],
    idx: &mut usize,
    expected_close: Option<&str>,
) -> Result<Vec<Block>, ParseError> {
    let mut out = Vec::new();
    let mut seg_start: usize = cursor(delims, *idx, body, expected_close);

    while *idx < delims.len() {
        let d = delims[*idx].clone();
        match d {
            delimiter::Delim::Open { name, attrs_json, span } => {
                // Flush plain markdown before the open.
                if seg_start < span.0 {
                    out.extend(parse_plain_markdown(&body[seg_start..span.0])?);
                }
                *idx += 1;
                let children = build_tree(body, delims, idx, Some(&name))?;
                let attrs = if attrs_json.is_empty() {
                    Value::Object(Default::default())
                } else {
                    serde_json::from_str(&attrs_json).map_err(|e| {
                        ParseError::BlockAttrs {
                            line: line_of(body, span.0),
                            message: e.to_string(),
                        }
                    })?
                };
                out.push(Block {
                    r#type: format!("lopress:{}", name),
                    attrs,
                    children,
                    text: None,
                });
                seg_start = next_delim_start(delims, *idx, body).unwrap_or(body.len());
                // after a close, continue from the close's end
                if *idx > 0 {
                    if let delimiter::Delim::Close { span: cspan, .. } = &delims[*idx - 1] {
                        seg_start = cspan.1;
                    }
                }
            }
            delimiter::Delim::Close { name, span } => {
                match expected_close {
                    Some(exp) if exp == name => {
                        // Flush tail segment before the close.
                        if seg_start < span.0 {
                            out.extend(parse_plain_markdown(&body[seg_start..span.0])?);
                        }
                        *idx += 1;
                        return Ok(out);
                    }
                    Some(exp) => {
                        return Err(ParseError::MismatchedClose {
                            expected: exp.to_string(),
                            actual: name,
                            line: line_of(body, span.0),
                        });
                    }
                    None => {
                        return Err(ParseError::MismatchedClose {
                            expected: "<none>".into(),
                            actual: name,
                            line: line_of(body, span.0),
                        });
                    }
                }
            }
        }
    }

    if let Some(exp) = expected_close {
        return Err(ParseError::UnterminatedBlock {
            block_type: exp.to_string(),
            line: 0,
        });
    }

    // Flush trailing markdown.
    if seg_start < body.len() {
        out.extend(parse_plain_markdown(&body[seg_start..])?);
    }

    Ok(out)
}

fn cursor(
    delims: &[delimiter::Delim],
    idx: usize,
    body: &str,
    _expected_close: Option<&str>,
) -> usize {
    if idx >= delims.len() {
        return body.len();
    }
    match &delims[idx] {
        delimiter::Delim::Open { span, .. } | delimiter::Delim::Close { span, .. } => {
            // The caller treats everything before the first delimiter as plain text.
            // Return 0 so the initial segment covers [0..span.0).
            let _ = span;
            0
        }
    }
}

fn next_delim_start(delims: &[delimiter::Delim], idx: usize, body: &str) -> Option<usize> {
    match delims.get(idx)? {
        delimiter::Delim::Open { span, .. } | delimiter::Delim::Close { span, .. } => {
            Some(span.0)
        }
    }
    .or_else(|| Some(body.len()))
}

fn line_of(src: &str, byte_offset: usize) -> usize {
    src[..byte_offset.min(src.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Rename the original inner parser function used in Task 5 to `parse_plain_markdown`.
fn parse_plain_markdown(body: &str) -> Result<Vec<Block>, ParseError> {
    let mut parser = Parser::new(body);
    parse_blocks(&mut parser, None)
}
```

Also: rename references to the original `parse`-internal flat parsing. The `parse_blocks` / `parse_one` / `consume_inline` functions from Task 5 are unchanged and reused by `parse_plain_markdown`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p lopress-core parser`
Expected: 11 passed (6 from Task 5 + 5 new).

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: segment body around custom-block delimiters"
```

---

## Task 7: lopress-core — markdown serializer

Round-trip serializer: `Document -> String`. The output for a document parsed from a given input is not required to be byte-identical to the input (the spec allows "insignificant whitespace normalization"), but `parse(serialize(parse(x))) == parse(x)` must hold. Proptest in Task 8 will check this.

**Files:**
- Create: `crates/lopress-core/src/serializer.rs`
- Modify: `crates/lopress-core/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/lopress-core/src/serializer.rs`:

```rust
use crate::types::{Block, Document, FrontMatter};
use serde_json::Value;
use std::fmt::Write;

/// Render a Document back to markdown source.
pub fn serialize(doc: &Document) -> String {
    let _ = doc;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn serializes_frontmatter_when_set() {
        let s = serialize(&Document {
            front_matter: FrontMatter {
                title: Some("Hi".into()),
                draft: true,
                ..Default::default()
            },
            blocks: vec![Block::paragraph("hello")],
        });
        assert!(s.starts_with("---\n"));
        assert!(s.contains("title: Hi\n"));
        assert!(s.contains("draft: true\n"));
        assert!(s.ends_with("hello\n"));
    }

    #[test]
    fn omits_frontmatter_when_default() {
        let s = serialize(&Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::paragraph("hi")],
        });
        assert!(!s.starts_with("---"));
    }

    #[test]
    fn serializes_heading_at_right_level() {
        let s = serialize(&Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block::heading(3, "title")],
        });
        assert_eq!(s, "### title\n");
    }

    #[test]
    fn serializes_custom_block_with_attrs() {
        use serde_json::json;
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:video".into(),
                attrs: json!({"src":"a.mp4"}),
                children: vec![],
                text: None,
            }],
        };
        let s = serialize(&doc);
        assert!(s.contains(r#"<!-- lopress:video {"src":"a.mp4"} -->"#));
        assert!(s.contains("<!-- /lopress:video -->"));
    }

    #[test]
    fn roundtrip_simple_doc() {
        let src = "---\ntitle: t\n---\nhello\n\n## section\n";
        let d = parse(src).unwrap();
        let s = serialize(&d);
        let d2 = parse(&s).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn roundtrip_nested_columns() {
        let src = concat!(
            "<!-- lopress:columns {\"count\":2} -->\n",
            "<!-- lopress:column -->\nleft\n<!-- /lopress:column -->\n",
            "<!-- lopress:column -->\nright\n<!-- /lopress:column -->\n",
            "<!-- /lopress:columns -->\n",
        );
        let d = parse(src).unwrap();
        let s = serialize(&d);
        let d2 = parse(&s).unwrap();
        assert_eq!(d, d2);
    }
}
```

- [ ] **Step 2: Add `pub mod serializer;` to `crates/lopress-core/src/lib.rs` and re-export `serialize`**

```rust
pub mod delimiter;
pub mod error;
pub mod frontmatter;
pub mod parser;
pub mod serializer;
pub mod types;

pub use delimiter::{scan as scan_delimiters, Delim};
pub use error::ParseError;
pub use parser::parse;
pub use serializer::serialize;
pub use types::{Block, Document, FrontMatter};
```

- [ ] **Step 3: Run to confirm failures**

Run: `cargo test -p lopress-core serializer`
Expected: 6 failures.

- [ ] **Step 4: Implement `serialize`**

Replace the stub:

```rust
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    if !is_default_frontmatter(&doc.front_matter) {
        let yaml = serde_yaml::to_string(&doc.front_matter)
            .expect("frontmatter yaml serialization cannot fail for known types");
        out.push_str("---\n");
        out.push_str(&yaml);
        if !yaml.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("---\n");
    }
    for (i, b) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_block(&mut out, b, 0);
    }
    out
}

fn is_default_frontmatter(fm: &FrontMatter) -> bool {
    fm.title.is_none()
        && fm.slug.is_none()
        && fm.date.is_none()
        && fm.tags.is_empty()
        && !fm.draft
        && fm.description.is_none()
        && fm.image.is_none()
        && fm.extra.is_empty()
}

fn write_block(out: &mut String, b: &Block, _depth: usize) {
    match b.r#type.as_str() {
        "paragraph" => {
            if let Some(t) = &b.text {
                out.push_str(t);
                if !t.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        "heading" => {
            let level = b.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            for _ in 0..level.max(1) {
                out.push('#');
            }
            out.push(' ');
            if let Some(t) = &b.text {
                out.push_str(t);
            }
            out.push('\n');
        }
        "quote" => {
            for c in &b.children {
                let mut inner = String::new();
                write_block(&mut inner, c, 0);
                for line in inner.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        "code_block" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str("```");
            out.push_str(lang);
            out.push('\n');
            if let Some(t) = &b.text {
                out.push_str(t);
                if !t.ends_with('\n') {
                    out.push('\n');
                }
            }
            out.push_str("```\n");
        }
        "list" => {
            let ordered = b.attrs.get("ordered").and_then(|v| v.as_bool()).unwrap_or(false);
            for (idx, item) in b.children.iter().enumerate() {
                let mut inner = String::new();
                for c in &item.children {
                    write_block(&mut inner, c, 0);
                }
                let marker = if ordered {
                    format!("{}. ", idx + 1)
                } else {
                    "- ".to_string()
                };
                let text = inner.trim_end_matches('\n');
                let mut first = true;
                for line in text.lines() {
                    if first {
                        out.push_str(&marker);
                        first = false;
                    } else {
                        out.push_str("  ");
                    }
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        "image" => {
            let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
            let alt = b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or("");
            let _ = write!(out, "![{}]({})\n", alt, src);
        }
        custom if custom.starts_with("lopress:") => {
            let name = &custom["lopress:".len()..];
            out.push_str("<!-- lopress:");
            out.push_str(name);
            if !is_empty_attrs(&b.attrs) {
                out.push(' ');
                out.push_str(&serde_json::to_string(&b.attrs).unwrap_or_default());
            }
            out.push_str(" -->\n");
            for (i, c) in b.children.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                write_block(out, c, 0);
            }
            out.push_str("<!-- /lopress:");
            out.push_str(name);
            out.push_str(" -->\n");
        }
        _ => {
            // Unknown block — emit as HTML comment placeholder.
            out.push_str("<!-- unknown block: ");
            out.push_str(&b.r#type);
            out.push_str(" -->\n");
        }
    }
}

fn is_empty_attrs(v: &Value) -> bool {
    matches!(v, Value::Object(m) if m.is_empty())
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p lopress-core serializer`
Expected: 6 passed.

Run the full crate suite: `cargo test -p lopress-core`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: markdown serializer"
```

---

## Task 8: lopress-core — proptest round-trip

**Files:**
- Create: `crates/lopress-core/tests/roundtrip.rs`

- [ ] **Step 1: Write the proptest-driven round-trip test**

```rust
use lopress_core::{parse, serialize, Block, Document, FrontMatter};
use proptest::prelude::*;
use serde_json::json;

fn arb_text() -> impl Strategy<Value = String> {
    // ASCII, no newlines, no lopress prefix, no backtick/angle to keep shapes stable.
    "[a-zA-Z0-9 .,!?]{1,40}".prop_filter("no empty", |s| !s.trim().is_empty())
}

fn arb_paragraph() -> impl Strategy<Value = Block> {
    arb_text().prop_map(Block::paragraph)
}

fn arb_heading() -> impl Strategy<Value = Block> {
    (1u8..=6, arb_text()).prop_map(|(lvl, t)| Block::heading(lvl, t))
}

fn arb_custom_block() -> impl Strategy<Value = Block> {
    ("video|callout|note", arb_text()).prop_map(|(name, body)| Block {
        r#type: format!("lopress:{}", name),
        attrs: json!({ "id": body.len() }),
        children: vec![Block::paragraph(body)],
        text: None,
    })
}

fn arb_block() -> impl Strategy<Value = Block> {
    prop_oneof![arb_paragraph(), arb_heading(), arb_custom_block()]
}

fn arb_doc() -> impl Strategy<Value = Document> {
    prop::collection::vec(arb_block(), 1..8).prop_map(|blocks| Document {
        front_matter: FrontMatter::default(),
        blocks,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn serialize_then_parse_is_stable(doc in arb_doc()) {
        let once = serialize(&doc);
        let parsed = parse(&once).unwrap();
        let twice = serialize(&parsed);
        prop_assert_eq!(once, twice);
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p lopress-core --test roundtrip`
Expected: 1 test passes (proptest runs 64 cases).

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-core/
git commit -m "lopress-core: round-trip proptest"
```

---

## Task 9: lopress-plugin — manifest and registry

**Files:**
- Modify: `crates/lopress-plugin/Cargo.toml`
- Create: `crates/lopress-plugin/src/manifest.rs`
- Create: `crates/lopress-plugin/src/registry.rs`
- Create: `crates/lopress-plugin/src/loader.rs`
- Create: `crates/lopress-plugin/src/error.rs`
- Modify: `crates/lopress-plugin/src/lib.rs`

- [ ] **Step 1: Dependencies**

`crates/lopress-plugin/Cargo.toml`:

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
thiserror = { workspace = true }
walkdir = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Error type**

`crates/lopress-plugin/src/error.rs`:

```rust
use thiserror::Error;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("I/O error at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("manifest error at {path}: {message}")]
    Manifest { path: PathBuf, message: String },
    #[error("plugin `{name}` declares template `{template}` but file does not exist")]
    MissingTemplate { name: String, template: String },
    #[error("duplicate block name `{0}` across plugins")]
    DuplicateBlock(String),
}
```

- [ ] **Step 3: Write failing tests for manifest parsing**

`crates/lopress-plugin/src/manifest.rs`:

```rust
use crate::error::PluginError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub theme: bool,
    #[serde(default)]
    pub blocks: Vec<BlockDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockDecl {
    pub name: String,
    pub template: String,
    /// Map attr name -> schema entry.
    #[serde(default)]
    pub attrs: BTreeMap<String, AttrDecl>,
    /// Optional escape hatches (phase-2 — parsed but not yet used).
    #[serde(default)]
    pub renderer: Option<String>,
    #[serde(default)]
    pub editor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AttrType {
    String,
    Number,
    Bool,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttrDecl {
    #[serde(rename = "type")]
    pub kind: AttrType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub ui: Option<String>, // "text", "number", "checkbox", "select", "image-picker", "color"
    #[serde(default)]
    pub options: Vec<String>, // for select
}

pub fn parse_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let src = std::fs::read_to_string(path).map_err(|source| PluginError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest: PluginManifest =
        toml::from_str(&src).map_err(|e| PluginError::Manifest {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, contents: &str) -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn parses_minimal_theme_plugin() {
        let dir = TempDir::new().unwrap();
        let p = write(
            &dir,
            "plugin.toml",
            r#"
name = "default"
version = "0.1.0"
theme = true
"#,
        );
        let m = parse_manifest(&p).unwrap();
        assert_eq!(m.name, "default");
        assert!(m.theme);
        assert!(m.blocks.is_empty());
    }

    #[test]
    fn parses_plugin_with_blocks_and_attrs() {
        let dir = TempDir::new().unwrap();
        let p = write(
            &dir,
            "plugin.toml",
            r#"
name = "video"
version = "0.1.0"

[[blocks]]
name     = "lopress:video"
template = "blocks/video.html"

[blocks.attrs]
src      = { type = "string", required = true,  ui = "text" }
autoplay = { type = "bool",   default  = false, ui = "checkbox" }
"#,
        );
        let m = parse_manifest(&p).unwrap();
        assert_eq!(m.blocks.len(), 1);
        let b = &m.blocks[0];
        assert_eq!(b.name, "lopress:video");
        assert!(b.attrs.contains_key("src"));
        assert_eq!(b.attrs["src"].kind, AttrType::String);
        assert!(b.attrs["src"].required);
        assert_eq!(b.attrs["src"].ui.as_deref(), Some("text"));
    }

    #[test]
    fn errors_on_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let p = write(&dir, "plugin.toml", "this is not toml = = = =");
        assert!(parse_manifest(&p).is_err());
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p lopress-plugin manifest`
Expected: 3 passed.

- [ ] **Step 5: Write registry + loader**

`crates/lopress-plugin/src/registry.rs`:

```rust
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
    /// Map "lopress:video" -> (plugin-index, block-index).
    pub block_index: BTreeMap<String, (usize, usize)>,
    /// Map theme-name -> plugin-index.
    pub theme_index: BTreeMap<String, usize>,
}

impl PluginRegistry {
    pub fn insert(&mut self, plugin: LoadedPlugin) -> Result<(), PluginError> {
        let pi = self.plugins.len();
        if plugin.manifest.theme {
            self.theme_index
                .insert(plugin.manifest.name.clone(), pi);
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
```

- [ ] **Step 6: Loader**

`crates/lopress-plugin/src/loader.rs`:

```rust
use crate::error::PluginError;
use crate::manifest::parse_manifest;
use crate::registry::{LoadedPlugin, PluginRegistry};
use std::path::Path;

/// Scan `<workspace>/plugins/` and load every plugin whose `plugin.toml` parses.
/// `enabled` is the list of plugin names permitted to load; `None` means all.
/// Plugins with names not in `enabled` are skipped.
pub fn load_dir(
    dir: &Path,
    enabled: Option<&[String]>,
) -> Result<PluginRegistry, PluginError> {
    let mut reg = PluginRegistry::default();
    if !dir.exists() {
        return Ok(reg);
    }
    for entry in std::fs::read_dir(dir).map_err(|source| PluginError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| PluginError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let manifest_path = root.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = parse_manifest(&manifest_path)?;
        if let Some(list) = enabled {
            if !list.iter().any(|n| n == &manifest.name) {
                continue;
            }
        }
        // Verify declared template files exist relative to plugin root.
        for block in &manifest.blocks {
            if !root.join(&block.template).exists() {
                return Err(PluginError::MissingTemplate {
                    name: block.name.clone(),
                    template: block.template.clone(),
                });
            }
        }
        reg.insert(LoadedPlugin {
            root,
            manifest,
        })?;
    }
    Ok(reg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_plugin(root: &std::path::Path, name: &str, block: Option<&str>) {
        std::fs::create_dir_all(root).unwrap();
        let mut toml_src = format!("name = \"{}\"\nversion = \"0.1.0\"\n", name);
        if let Some(b) = block {
            toml_src.push_str(&format!(
                "\n[[blocks]]\nname = \"{}\"\ntemplate = \"blocks/x.html\"\n",
                b
            ));
            let tpl = root.join("blocks");
            std::fs::create_dir_all(&tpl).unwrap();
            std::fs::write(tpl.join("x.html"), "<div>x</div>").unwrap();
        }
        std::fs::write(root.join("plugin.toml"), toml_src).unwrap();
    }

    #[test]
    fn missing_plugins_dir_returns_empty_registry() {
        let d = TempDir::new().unwrap();
        let reg = load_dir(&d.path().join("plugins"), None).unwrap();
        assert!(reg.plugins.is_empty());
    }

    #[test]
    fn loads_plugins_and_indexes_blocks() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        make_plugin(&plugins.join("a"), "a", Some("lopress:a-block"));
        make_plugin(&plugins.join("b"), "b", Some("lopress:b-block"));
        let reg = load_dir(&plugins, None).unwrap();
        assert_eq!(reg.plugins.len(), 2);
        assert!(reg.block("lopress:a-block").is_some());
        assert!(reg.block("lopress:b-block").is_some());
    }

    #[test]
    fn respects_enabled_allowlist() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        make_plugin(&plugins.join("a"), "a", None);
        make_plugin(&plugins.join("b"), "b", None);
        let enabled = vec!["a".to_string()];
        let reg = load_dir(&plugins, Some(&enabled)).unwrap();
        assert_eq!(reg.plugins.len(), 1);
        assert_eq!(reg.plugins[0].manifest.name, "a");
    }

    #[test]
    fn missing_template_is_error() {
        let d = TempDir::new().unwrap();
        let plugins = d.path().join("plugins");
        let root = plugins.join("bad");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            r#"name="bad"
version="0.1.0"

[[blocks]]
name="lopress:x"
template="blocks/missing.html"
"#,
        )
        .unwrap();
        let err = load_dir(&plugins, None).unwrap_err();
        matches!(err, PluginError::MissingTemplate { .. });
    }
}
```

- [ ] **Step 7: Wire up `crates/lopress-plugin/src/lib.rs`**

```rust
pub mod error;
pub mod loader;
pub mod manifest;
pub mod registry;

pub use error::PluginError;
pub use loader::load_dir;
pub use manifest::{AttrDecl, AttrType, BlockDecl, PluginManifest};
pub use registry::{LoadedPlugin, PluginRegistry};
```

- [ ] **Step 8: Run**

Run: `cargo test -p lopress-plugin`
Expected: 7 tests pass (3 manifest + 4 loader).

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-plugin/
git commit -m "lopress-plugin: manifest parser, loader, registry"
```

---

## Task 10: lopress-theme — template engine and context types

**Files:**
- Modify: `crates/lopress-theme/Cargo.toml`
- Create: `crates/lopress-theme/src/engine.rs`
- Create: `crates/lopress-theme/src/context.rs`
- Create: `crates/lopress-theme/src/error.rs`
- Modify: `crates/lopress-theme/src/lib.rs`

- [ ] **Step 1: Dependencies**

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
tera = { workspace = true }
thiserror = { workspace = true }
include_dir = { workspace = true }
chrono = { workspace = true }
lopress-core = { path = "../lopress-core" }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Error type**

`crates/lopress-theme/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("template error: {0}")]
    Tera(#[from] tera::Error),
    #[error("missing template `{0}`")]
    MissingTemplate(String),
    #[error("theme `{0}` not found")]
    NotFound(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 3: Context types**

`crates/lopress-theme/src/context.rs`:

```rust
use chrono::NaiveDate;
use serde::Serialize;

/// Top-level context passed to every template render.
#[derive(Debug, Clone, Serialize)]
pub struct RenderContext<'a> {
    pub site: &'a SiteCtx,
    pub page: &'a PageCtx,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub nav: Vec<NavItem>,
    /// Reverse-chronological list of non-draft posts, for index/tag/feed use.
    pub posts: Vec<PostSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavItem {
    pub label: String,
    pub href: String,
}

/// Short form of a post used by index pages and feeds. Excludes full body.
#[derive(Debug, Clone, Serialize)]
pub struct PostSummary {
    pub title: String,
    pub slug: String,
    pub url: String,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub description: Option<String>,
}

/// Context for the page being rendered. Polymorphic: depending on which
/// template is selected, different fields are populated.
#[derive(Debug, Clone, Serialize)]
pub struct PageCtx {
    pub kind: PageKind,
    pub title: String,
    pub slug: String,
    pub url: String,
    pub canonical: String,
    pub description: Option<String>,
    pub og_image: Option<String>,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
    /// Rendered body HTML (for `post.html` / `page.html`).
    pub body_html: String,
    /// Populated only for `tag.html` / `index.html`.
    pub posts: Vec<PostSummary>,
    /// Populated only for `tag.html`.
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Index,
    Post,
    Page,
    Tag,
    NotFound,
}
```

- [ ] **Step 4: Engine wrapper with failing test**

`crates/lopress-theme/src/engine.rs`:

```rust
use crate::error::ThemeError;
use crate::context::RenderContext;
use tera::Tera;

pub struct ThemeEngine {
    tera: Tera,
}

impl ThemeEngine {
    /// Create an engine by loading `*.html` files from the given list of
    /// `(name, contents)` pairs. Names correspond to template lookup keys
    /// like `"layout.html"`, `"post.html"`, etc.
    pub fn from_templates(templates: &[(String, String)]) -> Result<Self, ThemeError> {
        let mut tera = Tera::default();
        tera.add_raw_templates(
            templates
                .iter()
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .collect::<Vec<_>>(),
        )?;
        Ok(Self { tera })
    }

    pub fn render(&self, template: &str, ctx: &RenderContext) -> Result<String, ThemeError> {
        let mut t = tera::Context::new();
        t.insert("site", ctx.site);
        t.insert("page", ctx.page);
        self.tera.render(template, &t).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::*;

    fn site() -> SiteCtx {
        SiteCtx {
            title: "T".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![],
        }
    }

    fn page() -> PageCtx {
        PageCtx {
            kind: PageKind::Post,
            title: "P".into(),
            slug: "p".into(),
            url: "/posts/p/".into(),
            canonical: "https://example.com/posts/p/".into(),
            description: None,
            og_image: None,
            date: None,
            tags: vec![],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        }
    }

    #[test]
    fn renders_minimal_template() {
        let engine = ThemeEngine::from_templates(&[(
            "post.html".into(),
            "<title>{{ page.title }}</title>{{ page.body_html | safe }}".into(),
        )])
        .unwrap();
        let s = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site(),
                    page: &page(),
                },
            )
            .unwrap();
        assert_eq!(s, "<title>P</title><p>body</p>");
    }
}
```

- [ ] **Step 5: Wire `lib.rs`**

```rust
pub mod context;
pub mod engine;
pub mod error;

pub use context::{NavItem, PageCtx, PageKind, PostSummary, RenderContext, SiteCtx};
pub use engine::ThemeEngine;
pub use error::ThemeError;
```

- [ ] **Step 6: Run**

Run: `cargo test -p lopress-theme engine`
Expected: 1 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-theme/
git commit -m "lopress-theme: tera engine wrapper and context types"
```

---

## Task 11: lopress-theme — built-in default theme

**Files:**
- Create: `crates/lopress-theme/assets/default-theme/plugin.toml`
- Create: `crates/lopress-theme/assets/default-theme/templates/layout.html`
- Create: `crates/lopress-theme/assets/default-theme/templates/post.html`
- Create: `crates/lopress-theme/assets/default-theme/templates/page.html`
- Create: `crates/lopress-theme/assets/default-theme/templates/index.html`
- Create: `crates/lopress-theme/assets/default-theme/templates/tag.html`
- Create: `crates/lopress-theme/assets/default-theme/templates/404.html`
- Create: `crates/lopress-theme/assets/default-theme/theme.css`
- Create: `crates/lopress-theme/src/builtin.rs`
- Modify: `crates/lopress-theme/src/lib.rs`

- [ ] **Step 1: Default theme manifest**

`crates/lopress-theme/assets/default-theme/plugin.toml`:

```toml
name = "default"
version = "0.1.0"
theme = true
```

- [ ] **Step 2: Templates**

`crates/lopress-theme/assets/default-theme/templates/layout.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{% block title %}{{ page.title }} — {{ site.title }}{% endblock %}</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
{% if page.description %}<meta name="description" content="{{ page.description }}">{% endif %}
<link rel="canonical" href="{{ page.canonical }}">
<link rel="stylesheet" href="/assets/theme.css">
<meta property="og:title" content="{{ page.title }}">
{% if page.description %}<meta property="og:description" content="{{ page.description }}">{% endif %}
{% if page.og_image %}<meta property="og:image" content="{{ page.og_image }}">{% endif %}
<meta name="twitter:card" content="summary_large_image">
{% block extra_head %}{% endblock %}
</head>
<body>
<header class="site-header">
  <a class="site-title" href="/">{{ site.title }}</a>
  <nav class="site-nav">
    {% for item in site.nav %}<a href="{{ item.href }}">{{ item.label }}</a>{% endfor %}
  </nav>
</header>
<main>
{% block content %}{% endblock %}
</main>
<footer class="site-footer">
  <a href="/feed.xml">RSS</a>
</footer>
</body>
</html>
```

`templates/post.html`:

```html
{% extends "layout.html" %}
{% block content %}
<article class="post">
  <header>
    <h1>{{ page.title }}</h1>
    {% if page.date %}<time datetime="{{ page.date }}">{{ page.date }}</time>{% endif %}
    {% if page.tags %}
      <ul class="tags">
        {% for t in page.tags %}<li><a href="/tags/{{ t }}/">{{ t }}</a></li>{% endfor %}
      </ul>
    {% endif %}
  </header>
  <div class="body">{{ page.body_html | safe }}</div>
</article>
{% endblock %}
```

`templates/page.html`:

```html
{% extends "layout.html" %}
{% block content %}
<article class="page">
  <h1>{{ page.title }}</h1>
  <div class="body">{{ page.body_html | safe }}</div>
</article>
{% endblock %}
```

`templates/index.html`:

```html
{% extends "layout.html" %}
{% block content %}
<section class="index">
  <h1>{{ site.title }}</h1>
  <ul class="post-list">
    {% for p in page.posts %}
    <li>
      <a href="{{ p.url }}">{{ p.title }}</a>
      {% if p.date %}<time datetime="{{ p.date }}">{{ p.date }}</time>{% endif %}
      {% if p.description %}<p class="excerpt">{{ p.description }}</p>{% endif %}
    </li>
    {% endfor %}
  </ul>
</section>
{% endblock %}
```

`templates/tag.html`:

```html
{% extends "layout.html" %}
{% block content %}
<section class="tag">
  <h1>Tagged: {{ page.tag }}</h1>
  <ul class="post-list">
    {% for p in page.posts %}
    <li><a href="{{ p.url }}">{{ p.title }}</a></li>
    {% endfor %}
  </ul>
</section>
{% endblock %}
```

`templates/404.html`:

```html
{% extends "layout.html" %}
{% block title %}Not found — {{ site.title }}{% endblock %}
{% block content %}
<section class="not-found"><h1>404</h1><p>That page doesn't exist.</p></section>
{% endblock %}
```

- [ ] **Step 3: Minimal `theme.css`**

`crates/lopress-theme/assets/default-theme/theme.css`:

```css
:root { --fg: #222; --bg: #fff; --accent: #4a6cf7; }
* { box-sizing: border-box; }
body {
  margin: 0; padding: 0;
  font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Arial, sans-serif;
  color: var(--fg); background: var(--bg);
}
.site-header, main, .site-footer { max-width: 720px; margin: 0 auto; padding: 1rem; }
.site-header { display: flex; justify-content: space-between; align-items: baseline; }
.site-title { font-weight: 600; text-decoration: none; color: inherit; }
.site-nav a { margin-left: 1rem; text-decoration: none; color: var(--accent); }
h1, h2, h3 { line-height: 1.2; }
.post-list { list-style: none; padding: 0; }
.post-list li { margin-bottom: 1.5rem; }
.tags { list-style: none; padding: 0; display: flex; gap: 0.5rem; margin: 0.5rem 0; }
.tags a { font-size: 0.8rem; color: var(--accent); text-decoration: none; }
pre { background: #f5f5f5; padding: 0.75rem; overflow: auto; border-radius: 4px; }
code { background: #f5f5f5; padding: 0.1rem 0.3rem; border-radius: 3px; }
```

- [ ] **Step 4: Loader that reads the embedded directory**

`crates/lopress-theme/src/builtin.rs`:

```rust
use crate::engine::ThemeEngine;
use crate::error::ThemeError;
use include_dir::{include_dir, Dir};

static DEFAULT: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/default-theme");

/// Build a ThemeEngine from the embedded default theme.
pub fn default_engine() -> Result<ThemeEngine, ThemeError> {
    let templates = DEFAULT
        .get_dir("templates")
        .ok_or_else(|| ThemeError::MissingTemplate("templates/".into()))?;
    let mut tpls = Vec::new();
    for entry in templates.files() {
        let name = entry
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let contents = entry.contents_utf8().unwrap_or("").to_string();
        tpls.push((name, contents));
    }
    ThemeEngine::from_templates(&tpls)
}

/// Return the default theme's CSS content.
pub fn default_css() -> &'static str {
    DEFAULT
        .get_file("theme.css")
        .and_then(|f| f.contents_utf8())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::*;

    #[test]
    fn default_engine_renders_post() {
        let engine = default_engine().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![],
        };
        let page = PageCtx {
            kind: PageKind::Post,
            title: "Hi".into(),
            slug: "hi".into(),
            url: "/posts/hi/".into(),
            canonical: "https://example.com/posts/hi/".into(),
            description: Some("d".into()),
            og_image: None,
            date: None,
            tags: vec!["a".into()],
            body_html: "<p>body</p>".into(),
            posts: vec![],
            tag: None,
        };
        let html = engine
            .render(
                "post.html",
                &RenderContext {
                    site: &site,
                    page: &page,
                },
            )
            .unwrap();
        assert!(html.contains("<title>Hi — S</title>"));
        assert!(html.contains("<p>body</p>"));
        assert!(html.contains("href=\"/tags/a/\""));
    }

    #[test]
    fn default_css_is_non_empty() {
        assert!(default_css().contains("body"));
    }
}
```

- [ ] **Step 5: Wire lib.rs**

Add to `crates/lopress-theme/src/lib.rs`:

```rust
pub mod builtin;
pub mod context;
pub mod engine;
pub mod error;

pub use builtin::{default_css, default_engine};
pub use context::{NavItem, PageCtx, PageKind, PostSummary, RenderContext, SiteCtx};
pub use engine::ThemeEngine;
pub use error::ThemeError;
```

- [ ] **Step 6: Run**

Run: `cargo test -p lopress-theme`
Expected: 3 passed (1 engine + 2 builtin).

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-theme/
git commit -m "lopress-theme: built-in default theme (embedded at compile time)"
```

---

## Task 12: lopress-theme — active theme resolution

Resolves an active theme name from `lopress.toml` against the plugin registry, falling back to the built-in when none matches. Handles the name-collision rule per spec §5.5.

**Files:**
- Create: `crates/lopress-theme/src/resolver.rs`
- Modify: `crates/lopress-theme/src/lib.rs`

- [ ] **Step 1: Tests**

`crates/lopress-theme/src/resolver.rs`:

```rust
use crate::builtin::default_engine;
use crate::engine::ThemeEngine;
use crate::error::ThemeError;
use lopress_plugin::PluginRegistry;
use std::path::PathBuf;

pub struct ResolvedTheme {
    pub engine: ThemeEngine,
    /// Path on disk to the theme's `theme.css`, or None if using the built-in.
    pub css_path: Option<PathBuf>,
    /// Raw CSS content (either from disk or the built-in).
    pub css_content: String,
}

pub fn resolve(
    registry: &PluginRegistry,
    theme_name: &str,
) -> Result<ResolvedTheme, ThemeError> {
    // If a plugin with this name exists and is a theme, use it (overriding
    // the built-in when name == "default").
    if let Some(plugin) = registry.theme(theme_name) {
        let templates_dir = plugin.root.join("templates");
        let mut tpls = Vec::new();
        for entry in std::fs::read_dir(&templates_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) != Some("html") {
                continue;
            }
            let name = entry
                .path()
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let contents = std::fs::read_to_string(entry.path())?;
            tpls.push((name, contents));
        }
        let engine = ThemeEngine::from_templates(&tpls)?;
        let css_path = plugin.root.join("theme.css");
        let css_content = std::fs::read_to_string(&css_path).unwrap_or_default();
        return Ok(ResolvedTheme {
            engine,
            css_path: Some(css_path),
            css_content,
        });
    }

    // Fallback: built-in default theme. Only valid when theme_name == "default".
    if theme_name == "default" {
        Ok(ResolvedTheme {
            engine: default_engine()?,
            css_path: None,
            css_content: crate::builtin::default_css().to_string(),
        })
    } else {
        Err(ThemeError::NotFound(theme_name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_name_resolves_to_builtin_when_no_plugin() {
        let reg = PluginRegistry::default();
        let t = resolve(&reg, "default").unwrap();
        assert!(t.css_path.is_none());
        assert!(t.css_content.contains("body"));
    }

    #[test]
    fn unknown_name_errors() {
        let reg = PluginRegistry::default();
        let err = resolve(&reg, "mystery").unwrap_err();
        matches!(err, ThemeError::NotFound(_));
    }
}
```

- [ ] **Step 2: Wire `lib.rs`** — add `pub mod resolver;` and export `pub use resolver::{resolve, ResolvedTheme};`.

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-theme`
Expected: 5 passed (prior 3 + 2 new).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-theme/
git commit -m "lopress-theme: active-theme resolver with built-in fallback"
```

---

## Task 13: lopress-assets — image variant generation with cache

**Files:**
- Modify: `crates/lopress-assets/Cargo.toml`
- Create: `crates/lopress-assets/src/cache.rs`
- Create: `crates/lopress-assets/src/image.rs`
- Create: `crates/lopress-assets/src/error.rs`
- Modify: `crates/lopress-assets/src/lib.rs`

- [ ] **Step 1: Dependencies**

```toml
[dependencies]
blake3 = { workspace = true }
image = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
webp = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Error type**

`crates/lopress-assets/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("image decode error: {0}")]
    Decode(#[from] image::ImageError),
    #[error("webp encode error: {0}")]
    Webp(String),
    #[error("json cache error: {0}")]
    Json(#[from] serde_json::Error),
}
```

- [ ] **Step 3: Cache**

`crates/lopress-assets/src/cache.rs`:

```rust
use crate::error::AssetError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Variant cache: key -> output filename (relative to www/images/).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VariantCache {
    pub entries: BTreeMap<String, String>,
}

impl VariantCache {
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), AssetError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(path, s)?;
        Ok(())
    }

    /// Build a cache key from source hash, target width, and target format.
    pub fn key(source_hash: &str, width: u32, format: &str) -> String {
        format!("{}-{}-{}", source_hash, width, format)
    }
}

/// Hash a file's contents with blake3 and return the hex string.
pub fn hash_file(path: &Path) -> Result<String, AssetError> {
    let bytes = std::fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Default variant for output filenames: `<stem>.<width>w.<ext>`
pub fn variant_filename(stem: &str, width: u32, ext: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}w.{}", stem, width, ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cache_roundtrip_via_json() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("cache.json");
        let mut c = VariantCache::default();
        c.entries.insert("abc-400-webp".into(), "foo.400w.webp".into());
        c.save(&p).unwrap();
        let loaded = VariantCache::load(&p).unwrap();
        assert_eq!(c.entries, loaded.entries);
    }

    #[test]
    fn hash_is_deterministic() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("x.bin");
        std::fs::write(&p, b"hello").unwrap();
        let h1 = hash_file(&p).unwrap();
        let h2 = hash_file(&p).unwrap();
        assert_eq!(h1, h2);
    }
}
```

- [ ] **Step 4: Image pipeline**

`crates/lopress-assets/src/image.rs`:

```rust
use crate::cache::{hash_file, variant_filename, VariantCache};
use crate::error::AssetError;
use image::{ImageReader, DynamicImage, ImageFormat};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct VariantSpec {
    pub widths: Vec<u32>,
    pub webp: bool,
    pub keep_original_format: bool,
}

impl Default for VariantSpec {
    fn default() -> Self {
        Self {
            widths: vec![400, 800, 1600],
            webp: true,
            keep_original_format: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageResult {
    /// Output files, relative to `www/images/`.
    pub files: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub filename: PathBuf,
    pub width: u32,
    pub format: String, // "webp" or original extension
}

/// Generate all variants of `src` into `www_images_dir`, consulting and updating
/// `cache`. Returns the list of variant filenames (not re-encoding cached ones).
pub fn process_image(
    src: &Path,
    www_images_dir: &Path,
    cache: &mut VariantCache,
    spec: &VariantSpec,
) -> Result<ImageResult, AssetError> {
    std::fs::create_dir_all(www_images_dir)?;

    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .to_string();
    let ext = src
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("bin")
        .to_lowercase();
    let hash = hash_file(src)?;

    // Always copy the original through.
    let original_out = www_images_dir.join(format!("{}.{}", stem, ext));
    if !original_out.exists() {
        std::fs::copy(src, &original_out)?;
    }

    let img = ImageReader::open(src)?.with_guessed_format()?.decode()?;
    let mut files = Vec::new();

    for &w in &spec.widths {
        // Upscaling is useless; skip variants wider than original.
        if w >= img.width() {
            continue;
        }
        if spec.webp {
            let key = VariantCache::key(&hash, w, "webp");
            let filename = variant_filename(&stem, w, "webp");
            let out_path = www_images_dir.join(&filename);
            if !cache.entries.contains_key(&key) || !out_path.exists() {
                let resized = img.thumbnail(w, u32::MAX);
                write_webp(&resized, &out_path)?;
                cache.entries.insert(key, filename.to_string_lossy().into());
            }
            files.push(Variant {
                filename,
                width: w,
                format: "webp".into(),
            });
        }
        if spec.keep_original_format {
            let key = VariantCache::key(&hash, w, &ext);
            let filename = variant_filename(&stem, w, &ext);
            let out_path = www_images_dir.join(&filename);
            if !cache.entries.contains_key(&key) || !out_path.exists() {
                let resized = img.thumbnail(w, u32::MAX);
                let format = ImageFormat::from_extension(&ext).unwrap_or(ImageFormat::Jpeg);
                resized.save_with_format(&out_path, format)?;
                cache.entries.insert(key, filename.to_string_lossy().into());
            }
            files.push(Variant {
                filename,
                width: w,
                format: ext.clone(),
            });
        }
    }

    Ok(ImageResult { files })
}

fn write_webp(img: &DynamicImage, out: &Path) -> Result<(), AssetError> {
    let rgba = img.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let encoded = encoder.encode(80.0);
    std::fs::write(out, encoded.iter().copied().collect::<Vec<_>>())
        .map_err(AssetError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use tempfile::TempDir;

    fn make_image(path: &Path, w: u32, h: u32) {
        let mut img = RgbImage::new(w, h);
        for p in img.pixels_mut() { *p = Rgb([200, 100, 50]); }
        img.save(path).unwrap();
    }

    #[test]
    fn produces_expected_variants_and_caches_them() {
        let d = TempDir::new().unwrap();
        let src = d.path().join("src.jpg");
        make_image(&src, 2000, 1500);

        let www = d.path().join("www/images");
        let mut cache = VariantCache::default();
        let spec = VariantSpec::default();

        let r1 = process_image(&src, &www, &mut cache, &spec).unwrap();
        // 3 widths * (webp + original) = 6 variants.
        assert_eq!(r1.files.len(), 6);
        let before = cache.entries.len();

        // Re-run: everything cached, no new entries.
        let _r2 = process_image(&src, &www, &mut cache, &spec).unwrap();
        assert_eq!(cache.entries.len(), before);
    }

    #[test]
    fn skips_widths_wider_than_source() {
        let d = TempDir::new().unwrap();
        let src = d.path().join("small.jpg");
        make_image(&src, 500, 400); // smaller than 800 and 1600

        let www = d.path().join("www/images");
        let mut cache = VariantCache::default();
        let spec = VariantSpec::default();

        let r = process_image(&src, &www, &mut cache, &spec).unwrap();
        // Only the 400 width is narrower; expect 2 variants (webp + jpg).
        assert_eq!(r.files.len(), 2);
    }
}
```

- [ ] **Step 5: Wire `lib.rs`**

```rust
pub mod cache;
pub mod error;
pub mod image;

pub use cache::{hash_file, VariantCache};
pub use error::AssetError;
pub use image::{process_image, ImageResult, Variant, VariantSpec};
```

- [ ] **Step 6: Run**

Run: `cargo test -p lopress-assets`
Expected: 4 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/lopress-assets/
git commit -m "lopress-assets: image variant pipeline with hash-keyed cache"
```

---

## Task 14: lopress-build — site config and workspace layout

**Files:**
- Modify: `crates/lopress-build/Cargo.toml`
- Create: `crates/lopress-build/src/site.rs`
- Create: `crates/lopress-build/src/error.rs`
- Modify: `crates/lopress-build/src/lib.rs`

- [ ] **Step 1: Dependencies**

```toml
[dependencies]
anyhow = { workspace = true }
chrono = { workspace = true }
quick-xml = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
toml = { workspace = true }
walkdir = { workspace = true }
lopress-core = { path = "../lopress-core" }
lopress-plugin = { path = "../lopress-plugin" }
lopress-theme = { path = "../lopress-theme" }
lopress-assets = { path = "../lopress-assets" }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 2: Error**

`crates/lopress-build/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("site config: {0}")]
    Config(String),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("core parse: {0}")]
    Parse(#[from] lopress_core::ParseError),
    #[error("plugin: {0}")]
    Plugin(#[from] lopress_plugin::PluginError),
    #[error("theme: {0}")]
    Theme(#[from] lopress_theme::ThemeError),
    #[error("assets: {0}")]
    Assets(#[from] lopress_assets::AssetError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("xml: {0}")]
    Xml(String),
    #[error("one or more pages failed to build: {0:?}")]
    PartialFailure(Vec<PageFailure>),
}

#[derive(Debug, Clone)]
pub struct PageFailure {
    pub path: std::path::PathBuf,
    pub message: String,
}
```

- [ ] **Step 3: Config**

`crates/lopress-build/src/site.rs`:

```rust
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
```

- [ ] **Step 4: Wire `lib.rs`**

```rust
pub mod error;
pub mod site;

pub use error::BuildError;
pub use site::{SiteConfig, Workspace};
```

- [ ] **Step 5: Run**

Run: `cargo test -p lopress-build`
Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: site config and workspace layout"
```

---

## Task 15: lopress-build — block tree to HTML renderer

Translates `lopress_core::Block` → HTML. Standard blocks render inline; `lopress:*` blocks look up their template in the plugin registry and render via Tera.

**Files:**
- Create: `crates/lopress-build/src/render.rs`
- Modify: `crates/lopress-build/src/lib.rs`

- [ ] **Step 1: Tests**

`crates/lopress-build/src/render.rs`:

```rust
use crate::error::BuildError;
use lopress_core::{Block, Document};
use lopress_plugin::PluginRegistry;
use serde_json::Value;
use std::fmt::Write;
use tera::Tera;

/// Render the body of a Document into HTML. `tera` may be shared with the
/// theme engine but must also know the plugin templates (the builder inserts
/// them at startup).
pub fn render_body(
    doc: &Document,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<String, BuildError> {
    let mut out = String::new();
    for b in &doc.blocks {
        write_block(&mut out, b, registry, tera)?;
    }
    Ok(out)
}

fn write_block(
    out: &mut String,
    b: &Block,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<(), BuildError> {
    match b.r#type.as_str() {
        "paragraph" => {
            let _ = write!(out, "<p>{}</p>\n", escape(b.text.as_deref().unwrap_or("")));
        }
        "heading" => {
            let level = b.attrs.get("level").and_then(|v| v.as_u64()).unwrap_or(1);
            let _ = write!(
                out,
                "<h{0}>{1}</h{0}>\n",
                level,
                escape(b.text.as_deref().unwrap_or(""))
            );
        }
        "quote" => {
            out.push_str("<blockquote>\n");
            for c in &b.children {
                write_block(out, c, registry, tera)?;
            }
            out.push_str("</blockquote>\n");
        }
        "code_block" => {
            let lang = b.attrs.get("lang").and_then(|v| v.as_str()).unwrap_or("");
            let class = if lang.is_empty() { String::new() } else { format!(" class=\"language-{}\"", escape(lang)) };
            let _ = write!(
                out,
                "<pre><code{0}>{1}</code></pre>\n",
                class,
                escape(b.text.as_deref().unwrap_or(""))
            );
        }
        "list" => {
            let ordered = b.attrs.get("ordered").and_then(|v| v.as_bool()).unwrap_or(false);
            let tag = if ordered { "ol" } else { "ul" };
            let _ = write!(out, "<{}>\n", tag);
            for item in &b.children {
                out.push_str("<li>");
                for c in &item.children {
                    write_block(out, c, registry, tera)?;
                }
                out.push_str("</li>\n");
            }
            let _ = write!(out, "</{}>\n", tag);
        }
        "image" => {
            let src = b.attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
            let alt = b.attrs.get("alt").and_then(|v| v.as_str()).unwrap_or("");
            let _ = write!(
                out,
                "<img src=\"{}\" alt=\"{}\">\n",
                escape(src),
                escape(alt)
            );
        }
        custom if custom.starts_with("lopress:") => {
            render_custom(out, b, custom, registry, tera)?;
        }
        other => {
            let _ = write!(out, "<!-- unknown block: {} -->\n", escape(other));
        }
    }
    Ok(())
}

fn render_custom(
    out: &mut String,
    b: &Block,
    full_name: &str,
    registry: &PluginRegistry,
    tera: &Tera,
) -> Result<(), BuildError> {
    let Some((plugin, decl)) = registry.block(full_name) else {
        let _ = write!(out, "<!-- missing plugin for {} -->\n", escape(full_name));
        return Ok(());
    };
    // Render inner children first.
    let mut inner_html = String::new();
    for c in &b.children {
        write_block(&mut inner_html, c, registry, tera)?;
    }
    let template_key = format!("{}::{}", plugin.manifest.name, decl.template);
    let mut ctx = tera::Context::new();
    ctx.insert("attrs", &b.attrs);
    ctx.insert("inner_html", &inner_html);
    let rendered = tera
        .render(&template_key, &ctx)
        .map_err(|e| BuildError::Config(format!("template {}: {}", template_key, e)))?;
    out.push_str(&rendered);
    if !rendered.ends_with('\n') {
        out.push('\n');
    }
    let _ = b;
    Ok(())
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopress_core::FrontMatter;
    use serde_json::json;
    use tera::Tera;

    fn empty_registry() -> PluginRegistry {
        PluginRegistry::default()
    }

    #[test]
    fn renders_paragraph_and_heading() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![
                Block::heading(2, "Hi"),
                Block::paragraph("body"),
            ],
        };
        let tera = Tera::default();
        let html = render_body(&doc, &empty_registry(), &tera).unwrap();
        assert_eq!(html, "<h2>Hi</h2>\n<p>body</p>\n");
    }

    #[test]
    fn unknown_custom_block_emits_comment() {
        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:missing".into(),
                attrs: json!({}),
                children: vec![],
                text: None,
            }],
        };
        let tera = Tera::default();
        let html = render_body(&doc, &empty_registry(), &tera).unwrap();
        assert!(html.contains("missing plugin for lopress:missing"));
    }

    #[test]
    fn known_custom_block_renders_via_template() {
        let mut reg = PluginRegistry::default();
        use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
        reg.insert(LoadedPlugin {
            root: std::path::PathBuf::from("/does/not/exist"),
            manifest: PluginManifest {
                name: "demo".into(),
                version: "0.1.0".into(),
                theme: false,
                blocks: vec![BlockDecl {
                    name: "lopress:demo".into(),
                    template: "blocks/demo.html".into(),
                    attrs: Default::default(),
                    renderer: None,
                    editor: None,
                }],
            },
        })
        .unwrap();

        let mut tera = Tera::default();
        tera.add_raw_template(
            "demo::blocks/demo.html",
            "<figure data-x=\"{{ attrs.x }}\">{{ inner_html | safe }}</figure>",
        )
        .unwrap();

        let doc = Document {
            front_matter: FrontMatter::default(),
            blocks: vec![Block {
                r#type: "lopress:demo".into(),
                attrs: json!({"x":"v"}),
                children: vec![Block::paragraph("inner")],
                text: None,
            }],
        };
        let html = render_body(&doc, &reg, &tera).unwrap();
        assert!(html.contains("data-x=\"v\""));
        assert!(html.contains("<p>inner</p>"));
    }
}
```

- [ ] **Step 2: Wire `lib.rs`** — add `pub mod render;` and `pub use render::render_body;`.

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-build render`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: block tree to HTML renderer"
```

---

## Task 16: lopress-build — page pipeline (posts, pages, index, tags)

Discover markdown sources, render each one's HTML body, feed it through the active theme, write to `www/`. Also build the `posts` list used by the index and tag archives.

**Files:**
- Create: `crates/lopress-build/src/pages.rs`
- Modify: `crates/lopress-build/src/lib.rs`

- [ ] **Step 1: Tests**

`crates/lopress-build/src/pages.rs`:

```rust
use crate::error::{BuildError, PageFailure};
use crate::render::render_body;
use crate::site::Workspace;
use lopress_core::{parse, Document};
use lopress_plugin::PluginRegistry;
use lopress_theme::{PageCtx, PageKind, PostSummary, RenderContext, SiteCtx, ThemeEngine};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct DiscoveredPost {
    pub source_path: PathBuf,
    pub slug: String,
    pub doc: Document,
}

/// Walk `dir` and return all `*.md` files paired with their parsed Document
/// and computed slug. `kind` is only used for error messages.
pub fn discover(
    dir: &Path,
    kind: &str,
) -> Result<(Vec<DiscoveredPost>, Vec<PageFailure>), BuildError> {
    let mut ok = Vec::new();
    let mut failures = Vec::new();
    if !dir.exists() {
        return Ok((ok, failures));
    }
    for entry in WalkDir::new(dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let src = std::fs::read_to_string(entry.path())?;
        let doc = match parse(&src) {
            Ok(d) => d,
            Err(e) => {
                failures.push(PageFailure {
                    path: entry.path().to_path_buf(),
                    message: format!("{} parse: {}", kind, e),
                });
                continue;
            }
        };
        let slug = doc
            .front_matter
            .slug
            .clone()
            .unwrap_or_else(|| {
                entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string()
            });
        ok.push(DiscoveredPost {
            source_path: entry.path().to_path_buf(),
            slug,
            doc,
        });
    }
    Ok((ok, failures))
}

/// Build the list of PostSummary objects used by index/tag templates and feed.
pub fn post_summaries(posts: &[DiscoveredPost], base_url: &str) -> Vec<PostSummary> {
    let mut out: Vec<PostSummary> = posts
        .iter()
        .filter(|p| !p.doc.front_matter.draft)
        .map(|p| {
            let url = format!("/posts/{}/", p.slug);
            let _ = base_url;
            PostSummary {
                title: p
                    .doc
                    .front_matter
                    .title
                    .clone()
                    .unwrap_or_else(|| p.slug.clone()),
                slug: p.slug.clone(),
                url,
                date: p.doc.front_matter.date,
                tags: p.doc.front_matter.tags.clone(),
                description: p.doc.front_matter.description.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.slug.cmp(&b.slug)));
    out
}

/// Render every post/page into www/ via the theme engine. Returns page
/// failures; successful pages have already been written.
pub fn render_all(
    workspace: &Workspace,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
    posts: &[DiscoveredPost],
    pages: &[DiscoveredPost],
) -> Result<Vec<PageFailure>, BuildError> {
    let www = workspace.www_dir();
    std::fs::create_dir_all(&www)?;
    let summaries = post_summaries(posts, &workspace.config.site.base_url);

    let site_ctx = SiteCtx {
        title: workspace.config.site.title.clone(),
        base_url: workspace.config.site.base_url.clone(),
        nav: workspace
            .config
            .site
            .nav
            .items
            .iter()
            .map(|n| lopress_theme::NavItem {
                label: n.label.clone(),
                href: n.href.clone(),
            })
            .collect(),
        posts: summaries.clone(),
    };

    let mut failures = Vec::new();

    // Posts
    for p in posts {
        if p.doc.front_matter.draft {
            continue;
        }
        match render_one_post(&www, &site_ctx, p, registry, theme, tera_shared) {
            Ok(()) => {}
            Err(e) => failures.push(PageFailure {
                path: p.source_path.clone(),
                message: e.to_string(),
            }),
        }
    }

    // Pages
    for p in pages {
        match render_one_page(&www, &site_ctx, p, registry, theme, tera_shared) {
            Ok(()) => {}
            Err(e) => failures.push(PageFailure {
                path: p.source_path.clone(),
                message: e.to_string(),
            }),
        }
    }

    // Index (post list)
    match render_index(&www, &site_ctx, theme) {
        Ok(()) => {}
        Err(e) => failures.push(PageFailure {
            path: www.join("index.html"),
            message: e.to_string(),
        }),
    }

    // Tag archives
    let mut by_tag: BTreeMap<String, Vec<PostSummary>> = BTreeMap::new();
    for s in &summaries {
        for t in &s.tags {
            by_tag.entry(t.clone()).or_default().push(s.clone());
        }
    }
    for (tag, posts) in by_tag {
        match render_tag(&www, &site_ctx, &tag, &posts, theme) {
            Ok(()) => {}
            Err(e) => failures.push(PageFailure {
                path: www.join(format!("tags/{}/index.html", tag)),
                message: e.to_string(),
            }),
        }
    }

    Ok(failures)
}

fn render_one_post(
    www: &Path,
    site: &SiteCtx,
    post: &DiscoveredPost,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
) -> Result<(), BuildError> {
    let body_html = render_body(&post.doc, registry, tera_shared)?;
    let url = format!("/posts/{}/", post.slug);
    let canonical = join_url(&site.base_url, &url);
    let page = PageCtx {
        kind: PageKind::Post,
        title: post.doc.front_matter.title.clone().unwrap_or_else(|| post.slug.clone()),
        slug: post.slug.clone(),
        url: url.clone(),
        canonical,
        description: post.doc.front_matter.description.clone(),
        og_image: post.doc.front_matter.image.clone(),
        date: post.doc.front_matter.date,
        tags: post.doc.front_matter.tags.clone(),
        body_html,
        posts: vec![],
        tag: None,
    };
    let html = theme.render(
        "post.html",
        &RenderContext {
            site,
            page: &page,
        },
    )?;
    write_page(www, &format!("posts/{}", post.slug), &html)
}

fn render_one_page(
    www: &Path,
    site: &SiteCtx,
    p: &DiscoveredPost,
    registry: &PluginRegistry,
    theme: &ThemeEngine,
    tera_shared: &tera::Tera,
) -> Result<(), BuildError> {
    let body_html = render_body(&p.doc, registry, tera_shared)?;
    let url = format!("/{}/", p.slug);
    let canonical = join_url(&site.base_url, &url);
    let page = PageCtx {
        kind: PageKind::Page,
        title: p.doc.front_matter.title.clone().unwrap_or_else(|| p.slug.clone()),
        slug: p.slug.clone(),
        url: url.clone(),
        canonical,
        description: p.doc.front_matter.description.clone(),
        og_image: p.doc.front_matter.image.clone(),
        date: p.doc.front_matter.date,
        tags: p.doc.front_matter.tags.clone(),
        body_html,
        posts: vec![],
        tag: None,
    };
    let html = theme.render(
        "page.html",
        &RenderContext {
            site,
            page: &page,
        },
    )?;
    write_page(www, &p.slug, &html)
}

fn render_index(
    www: &Path,
    site: &SiteCtx,
    theme: &ThemeEngine,
) -> Result<(), BuildError> {
    let page = PageCtx {
        kind: PageKind::Index,
        title: site.title.clone(),
        slug: String::new(),
        url: "/".into(),
        canonical: join_url(&site.base_url, "/"),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: site.posts.clone(),
        tag: None,
    };
    let html = theme.render(
        "index.html",
        &RenderContext {
            site,
            page: &page,
        },
    )?;
    std::fs::write(www.join("index.html"), html)?;
    Ok(())
}

fn render_tag(
    www: &Path,
    site: &SiteCtx,
    tag: &str,
    posts: &[PostSummary],
    theme: &ThemeEngine,
) -> Result<(), BuildError> {
    let url = format!("/tags/{}/", tag);
    let page = PageCtx {
        kind: PageKind::Tag,
        title: format!("Tagged: {}", tag),
        slug: tag.to_string(),
        url: url.clone(),
        canonical: join_url(&site.base_url, &url),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: posts.to_vec(),
        tag: Some(tag.to_string()),
    };
    let html = theme.render(
        "tag.html",
        &RenderContext {
            site,
            page: &page,
        },
    )?;
    write_page(www, &format!("tags/{}", tag), &html)
}

fn write_page(www: &Path, rel_dir: &str, html: &str) -> Result<(), BuildError> {
    let dir = www.join(rel_dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("index.html"), html)?;
    Ok(())
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{}{}", base, path)
}
```

- [ ] **Step 2: Wire `lib.rs`** — add `pub mod pages;` and `pub use pages::{render_all, discover, post_summaries};`.

- [ ] **Step 3: Smoke-test at crate level**

Run: `cargo build -p lopress-build`
Expected: compiles with zero warnings. (We cover behavior via the Task 20 integration fixtures.)

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: posts, pages, index, tag rendering"
```

---

## Task 17: lopress-build — feed.xml, sitemap.xml, robots.txt, 404.html

**Files:**
- Create: `crates/lopress-build/src/feed.rs`
- Create: `crates/lopress-build/src/sitemap.rs`
- Create: `crates/lopress-build/src/robots.rs`
- Create: `crates/lopress-build/src/not_found.rs`
- Modify: `crates/lopress-build/src/lib.rs`

- [ ] **Step 1: Feed generator**

`crates/lopress-build/src/feed.rs`:

```rust
use crate::error::BuildError;
use lopress_theme::{PostSummary, SiteCtx};
use std::fmt::Write;
use std::path::Path;

/// Write `www/feed.xml` — Atom feed of non-draft posts in reverse-chron order.
pub fn write(www: &Path, site: &SiteCtx) -> Result<(), BuildError> {
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    buf.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    let _ = writeln!(buf, "  <title>{}</title>", escape(&site.title));
    let _ = writeln!(
        buf,
        "  <link href=\"{}/feed.xml\" rel=\"self\"/>",
        site.base_url.trim_end_matches('/')
    );
    let _ = writeln!(
        buf,
        "  <link href=\"{}/\" />",
        site.base_url.trim_end_matches('/')
    );
    let _ = writeln!(buf, "  <id>{}/</id>", site.base_url.trim_end_matches('/'));
    if let Some(latest) = site.posts.first().and_then(|p| p.date) {
        let _ = writeln!(buf, "  <updated>{}T00:00:00Z</updated>", latest);
    }
    for p in &site.posts {
        entry(&mut buf, p, &site.base_url);
    }
    buf.push_str("</feed>\n");
    std::fs::write(www.join("feed.xml"), buf)?;
    Ok(())
}

fn entry(buf: &mut String, p: &PostSummary, base_url: &str) {
    let url = format!("{}{}", base_url.trim_end_matches('/'), p.url);
    buf.push_str("  <entry>\n");
    let _ = writeln!(buf, "    <title>{}</title>", escape(&p.title));
    let _ = writeln!(buf, "    <link href=\"{}\"/>", url);
    let _ = writeln!(buf, "    <id>{}</id>", url);
    if let Some(d) = p.date {
        let _ = writeln!(buf, "    <updated>{}T00:00:00Z</updated>", d);
    }
    if let Some(desc) = &p.description {
        let _ = writeln!(buf, "    <summary>{}</summary>", escape(desc));
    }
    buf.push_str("  </entry>\n");
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use lopress_theme::{PostSummary, SiteCtx};
    use tempfile::TempDir;

    #[test]
    fn writes_atom_feed_with_entries() {
        let d = TempDir::new().unwrap();
        let site = SiteCtx {
            title: "S".into(),
            base_url: "https://example.com".into(),
            nav: vec![],
            posts: vec![PostSummary {
                title: "Hi".into(),
                slug: "hi".into(),
                url: "/posts/hi/".into(),
                date: Some(NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()),
                tags: vec![],
                description: Some("d".into()),
            }],
        };
        write(d.path(), &site).unwrap();
        let s = std::fs::read_to_string(d.path().join("feed.xml")).unwrap();
        assert!(s.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(s.contains("https://example.com/posts/hi/"));
        assert!(s.contains("<title>Hi</title>"));
    }
}
```

- [ ] **Step 2: Sitemap**

`crates/lopress-build/src/sitemap.rs`:

```rust
use crate::error::BuildError;
use lopress_theme::SiteCtx;
use std::fmt::Write;
use std::path::Path;

pub fn write(
    www: &Path,
    site: &SiteCtx,
    page_urls: &[String],
    tag_urls: &[String],
) -> Result<(), BuildError> {
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    buf.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    let base = site.base_url.trim_end_matches('/');

    let _ = writeln!(buf, "  <url><loc>{}/</loc></url>", base);
    for p in &site.posts {
        let _ = writeln!(buf, "  <url><loc>{}{}</loc></url>", base, p.url);
    }
    for u in page_urls {
        let _ = writeln!(buf, "  <url><loc>{}{}</loc></url>", base, u);
    }
    for u in tag_urls {
        let _ = writeln!(buf, "  <url><loc>{}{}</loc></url>", base, u);
    }
    buf.push_str("</urlset>\n");
    std::fs::write(www.join("sitemap.xml"), buf)?;
    Ok(())
}
```

- [ ] **Step 3: robots + 404**

`crates/lopress-build/src/robots.rs`:

```rust
use crate::error::BuildError;
use crate::site::SiteConfig;
use std::path::Path;

pub fn write(www: &Path, config: &SiteConfig) -> Result<(), BuildError> {
    let body = config
        .robots
        .as_ref()
        .map(|r| r.body.clone())
        .unwrap_or_else(default_body);
    std::fs::write(www.join("robots.txt"), body)?;
    Ok(())
}

fn default_body() -> String {
    "User-agent: *\nAllow: /\n".into()
}
```

`crates/lopress-build/src/not_found.rs`:

```rust
use crate::error::BuildError;
use lopress_theme::{PageCtx, PageKind, RenderContext, SiteCtx, ThemeEngine};
use std::path::Path;

pub fn write(www: &Path, site: &SiteCtx, theme: &ThemeEngine) -> Result<(), BuildError> {
    let page = PageCtx {
        kind: PageKind::NotFound,
        title: "Not found".into(),
        slug: "404".into(),
        url: "/404.html".into(),
        canonical: format!(
            "{}/404.html",
            site.base_url.trim_end_matches('/')
        ),
        description: None,
        og_image: None,
        date: None,
        tags: vec![],
        body_html: String::new(),
        posts: vec![],
        tag: None,
    };
    let html = theme.render(
        "404.html",
        &RenderContext {
            site,
            page: &page,
        },
    )?;
    std::fs::write(www.join("404.html"), html)?;
    Ok(())
}
```

- [ ] **Step 4: Wire `lib.rs`**

Add: `pub mod feed; pub mod sitemap; pub mod robots; pub mod not_found;`

- [ ] **Step 5: Run crate tests**

Run: `cargo test -p lopress-build`
Expected: prior tests plus 1 feed test, all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: feed, sitemap, robots, 404 generators"
```

---

## Task 18: lopress-build — top-level `build(workspace)` orchestrator

Brings everything together: load config, load plugins, resolve theme, install plugin templates into Tera, discover + render pages, write feed/sitemap/robots/404, save cache. Also: copy theme CSS, plugin assets, and image variants.

**Files:**
- Create: `crates/lopress-build/src/build.rs`
- Modify: `crates/lopress-build/src/lib.rs`

- [ ] **Step 1: Implementation**

`crates/lopress-build/src/build.rs`:

```rust
use crate::error::{BuildError, PageFailure};
use crate::not_found;
use crate::pages;
use crate::robots;
use crate::sitemap;
use crate::feed;
use crate::render;
use crate::site::Workspace;
use lopress_assets::{process_image, VariantCache, VariantSpec};
use lopress_plugin::load_dir;
use lopress_theme::{resolve, SiteCtx};
use std::path::Path;
use tera::Tera;

pub struct BuildReport {
    pub pages_written: usize,
    pub failures: Vec<PageFailure>,
}

pub fn build(workspace: &Path) -> Result<BuildReport, BuildError> {
    let ws = Workspace::load(workspace)?;
    std::fs::create_dir_all(ws.www_dir())?;

    // Plugins
    let registry = load_dir(
        &ws.plugins_dir(),
        if ws.config.plugins.enabled.is_empty() {
            None
        } else {
            Some(ws.config.plugins.enabled.as_slice())
        },
    )?;

    // Theme
    let theme = resolve(&registry, &ws.config.site.theme)?;

    // Build a shared Tera that knows every theme template and every plugin
    // block template, namespaced by plugin name.
    let mut tera = Tera::default();
    // Theme templates
    for (name, content) in theme_templates(&ws, &theme)? {
        tera.add_raw_template(&name, &content)
            .map_err(|e| BuildError::Config(format!("theme template `{}`: {}", name, e)))?;
    }
    // Plugin block templates, keyed "<plugin>::<template-path>".
    for plugin in &registry.plugins {
        for block in &plugin.manifest.blocks {
            let key = format!("{}::{}", plugin.manifest.name, block.template);
            let src = std::fs::read_to_string(plugin.root.join(&block.template))?;
            tera.add_raw_template(&key, &src).map_err(|e| {
                BuildError::Config(format!("plugin template `{}`: {}", key, e))
            })?;
        }
    }

    // Discover content
    let (posts, mut failures) = pages::discover(&ws.posts_dir(), "post")?;
    let (pages_src, page_failures) = pages::discover(&ws.pages_dir(), "page")?;
    failures.extend(page_failures);

    // Summaries for site ctx
    let summaries = pages::post_summaries(&posts, &ws.config.site.base_url);

    // Build SiteCtx now so feed/sitemap/404 see the same content
    let site_ctx = SiteCtx {
        title: ws.config.site.title.clone(),
        base_url: ws.config.site.base_url.clone(),
        nav: ws
            .config
            .site
            .nav
            .items
            .iter()
            .map(|n| lopress_theme::NavItem {
                label: n.label.clone(),
                href: n.href.clone(),
            })
            .collect(),
        posts: summaries.clone(),
    };

    // Render posts/pages/index/tags
    let render_failures =
        pages::render_all(&ws, &registry, &theme.engine, &tera, &posts, &pages_src)?;
    failures.extend(render_failures);

    // Feed, sitemap, robots, 404
    feed::write(&ws.www_dir(), &site_ctx)?;
    let page_urls: Vec<String> = pages_src.iter().map(|p| format!("/{}/", p.slug)).collect();
    let tag_urls: Vec<String> = tag_urls_for(&summaries);
    sitemap::write(&ws.www_dir(), &site_ctx, &page_urls, &tag_urls)?;
    robots::write(&ws.www_dir(), &ws.config)?;
    not_found::write(&ws.www_dir(), &site_ctx, &theme.engine)?;

    // Theme CSS
    write_theme_css(&ws, &theme)?;

    // Plugin assets (bulk copy)
    for plugin in &registry.plugins {
        let assets = plugin.root.join("assets");
        if assets.exists() {
            let target = ws.www_dir().join("assets").join(&plugin.manifest.name);
            copy_dir(&assets, &target)?;
        }
    }

    // Image pipeline
    let mut img_cache = VariantCache::load(&ws.www_dir().join(".lopress-image-cache.json"))?;
    let spec = VariantSpec {
        widths: ws.config.build.image_variants.clone(),
        ..VariantSpec::default()
    };
    let src_images = ws.images_dir();
    let www_images = ws.www_dir().join("images");
    if src_images.exists() {
        for entry in walkdir::WalkDir::new(&src_images).min_depth(1) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            match process_image(entry.path(), &www_images, &mut img_cache, &spec) {
                Ok(_) => {}
                Err(e) => failures.push(PageFailure {
                    path: entry.path().to_path_buf(),
                    message: format!("image: {}", e),
                }),
            }
        }
    }
    img_cache.save(&ws.www_dir().join(".lopress-image-cache.json"))?;

    // Count pages written from walking www/; simpler: posts + pages + tags + index.
    let pages_written = posts.iter().filter(|p| !p.doc.front_matter.draft).count()
        + pages_src.len()
        + tag_urls.len()
        + 1;

    Ok(BuildReport {
        pages_written,
        failures,
    })
}

fn theme_templates(
    ws: &Workspace,
    theme: &lopress_theme::ResolvedTheme,
) -> Result<Vec<(String, String)>, BuildError> {
    // The ResolvedTheme's engine already holds parsed templates, but we need
    // them keyed unnamespaced so `post.html` / `layout.html` / etc. resolve.
    // The builtin and on-disk resolvers store them under their filename; rehydrate
    // here by re-reading from the theme's source. For the built-in, we replicate
    // via the known file set.
    let mut out = Vec::new();
    if let Some(css_path) = &theme.css_path {
        // Theme is from disk; read `<theme_root>/templates/*.html`.
        let templates_dir = css_path.parent().unwrap().join("templates");
        for entry in std::fs::read_dir(templates_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("html") {
                let name = entry.path().file_name().unwrap().to_string_lossy().into();
                let contents = std::fs::read_to_string(entry.path())?;
                out.push((name, contents));
            }
        }
    } else {
        // Built-in theme — pull via the same helper the engine uses.
        for name in [
            "layout.html",
            "post.html",
            "page.html",
            "index.html",
            "tag.html",
            "404.html",
        ] {
            if let Some(src) = lopress_theme::builtin_template(name) {
                out.push((name.into(), src.into()));
            }
        }
    }
    let _ = ws;
    Ok(out)
}

fn write_theme_css(
    ws: &Workspace,
    theme: &lopress_theme::ResolvedTheme,
) -> Result<(), BuildError> {
    let target = ws.www_dir().join("assets").join("theme.css");
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::write(target, &theme.css_content)?;
    Ok(())
}

fn tag_urls_for(posts: &[lopress_theme::PostSummary]) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut tags = BTreeSet::new();
    for p in posts {
        for t in &p.tags {
            tags.insert(t.clone());
        }
    }
    tags.into_iter().map(|t| format!("/tags/{}/", t)).collect()
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), BuildError> {
    std::fs::create_dir_all(to)?;
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(from).unwrap();
        let dst = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dst)?;
        } else {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dst)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Add a helper to lopress-theme for looking up built-in templates by name**

In `crates/lopress-theme/src/builtin.rs`, add:

```rust
/// Return the source of a built-in template by filename, or None.
pub fn builtin_template(name: &str) -> Option<&'static str> {
    DEFAULT
        .get_file(&format!("templates/{}", name))?
        .contents_utf8()
}
```

And re-export from `lib.rs`:

```rust
pub use builtin::{builtin_template, default_css, default_engine};
```

- [ ] **Step 3: Wire `lopress-build/src/lib.rs`**

```rust
pub mod build;
pub mod error;
pub mod feed;
pub mod not_found;
pub mod pages;
pub mod render;
pub mod robots;
pub mod site;
pub mod sitemap;

pub use build::{build, BuildReport};
pub use error::{BuildError, PageFailure};
pub use site::{SiteConfig, Workspace};
```

- [ ] **Step 4: Build**

Run: `cargo build -p lopress-build`
Expected: compiles with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-build/ crates/lopress-theme/
git commit -m "lopress-build: top-level build orchestrator"
```

---

## Task 19: lopress-build — integration test with a minimal fixture

Concrete end-to-end test: a workspace on disk, call `build()`, assert the output files.

**Files:**
- Create: `crates/lopress-build/tests/build_integration.rs`
- Create: `crates/lopress-build/tests/fixtures/minimal/lopress.toml`
- Create: `crates/lopress-build/tests/fixtures/minimal/src/posts/hello.md`
- Create: `crates/lopress-build/tests/fixtures/minimal/src/pages/about.md`

- [ ] **Step 1: Fixture files**

`crates/lopress-build/tests/fixtures/minimal/lopress.toml`:

```toml
[site]
title = "Test Site"
base_url = "https://example.com"

[site.nav]
items = [
  { label = "Home", href = "/" },
  { label = "About", href = "/about/" },
]
```

`crates/lopress-build/tests/fixtures/minimal/src/posts/hello.md`:

```markdown
---
title: Hello
date: 2026-04-18
tags: [intro]
---

# Hello

First post.
```

`crates/lopress-build/tests/fixtures/minimal/src/pages/about.md`:

```markdown
---
title: About
---

# About

About page.
```

- [ ] **Step 2: Test**

`crates/lopress-build/tests/build_integration.rs`:

```rust
use lopress_build::build;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn copy_fixture(name: &str) -> (TempDir, PathBuf) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let dst = TempDir::new().unwrap();
    copy_dir(&src, dst.path());
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let rel = entry.path().strip_prefix(from).unwrap();
        let dst = to.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dst).unwrap();
        } else {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(entry.path(), &dst).unwrap();
        }
    }
}

#[test]
fn minimal_site_builds_expected_files() {
    let (_tmp, root) = copy_fixture("minimal");
    let report = build(&root).unwrap();
    assert!(report.failures.is_empty(), "failures: {:?}", report.failures);

    let www = root.join("www");
    assert!(www.join("index.html").exists());
    assert!(www.join("posts/hello/index.html").exists());
    assert!(www.join("about/index.html").exists());
    assert!(www.join("tags/intro/index.html").exists());
    assert!(www.join("feed.xml").exists());
    assert!(www.join("sitemap.xml").exists());
    assert!(www.join("robots.txt").exists());
    assert!(www.join("404.html").exists());
    assert!(www.join("assets/theme.css").exists());

    let index = fs::read_to_string(www.join("index.html")).unwrap();
    assert!(index.contains("Test Site"));
    assert!(index.contains("/posts/hello/"));

    let hello = fs::read_to_string(www.join("posts/hello/index.html")).unwrap();
    assert!(hello.contains("<h1>Hello</h1>"));
    assert!(hello.contains("<p>First post.</p>"));

    let feed = fs::read_to_string(www.join("feed.xml")).unwrap();
    assert!(feed.contains("<title>Hello</title>"));
    assert!(feed.contains("https://example.com/posts/hello/"));
}
```

Also add `walkdir = { workspace = true }` to `[dev-dependencies]` of `lopress-build/Cargo.toml` if not already present.

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-build --test build_integration`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: integration test for minimal site"
```

---

## Task 20: lopress-build — integration test with a draft post

- [ ] **Step 1: Fixture**

`crates/lopress-build/tests/fixtures/with-draft/lopress.toml`:

```toml
[site]
title = "Draft Test"
base_url = "https://example.com"
```

`crates/lopress-build/tests/fixtures/with-draft/src/posts/done.md`:

```markdown
---
title: Done
date: 2026-04-18
---
Published.
```

`crates/lopress-build/tests/fixtures/with-draft/src/posts/wip.md`:

```markdown
---
title: WIP
date: 2026-04-18
draft: true
---
Not ready.
```

- [ ] **Step 2: Test**

Append to `crates/lopress-build/tests/build_integration.rs`:

```rust
#[test]
fn drafts_are_excluded_from_every_output() {
    let (_tmp, root) = copy_fixture("with-draft");
    let report = build(&root).unwrap();
    assert!(report.failures.is_empty());

    let www = root.join("www");
    assert!(www.join("posts/done/index.html").exists());
    assert!(
        !www.join("posts/wip/index.html").exists(),
        "draft post was written"
    );

    let feed = fs::read_to_string(www.join("feed.xml")).unwrap();
    assert!(!feed.contains("WIP"), "draft appears in feed");

    let sitemap = fs::read_to_string(www.join("sitemap.xml")).unwrap();
    assert!(!sitemap.contains("wip"), "draft appears in sitemap");

    let index = fs::read_to_string(www.join("index.html")).unwrap();
    assert!(!index.contains("WIP"), "draft appears in index");
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-build --test build_integration drafts`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: integration test for draft exclusion"
```

---

## Task 21: lopress-build — integration test with a custom plugin

Proves the plugin pipeline end-to-end: manifest loads, template renders, inner content is preserved, plugin asset is copied.

**Files:**
- Create: `crates/lopress-build/tests/fixtures/with-plugin/lopress.toml`
- Create: `crates/lopress-build/tests/fixtures/with-plugin/src/posts/demo.md`
- Create: `crates/lopress-build/tests/fixtures/with-plugin/plugins/callout/plugin.toml`
- Create: `crates/lopress-build/tests/fixtures/with-plugin/plugins/callout/blocks/callout.html`
- Create: `crates/lopress-build/tests/fixtures/with-plugin/plugins/callout/assets/callout.css`

- [ ] **Step 1: Fixture**

`lopress.toml`:

```toml
[site]
title = "Plugin Test"
base_url = "https://example.com"

[plugins]
enabled = ["callout"]
```

`plugins/callout/plugin.toml`:

```toml
name = "callout"
version = "0.1.0"

[[blocks]]
name = "lopress:callout"
template = "blocks/callout.html"

[blocks.attrs]
kind = { type = "string", required = false, ui = "select", options = ["info","warning"] }
```

`plugins/callout/blocks/callout.html`:

```html
<aside class="callout callout-{{ attrs.kind | default(value="info") }}">
  {{ inner_html | safe }}
</aside>
```

`plugins/callout/assets/callout.css`:

```css
.callout { padding: 1rem; border-left: 4px solid #888; }
.callout-warning { border-color: #c97a00; }
```

`src/posts/demo.md`:

```markdown
---
title: Demo
date: 2026-04-18
---

Before.

<!-- lopress:callout {"kind":"warning"} -->
Inside **callout**.
<!-- /lopress:callout -->

After.
```

- [ ] **Step 2: Test**

Append to `build_integration.rs`:

```rust
#[test]
fn plugin_block_renders_with_inner_content_and_asset_is_copied() {
    let (_tmp, root) = copy_fixture("with-plugin");
    let report = build(&root).unwrap();
    assert!(report.failures.is_empty(), "failures: {:?}", report.failures);

    let www = root.join("www");
    let html = fs::read_to_string(www.join("posts/demo/index.html")).unwrap();
    assert!(html.contains("class=\"callout callout-warning\""));
    assert!(html.contains("Inside"));
    assert!(html.contains("<p>Before."));
    assert!(html.contains("<p>After."));

    assert!(www.join("assets/callout/callout.css").exists());
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-build --test build_integration plugin`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: integration test for custom block plugin"
```

---

## Task 22: lopress-build — integration test with images

**Files:**
- Create: `crates/lopress-build/tests/fixtures/with-images/lopress.toml`
- Create: `crates/lopress-build/tests/fixtures/with-images/src/images/photo.jpg` (generated by test if missing)
- Create: `crates/lopress-build/tests/fixtures/with-images/src/posts/album.md`

- [ ] **Step 1: Fixture**

`lopress.toml`:

```toml
[site]
title = "Images Test"
base_url = "https://example.com"
```

`src/posts/album.md`:

```markdown
---
title: Album
date: 2026-04-18
---

![photo](../images/photo.jpg)
```

Leave `src/images/` empty on disk — the test creates the file because storing binary images in the plan is noisy.

- [ ] **Step 2: Test**

Append to `build_integration.rs`:

```rust
#[test]
fn image_pipeline_produces_variants_and_caches_on_rerun() {
    use image::{Rgb, RgbImage};

    let (_tmp, root) = copy_fixture("with-images");
    let images = root.join("src/images");
    fs::create_dir_all(&images).unwrap();
    let src_img = images.join("photo.jpg");
    let mut img = RgbImage::new(2000, 1500);
    for p in img.pixels_mut() { *p = Rgb([120, 180, 255]); }
    img.save(&src_img).unwrap();

    let report = build(&root).unwrap();
    assert!(report.failures.is_empty(), "failures: {:?}", report.failures);

    let www_images = root.join("www/images");
    assert!(www_images.join("photo.jpg").exists());
    assert!(www_images.join("photo.400w.webp").exists());
    assert!(www_images.join("photo.800w.webp").exists());
    assert!(www_images.join("photo.1600w.webp").exists());

    // Second build should not regenerate files; record mtimes of webp files,
    // rebuild, compare.
    let mtime_before = fs::metadata(www_images.join("photo.800w.webp"))
        .unwrap()
        .modified()
        .unwrap();
    build(&root).unwrap();
    let mtime_after = fs::metadata(www_images.join("photo.800w.webp"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime_before, mtime_after, "cached variant was regenerated");
}
```

Ensure `image` is a dev-dependency in `crates/lopress-build/Cargo.toml`:

```toml
[dev-dependencies]
# ... existing
image = { workspace = true }
```

- [ ] **Step 3: Run**

Run: `cargo test -p lopress-build --test build_integration image`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/
git commit -m "lopress-build: integration test for image pipeline and cache"
```

---

## Task 23: CLI binary

Wire everything into a usable `lopress` binary with `build` and `new` subcommands.

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `src/main.rs`**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lopress", version, about = "A personal blog authoring tool with static site generation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a workspace into `www/`.
    Build {
        /// Workspace directory (contains `lopress.toml`).
        workspace: PathBuf,
    },
    /// Scaffold a new workspace.
    New {
        /// Destination directory. Must not exist, or must be empty.
        dir: PathBuf,
        #[arg(long, default_value = "Untitled")]
        title: String,
        #[arg(long, default_value = "https://example.com")]
        base_url: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { workspace } => {
            let report = lopress_build::build(&workspace)?;
            println!(
                "built {} page(s); {} failure(s)",
                report.pages_written,
                report.failures.len()
            );
            for f in &report.failures {
                eprintln!("  FAIL {}: {}", f.path.display(), f.message);
            }
            if !report.failures.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::New { dir, title, base_url } => {
            scaffold::new_site(&dir, &title, &base_url)
        }
    }
}

mod scaffold {
    use anyhow::{bail, Result};
    use std::path::Path;

    pub fn new_site(dir: &Path, title: &str, base_url: &str) -> Result<()> {
        if dir.exists() {
            let non_empty = std::fs::read_dir(dir)?.next().is_some();
            if non_empty {
                bail!("target directory `{}` is not empty", dir.display());
            }
        } else {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            dir.join("lopress.toml"),
            format!(
                r#"[site]
title = "{}"
base_url = "{}"

[site.nav]
items = [
  {{ label = "Home", href = "/" }},
  {{ label = "About", href = "/about/" }},
]
"#,
                title, base_url
            ),
        )?;
        for d in ["src/posts", "src/pages", "src/images", "plugins"] {
            std::fs::create_dir_all(dir.join(d))?;
        }
        std::fs::write(
            dir.join("src/posts/hello.md"),
            r#"---
title: Hello
date: 2026-04-18
tags: [intro]
---

# Hello

Welcome to your new lopress site.
"#,
        )?;
        std::fs::write(
            dir.join("src/pages/about.md"),
            r#"---
title: About
---

# About

This is the about page.
"#,
        )?;
        std::fs::write(
            dir.join(".gitignore"),
            "/www\n/.lopress-cache.json\n",
        )?;
        println!("created workspace at {}", dir.display());
        Ok(())
    }
}
```

- [ ] **Step 2: Smoke-test the binary**

Run: `cargo run --bin lopress -- new /tmp/lopress-smoke`
Expected: prints "created workspace...".

Run: `cargo run --bin lopress -- build /tmp/lopress-smoke`
Expected: prints "built N page(s); 0 failure(s)".

Verify: `ls /tmp/lopress-smoke/www`
Expected: `index.html`, `posts/`, `about/`, `tags/`, `feed.xml`, `sitemap.xml`, `robots.txt`, `404.html`, `assets/`.

Cleanup: `rm -rf /tmp/lopress-smoke`.

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace`
Expected: every test passes; zero warnings.

- [ ] **Step 4: Run lint and format**

Run: `cargo fmt --check`
Expected: no output.

Run: `cargo clippy --workspace -- -D warnings`
Expected: no output.

If clippy flags issues, fix them inline before committing.

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "lopress: CLI binary with build and new subcommands"
```

---

## Task 24: CI — GitHub Actions workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 2: Commit**

```bash
git add .github/
git commit -m "ci: cargo fmt, clippy, test on Linux, macOS, Windows"
```

---

## Task 25: Update README with working usage

The README currently says "pre-implementation" — update it to reflect the CLI now works (GUI still planned).

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Edit README.md**

Replace the "Status: pre-implementation" block with:

```markdown
**Status: CLI works; GUI in progress.** The CLI static site generator (`lopress build`, `lopress new`) is implemented. The egui-based block editor and webview preview are planned for a later phase. See [`docs/superpowers/specs/2026-04-18-lopress-design.md`](docs/superpowers/specs/2026-04-18-lopress-design.md) for the full design and [`docs/superpowers/plans/`](docs/superpowers/plans/) for implementation plans.
```

Replace the "Building (once code exists)" section with:

```markdown
## Usage

Create a new workspace and build it:

```
cargo build --release
./target/release/lopress new my-site --title "My Blog" --base-url "https://myblog.example.com"
./target/release/lopress build my-site
# Open my-site/www/index.html in a browser.
```

The `www/` output is a complete static site — copy it to any static host.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README — phase 1 CLI is usable"
```

---

## Verification checklist

At the end of phase 1, all of these should be true:

- [ ] `cargo test --workspace` passes with no warnings.
- [ ] `cargo clippy --workspace -- -D warnings` passes with no output.
- [ ] `cargo fmt --check` passes with no output.
- [ ] `lopress new <tmpdir>` creates a workspace that then builds cleanly.
- [ ] `lopress build <workspace>` produces all expected files (index, posts, pages, tags, feed, sitemap, robots, 404, theme.css).
- [ ] Drafts are excluded from every output (index, feed, sitemap, tag archives, www/).
- [ ] A plugin with a custom block type renders via its template and its CSS is copied to www/.
- [ ] Image variants are cached; a second build does not re-encode unchanged images.
- [ ] CI runs on Linux, macOS, Windows.

When all boxes are ticked, Phase 1 is complete. Phase 2 (fs-watcher + serve command) and Phase 3 (GUI) are separate plans.

use crate::inject::inject_reload_script;
use crate::mime;
use std::path::{Path, PathBuf};

pub enum Resolved {
    File {
        content_type: &'static str,
        body: Vec<u8>,
    },
    Redirect {
        location: String,
    },
    NotFound {
        body: Vec<u8>,
    },
    Forbidden,
}

pub fn resolve(www: &Path, req_path: &str) -> std::io::Result<Resolved> {
    // Drop query string and fragment.
    let path = req_path.split('?').next().unwrap_or("");
    let path = path.split('#').next().unwrap_or("");

    // Percent-decoding: we keep it minimal. Reject anything containing `..`
    // in any form before join.
    if path.contains("..") {
        return Ok(Resolved::Forbidden);
    }

    let rel = path.trim_start_matches('/');
    let candidate: PathBuf = if rel.is_empty() {
        www.join("index.html")
    } else if path.ends_with('/') {
        www.join(rel).join("index.html")
    } else {
        www.join(rel)
    };

    // Ensure the canonical path stays under www/.
    let abs_www = www.canonicalize().unwrap_or_else(|_| www.to_path_buf());
    if let Ok(abs) = candidate.canonicalize() {
        if !abs.starts_with(&abs_www) {
            return Ok(Resolved::Forbidden);
        }
    }

    if candidate.is_file() {
        let bytes = std::fs::read(&candidate)?;
        let ct = mime::guess(&candidate);
        let body = if ct.starts_with("text/html") {
            inject_reload_script(&bytes)
        } else {
            bytes
        };
        return Ok(Resolved::File {
            content_type: ct,
            body,
        });
    }

    // If /foo lacks a trailing slash but /foo/index.html exists, redirect.
    if !path.ends_with('/') && !rel.is_empty() {
        let with_index = www.join(rel).join("index.html");
        if with_index.is_file() {
            return Ok(Resolved::Redirect {
                location: format!("{path}/"),
            });
        }
    }

    // 404 with the site's 404.html if present.
    let custom = www.join("404.html");
    let body = if custom.is_file() {
        let bytes = std::fs::read(&custom)?;
        inject_reload_script(&bytes)
    } else {
        b"404 Not Found".to_vec()
    };
    Ok(Resolved::NotFound { body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let d = TempDir::new().unwrap();
        let www = d.path();
        std::fs::write(www.join("index.html"), "<body>home</body>").unwrap();
        std::fs::create_dir_all(www.join("posts/hello")).unwrap();
        std::fs::write(www.join("posts/hello/index.html"), "<body>hi</body>").unwrap();
        std::fs::write(www.join("style.css"), "x{}").unwrap();
        std::fs::write(www.join("404.html"), "<body>missing</body>").unwrap();
        d
    }

    #[test]
    fn root_serves_index() {
        let d = setup();
        match resolve(d.path(), "/").unwrap() {
            Resolved::File { body, content_type } => {
                assert!(content_type.starts_with("text/html"));
                let s = String::from_utf8(body).unwrap();
                assert!(s.contains("home"));
                assert!(s.contains("EventSource"));
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn directory_path_with_slash_serves_index() {
        let d = setup();
        match resolve(d.path(), "/posts/hello/").unwrap() {
            Resolved::File { body, .. } => {
                assert!(String::from_utf8(body).unwrap().contains("hi"));
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn directory_path_without_slash_redirects() {
        let d = setup();
        match resolve(d.path(), "/posts/hello").unwrap() {
            Resolved::Redirect { location } => assert_eq!(location, "/posts/hello/"),
            _ => panic!("expected redirect"),
        }
    }

    #[test]
    fn css_not_injected() {
        let d = setup();
        match resolve(d.path(), "/style.css").unwrap() {
            Resolved::File { body, content_type } => {
                assert!(content_type.starts_with("text/css"));
                assert_eq!(body, b"x{}");
            }
            _ => panic!("expected file"),
        }
    }

    #[test]
    fn missing_path_serves_404() {
        let d = setup();
        match resolve(d.path(), "/no/such").unwrap() {
            Resolved::NotFound { body } => {
                assert!(String::from_utf8(body).unwrap().contains("missing"));
            }
            _ => panic!("expected 404"),
        }
    }

    #[test]
    fn dotdot_rejected() {
        let d = setup();
        matches!(
            resolve(d.path(), "/../etc/passwd").unwrap(),
            Resolved::Forbidden
        );
    }
}

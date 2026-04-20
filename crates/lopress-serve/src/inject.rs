pub const RELOAD_SCRIPT: &str = "<script>\n\
(() => {\n\
  const es = new EventSource('/__lopress/reload');\n\
  es.addEventListener('reload', () => location.reload());\n\
})();\n\
</script>\n";

pub fn inject_reload_script(html: &[u8]) -> Vec<u8> {
    let s = match std::str::from_utf8(html) {
        Ok(s) => s,
        Err(_) => return html.to_vec(),
    };
    if let Some(idx) = s.rfind("</body>") {
        if let (Some(head), Some(tail)) = (s.get(..idx), s.get(idx..)) {
            let mut out = String::with_capacity(s.len() + RELOAD_SCRIPT.len());
            out.push_str(head);
            out.push_str(RELOAD_SCRIPT);
            out.push_str(tail);
            return out.into_bytes();
        }
    }
    let mut v = html.to_vec();
    v.extend_from_slice(RELOAD_SCRIPT.as_bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_before_body_close() {
        let html = b"<html><body><h1>Hi</h1></body></html>";
        let out = String::from_utf8(inject_reload_script(html)).unwrap();
        assert!(out.contains("EventSource"));
        assert!(out.find("EventSource").unwrap() < out.find("</body>").unwrap());
    }

    #[test]
    fn appends_when_no_body_close() {
        let html = b"<h1>plain</h1>";
        let out = String::from_utf8(inject_reload_script(html)).unwrap();
        assert!(out.ends_with("</script>\n"));
    }

    #[test]
    fn leaves_invalid_utf8_untouched() {
        let html = &[0xffu8, 0xfe, 0xfd];
        assert_eq!(inject_reload_script(html), html);
    }
}

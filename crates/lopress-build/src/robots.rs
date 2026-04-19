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

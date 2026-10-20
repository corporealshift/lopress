//! Embeds the app icon into `lopress.exe` so Explorer, pinned shortcuts and the
//! taskbar show it. The live window's icon is set separately at runtime
//! (`lopress-editor`'s `app_icon`).

const ICON: &str = "assets/icon/lopress.ico";

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed={ICON}");
    // Check the target, not the host: `#[cfg(windows)]` here would describe
    // the machine running the build script.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(ICON)
            .compile()?;
    }
    Ok(())
}

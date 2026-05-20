#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use lopress_gui_host::{BuildStatus, Session};
use tempfile::tempdir;

/// `Session::open` returns and the background build eventually completes
/// successfully on a minimal workspace.
#[test]
fn session_open_eventually_completes_initial_build() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("lopress.toml"),
        "[site]\ntitle = \"T\"\nbase_url = \"https://t\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(temp.path().join("src/posts")).unwrap();
    std::fs::create_dir_all(temp.path().join("src/pages")).unwrap();

    let session = Session::open(temp.path()).expect("open succeeded");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match session.build_status() {
            BuildStatus::Ok { .. } => break,
            BuildStatus::Failed { message } => panic!("build failed: {message}"),
            _ => {
                if Instant::now() > deadline {
                    panic!("build did not complete within 5s");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

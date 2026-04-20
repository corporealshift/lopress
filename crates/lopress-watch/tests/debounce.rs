#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::integer_division,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use lopress_watch::{ChangeSet, Watcher};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn flush(lock: &Arc<Mutex<Vec<ChangeSet>>>, timeout: Duration) -> Vec<ChangeSet> {
    let start = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let v = lock.lock().unwrap();
        if !v.is_empty() || start.elapsed() >= timeout {
            return v.clone();
        }
    }
}

#[test]
fn coalesces_rapid_writes_into_one_changeset() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();

    // Give notify time to arm.
    std::thread::sleep(Duration::from_millis(200));

    for i in 0..5 {
        let p = root.join(format!("src/posts/a{i}.md"));
        std::fs::write(&p, format!("hello {i}")).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    let batches = flush(&seen, Duration::from_secs(3));
    assert_eq!(
        batches.len(),
        1,
        "expected 1 debounced batch, got {}",
        batches.len()
    );
    assert!(!batches[0].sources.is_empty());
    assert!(batches[0].plugins.is_empty());
    assert!(!batches[0].config);
}

#[test]
fn separate_bursts_produce_separate_batches() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("src/posts")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    std::fs::write(root.join("src/posts/a.md"), "burst 1").unwrap();
    std::thread::sleep(Duration::from_millis(600));
    std::fs::write(root.join("src/posts/b.md"), "burst 2").unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        if seen.lock().unwrap().len() >= 2 {
            break;
        }
    }
    let batches = seen.lock().unwrap().clone();
    assert!(
        batches.len() >= 2,
        "expected >=2 debounced batches, got {}",
        batches.len()
    );
}

#[test]
fn ignores_www_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("www")).unwrap();

    let seen: Arc<Mutex<Vec<ChangeSet>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_cb = Arc::clone(&seen);
    let _watcher = Watcher::spawn(&root, move |cs| {
        seen_cb.lock().unwrap().push(cs);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    std::fs::write(root.join("www/index.html"), "hi").unwrap();
    std::thread::sleep(Duration::from_millis(800));
    assert!(
        seen.lock().unwrap().is_empty(),
        "www write should be ignored"
    );
}

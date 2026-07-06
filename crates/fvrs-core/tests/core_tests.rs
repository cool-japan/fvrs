//! Integration tests for fvrs-core: watcher event delivery, hashing,
//! directory comparison, and recursive copy.
//!
//! All fixtures live under `std::env::temp_dir()` in uniquely named
//! subdirectories and are removed on completion.

use fvrs_core::core::{ComparisonType, FileSystem, HashAlgorithm};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a unique, empty scratch directory under the system temp dir.
fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "fvrs_core_{}_{}_{}_{}",
        tag,
        std::process::id(),
        nanos,
        DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("failed to create scratch directory");
    dir
}

fn write_file(path: &PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write fixture file");
}

#[tokio::test]
async fn watcher_delivers_events() {
    let dir = unique_temp_dir("watch");
    let mut fs = FileSystem::new();
    fs.watch_directory(&dir).await.expect("failed to start watching");

    // Give the backend a moment to arm (FSEvents/inotify start asynchronously),
    // then keep touching the file until an event lands or we give up.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let file_path = dir.join("watched_file.txt");
    let mut delivered = None;
    for attempt in 0..8 {
        std::fs::write(&file_path, format!("attempt {}", attempt))
            .expect("failed to write watched file");
        if let Ok(event) = tokio::time::timeout(Duration::from_secs(2), fs.next_event()).await {
            delivered = event;
            break;
        }
    }
    let event = delivered.expect("no file system event delivered within timeout");

    // Backends may canonicalize (e.g. /var -> /private/var on macOS), so match
    // on the unique directory name instead of the full prefix.
    let marker = dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        .expect("scratch directory has no name");
    assert!(
        event.path.to_string_lossy().contains(&marker),
        "event path {:?} is not inside watched dir {:?}",
        event.path,
        dir
    );

    fs.stop_watching();
    assert!(
        fs.try_next_event().is_none(),
        "events still delivered after stop_watching"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn hash_round_trip_all_algorithms() {
    let dir = unique_temp_dir("hash");
    let file = dir.join("abc.txt");
    write_file(&file, "abc");

    let fs = FileSystem::new();
    let cases: [(HashAlgorithm, Option<&str>); 6] = [
        (
            HashAlgorithm::MD5,
            Some("900150983cd24fb0d6963f7d28e17f72"),
        ),
        (
            HashAlgorithm::SHA1,
            Some("a9993e364706816aba3e25717850c26c9cd0d89d"),
        ),
        (
            HashAlgorithm::SHA256,
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        ),
        (HashAlgorithm::SHA512, None),
        (HashAlgorithm::BLAKE3, None),
        (HashAlgorithm::RIPEMD160, None),
    ];

    for (algorithm, known_vector) in cases {
        let result = fs
            .calculate_hash(&file, algorithm)
            .await
            .expect("hash calculation failed");
        assert_eq!(result.size, 3, "unexpected input size for {:?}", algorithm);
        assert!(
            !result.hash.is_empty(),
            "empty hash for {:?}",
            algorithm
        );
        if let Some(expected) = known_vector {
            assert_eq!(
                result.hash, expected,
                "known test vector mismatch for {:?}",
                algorithm
            );
        }
        let verified = fs
            .verify_hash(&file, &result.hash, algorithm)
            .await
            .expect("hash verification failed");
        assert!(verified, "round-trip verify failed for {:?}", algorithm);
        let mismatch = fs
            .verify_hash(&file, "deadbeef", algorithm)
            .await
            .expect("hash verification failed");
        assert!(!mismatch, "bogus hash verified for {:?}", algorithm);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn compare_directories_detects_differences() {
    let root = unique_temp_dir("cmp");
    let left = root.join("left");
    let right = root.join("right");

    write_file(&left.join("same.txt"), "identical contents\n");
    write_file(&right.join("same.txt"), "identical contents\n");
    write_file(&left.join("diff.txt"), "left version\n");
    write_file(&right.join("diff.txt"), "right version\n");
    write_file(&left.join("only_left.txt"), "left only\n");
    write_file(&right.join("sub").join("only_right.txt"), "right only\n");

    let fs = FileSystem::new();
    let result = fs
        .compare_directories(&left, &right, ComparisonType::Text)
        .await
        .expect("directory comparison failed");

    assert!(!result.identical, "differing trees reported as identical");
    assert!(
        result.total_differences >= 3,
        "expected at least 3 differences (text diff + 2 missing entries), got {}",
        result.total_differences
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn compare_directories_identical_trees() {
    let root = unique_temp_dir("cmp_same");
    let left = root.join("left");
    let right = root.join("right");

    for base in [&left, &right] {
        write_file(&base.join("a.txt"), "alpha\n");
        write_file(&base.join("sub").join("b.txt"), "beta\n");
    }

    let fs = FileSystem::new();
    let result = fs
        .compare_directories(&left, &right, ComparisonType::Binary)
        .await
        .expect("directory comparison failed");

    assert!(result.identical, "identical trees reported as different");
    assert_eq!(result.total_differences, 0);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn copy_recursive_directory() {
    let root = unique_temp_dir("copy");
    let src = root.join("src");
    let dest = root.join("dest");

    write_file(&src.join("a.txt"), "top level\n");
    write_file(&src.join("sub").join("b.txt"), "nested\n");
    write_file(&src.join("sub").join("deeper").join("c.txt"), "deepest\n");
    std::fs::create_dir_all(src.join("empty")).expect("failed to create empty dir");

    let fs = FileSystem::new();
    fs.copy(&src, &dest).await.expect("recursive copy failed");

    for (rel, expected) in [
        ("a.txt", "top level\n"),
        ("sub/b.txt", "nested\n"),
        ("sub/deeper/c.txt", "deepest\n"),
    ] {
        let copied = dest.join(rel);
        let contents =
            std::fs::read_to_string(&copied).expect("copied file missing or unreadable");
        assert_eq!(contents, expected, "content mismatch for {:?}", copied);
    }
    assert!(
        dest.join("empty").is_dir(),
        "empty directory was not recreated"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

use super::{
    process_alive, reclaim_guard_path, reclaim_stale, reclaim_stale_with_grace, FileLock, LockError,
};

fn lock_path() -> PathBuf {
    std::env::temp_dir().join(format!("knots-lock-test-{}.lock", Uuid::now_v7()))
}

#[test]
fn try_lock_is_non_blocking() {
    let path = lock_path();
    let first = FileLock::try_acquire(&path)
        .expect("initial lock should not fail")
        .expect("initial lock should succeed");
    let second = FileLock::try_acquire(&path).expect("second lock call should not fail");
    assert!(second.is_none());
    drop(first);
    let _ = std::fs::remove_file(path);
}

#[test]
fn acquire_times_out_when_held() {
    let path = lock_path();
    let first = FileLock::try_acquire(&path)
        .expect("initial lock should not fail")
        .expect("initial lock should succeed");
    let err = FileLock::acquire(&path, Duration::from_millis(20))
        .expect_err("lock should time out when already held");
    assert!(err.to_string().contains("lock busy"));
    drop(first);
    let _ = std::fs::remove_file(path);
}

#[test]
fn lock_file_contains_pid() {
    let path = lock_path();
    let _guard = FileLock::try_acquire(&path)
        .expect("lock should not fail")
        .expect("lock should succeed");
    let contents = std::fs::read_to_string(&path).expect("lock file should be readable");
    let pid: u32 = contents
        .trim()
        .parse()
        .expect("lock file should contain a PID");
    assert_eq!(pid, std::process::id());
}

#[test]
fn stale_lock_is_reclaimed() {
    let path = lock_path();
    // Write a PID that doesn't exist (PID 1 is init, use a very
    // high number that almost certainly isn't running).
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    std::fs::write(&path, "4294967295").expect("stale lock fixture should be writable");
    let reclaimed = reclaim_stale(&path).expect("reclaim should not fail");
    assert!(reclaimed);
    assert!(!path.exists());
}

#[test]
fn corrupt_lock_is_reclaimed_after_grace() {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    std::fs::write(&path, "not-a-pid").expect("corrupt lock fixture should be writable");
    let reclaimed = reclaim_stale_with_grace(&path, Duration::ZERO, Duration::ZERO)
        .expect("reclaim should not fail");
    assert!(reclaimed);
    assert!(!path.exists());
}

#[test]
fn fresh_corrupt_lock_is_busy_within_grace() {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    // An empty lock file is what a contender sees between the holder's
    // create_new and its PID write; it must not be deleted as corrupt.
    std::fs::write(&path, "").expect("empty lock fixture should be writable");
    let reclaimed = reclaim_stale(&path).expect("reclaim should not fail");
    assert!(!reclaimed);
    assert!(path.exists(), "fresh empty lock must survive reclamation");
    let _ = std::fs::remove_file(path);
}

#[test]
fn reclamation_is_serialized_by_guard() {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    std::fs::write(&path, "4294967295").expect("stale lock fixture should be writable");
    let guard = reclaim_guard_path(&path);
    std::fs::write(&guard, "").expect("guard fixture should be writable");

    // A fresh guard means another process is mid-reclaim: report busy.
    let reclaimed = reclaim_stale(&path).expect("reclaim should not fail");
    assert!(!reclaimed);
    assert!(path.exists(), "lock must survive while guard is held");

    // An abandoned guard (aged past the grace) is broken and reclaim runs.
    let reclaimed = reclaim_stale_with_grace(&path, Duration::ZERO, Duration::ZERO)
        .expect("reclaim should not fail");
    assert!(reclaimed);
    assert!(!path.exists());
    assert!(!guard.exists(), "guard should be released after reclaim");
}

#[test]
fn unremovable_reclaim_guard_reports_busy() {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    std::fs::write(&path, "4294967295").expect("stale lock fixture should be writable");
    // A directory at the guard path cannot be created over or removed as a
    // file, exercising the guard's error fallbacks.
    let guard = reclaim_guard_path(&path);
    std::fs::create_dir_all(&guard).expect("guard directory fixture should be creatable");

    let reclaimed = reclaim_stale_with_grace(&path, Duration::ZERO, Duration::ZERO)
        .expect("reclaim should not fail");
    assert!(!reclaimed);
    assert!(path.exists(), "lock must survive when the guard is stuck");

    let _ = std::fs::remove_dir_all(&guard);
    let _ = std::fs::remove_file(path);
}

#[test]
fn reclaim_in_missing_directory_reports_busy() {
    let path = std::env::temp_dir()
        .join(format!("knots-lock-missing-{}", Uuid::now_v7()))
        .join("child.lock");
    let reclaimed = reclaim_stale(&path).expect("reclaim should not fail");
    assert!(!reclaimed, "missing parent means nothing to reclaim");
}

#[test]
fn live_process_is_not_reclaimed() {
    assert!(process_alive(std::process::id()));
}

#[test]
fn exited_child_process_is_detected_as_dead() {
    // On Windows the Child keeps a handle open, so liveness goes through
    // OpenProcess + WaitForSingleObject on a real exited process.
    #[cfg(windows)]
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit", "0"])
        .spawn()
        .expect("child should spawn");
    #[cfg(not(windows))]
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("child should spawn");
    let pid = child.id();
    child.wait().expect("child should exit");
    assert!(!process_alive(pid));
}

#[test]
fn remove_stale_lock_handles_missing_and_undeletable_paths() {
    use super::remove_stale_lock;

    let missing = lock_path();
    assert!(remove_stale_lock(&missing).expect("missing lock counts as removed"));

    #[cfg(windows)]
    {
        // A directory cannot be removed as a file; Windows reports access
        // denied, which must read as "not reclaimed" rather than an error.
        let dir = std::env::temp_dir().join(format!("knots-lock-undeletable-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("dir fixture should be creatable");
        assert!(!remove_stale_lock(&dir).expect("undeletable lock reports not removed"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn dead_process_is_detected() {
    // PID 4294967295 overflows i32 and should be treated as dead.
    assert!(!process_alive(4294967295));
}

#[test]
fn zero_pid_is_not_alive() {
    assert!(!process_alive(0));
}

#[test]
fn stale_lock_is_reclaimed_via_try_acquire() {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir should be creatable");
    }
    // Plant a stale lock with a dead PID.
    std::fs::write(&path, "99999").expect("stale lock fixture should be writable");
    // try_acquire should reclaim the stale lock and succeed.
    let guard = FileLock::try_acquire(&path)
        .expect("try_acquire should not fail")
        .expect("stale lock should be reclaimed");
    drop(guard);
}

#[test]
fn reclaim_stale_returns_true_for_missing_file() {
    let path = lock_path();
    // File doesn't exist — reclaim should return true.
    let reclaimed = reclaim_stale(&path).expect("reclaim should not fail");
    assert!(reclaimed);
}

#[test]
fn io_error_paths_surface_as_lock_errors() {
    let path = std::env::temp_dir().join(format!("knots-lock-dir-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&path).expect("directory path should be creatable");

    let converted = LockError::from(std::io::Error::other("boom"));
    assert!(converted.to_string().contains("boom"));

    assert!(!reclaim_stale(&path).expect("directory should not be reclaimed as stale"));
    let _ = std::fs::remove_dir_all(path);
}

#[cfg(unix)]
#[test]
fn try_acquire_reports_open_errors_from_read_only_directories() {
    let parent = std::env::temp_dir().join(format!("knots-lock-readonly-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&parent).expect("parent dir should be creatable");
    let original = std::fs::metadata(&parent)
        .expect("metadata should be readable")
        .permissions();
    let mut readonly = original.clone();
    readonly.set_mode(0o555);
    std::fs::set_permissions(&parent, readonly).expect("permissions should update");

    let path = parent.join("child.lock");
    let err = super::try_acquire(&path).expect_err("read-only directory should fail");
    assert!(err.to_string().contains("lock I/O error"));

    std::fs::set_permissions(&parent, original).expect("permissions should restore");
    let _ = std::fs::remove_dir_all(parent);
}

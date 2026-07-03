use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::locks::{FileLock, LockError};
use crate::project::{DistributionMode, StorePaths};

use super::*;

fn unique_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-queue-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("temp root should be creatable");
    root
}

fn state_request(
    root: &Path,
    paths: &QueuePaths,
    request_id: &str,
    knot_id: &str,
) -> QueuedWriteRequest {
    QueuedWriteRequest {
        request_id: request_id.to_string(),
        repo_root: root.display().to_string(),
        store_root: root.join(".knots").display().to_string(),
        distribution: "git".to_string(),
        project_id: None,
        db_path: root.join(".knots/cache/state.sqlite").display().to_string(),
        response_path: paths
            .responses_dir
            .join(format!("{request_id}.json"))
            .display()
            .to_string(),
        operation: WriteOperation::State(StateOperation {
            id: knot_id.to_string(),
            state: "implementation".to_string(),
            force: false,
            approve_terminal_cascade: false,
            if_match: None,
            actor_kind: None,
            agent_name: None,
            agent_model: None,
            agent_version: None,
        }),
    }
}

#[test]
fn drain_pending_requests_processes_all_items_serially() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let first = state_request(&root, &paths, "first", "K-1");
    enqueue_request(&paths, &first).expect("first request should enqueue");
    thread::sleep(Duration::from_millis(2));

    let second = state_request(&root, &paths, "second", "K-2");
    enqueue_request(&paths, &second).expect("second request should enqueue");

    let mut seen = Vec::new();
    let processed = drain_pending_requests(&paths, &mut |request| {
        seen.push(request.request_id.clone());
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("drain should succeed");
    assert_eq!(processed, 2);
    assert_eq!(seen, vec!["first".to_string(), "second".to_string()]);

    let first_response = read_response_file(&paths.responses_dir.join("first.json"))
        .expect("first response should read")
        .expect("first response should exist");
    assert_eq!(first_response.output, "first");

    let second_response = read_response_file(&paths.responses_dir.join("second.json"))
        .expect("second response should read")
        .expect("second response should exist");
    assert_eq!(second_response.output, "second");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_and_wait_round_trips_response() {
    let root = unique_root();
    let response = enqueue_and_wait(
        &root,
        ".knots/cache/state.sqlite",
        WriteOperation::Next(NextOperation {
            id: "K-123".to_string(),
            expected_state: Some("planning".to_string()),
            json: false,
            approve_terminal_cascade: false,
            actor_kind: None,
            agent_name: None,
            agent_model: None,
            agent_version: None,
            lease_id: None,
        }),
        |request| QueuedWriteResponse::success(request.request_id.clone()),
    )
    .expect("enqueue and wait should succeed");

    assert!(response.success);
    assert!(!response.output.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_pending_requests_returns_zero_when_worker_is_busy() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let _held = FileLock::try_acquire(&paths.worker_lock_path)
        .expect("worker lock acquisition should not fail")
        .expect("worker lock should be available");

    let processed = drain_pending_requests(&paths, &mut |_request| {
        QueuedWriteResponse::success("unexpected".to_string())
    })
    .expect("drain should return success when worker is busy");
    assert_eq!(processed, 0);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_pending_requests_removes_invalid_request_files() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let invalid = paths.requests_dir.join("invalid.json");
    fs::write(&invalid, "{not valid json").expect("invalid request file should be writable");

    let err = drain_pending_requests(&paths, &mut |_request| {
        QueuedWriteResponse::success("unused".to_string())
    })
    .expect_err("invalid request file should fail");
    assert!(matches!(err, QueueError::Serde(_)));
    assert!(
        !invalid.exists(),
        "invalid request file should be removed after parse failure"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_pending_requests_recovers_claimed_processing_files() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");
    let request = state_request(&root, &paths, "stale-processing", "K-stale");
    let stale = paths.requests_dir.join("stale.json.processing");
    let bytes = serde_json::to_vec(&request).expect("request should serialize");
    fs::write(&stale, bytes).expect("stale processing request should write");

    let processed = drain_pending_requests(&paths, &mut |request| {
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("stale processing request should drain");

    assert_eq!(processed, 1);
    assert!(!stale.exists());
    let response = read_response_file(&paths.responses_dir.join("stale-processing.json"))
        .expect("response should read")
        .expect("response should exist");
    assert_eq!(response.output, "stale-processing");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_pending_requests_discards_claimed_file_when_response_already_exists() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");
    let request = state_request(&root, &paths, "already-responded", "K-stale");
    let stale = paths.requests_dir.join("already-responded.json.processing");
    let bytes = serde_json::to_vec(&request).expect("request should serialize");
    fs::write(&stale, bytes).expect("stale processing request should write");
    write_response_file(
        Path::new(&request.response_path),
        &QueuedWriteResponse::success("persisted-response".to_string()),
    )
    .expect("response should write");

    let mut executor_called = false;
    let processed = drain_pending_requests(&paths, &mut |_request| {
        executor_called = true;
        QueuedWriteResponse::success("duplicate".to_string())
    })
    .expect("already responded processing request should drain");

    assert_eq!(processed, 0);
    assert!(!executor_called);
    assert!(!stale.exists());
    let response = read_response_file(Path::new(&request.response_path))
        .expect("response should read")
        .expect("response should exist");
    assert_eq!(response.output, "persisted-response");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_and_wait_spins_until_worker_lock_is_released() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let held = FileLock::try_acquire(&paths.worker_lock_path)
        .expect("worker lock acquisition should not fail")
        .expect("worker lock should be available");
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(60));
        drop(held);
    });

    let response = enqueue_and_wait(
        &root,
        ".knots/cache/state.sqlite",
        WriteOperation::Claim(ClaimOperation {
            id: "K-1".to_string(),
            json: false,
            verbose: false,
            agent_name: None,
            agent_model: None,
            agent_version: None,
            lease_id: None,
            timeout_seconds: None,
            e2e: false,
        }),
        |request| QueuedWriteResponse::success(request.request_id.clone()),
    )
    .expect("enqueue and wait should succeed once lock is released");
    assert!(response.success);
    assert!(!response.output.is_empty());
    releaser.join().expect("releaser thread should complete");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn queue_error_display_and_from_cover_all_variants() {
    let io_error = QueueError::from(std::io::Error::other("disk full"));
    assert!(format!("{io_error}").contains("queue I/O error"));

    let serde_error =
        QueueError::from(serde_json::from_str::<serde_json::Value>("{").expect_err("bad json"));
    assert!(format!("{serde_error}").contains("queue serialization error"));

    let lock_error = QueueError::from(LockError::Busy(PathBuf::from("busy.lock")));
    assert!(format!("{lock_error}").contains("queue lock error"));

    let timeout_error = QueueError::Timeout(Duration::from_millis(1));
    assert!(format!("{timeout_error}").contains("timed out waiting for queued write response"));
}

#[test]
fn list_request_files_and_read_response_file_handle_missing_paths() {
    let root = unique_root();

    let missing_requests = root.join("missing-requests");
    let files = list_request_files(&missing_requests).expect("missing dir should be treated empty");
    assert!(files.is_empty());

    let missing_response = root.join("missing-response.json");
    let response = read_response_file(&missing_response).expect("missing response should be ok");
    assert!(response.is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_and_wait_with_context_builds_local_only_absolute_request() {
    let root = unique_root();
    let store_paths = StorePaths {
        root: root.join("custom-store"),
    };
    fs::create_dir_all(&store_paths.root).expect("custom store should exist");
    let canonical_root = fs::canonicalize(&root).expect("root should canonicalize");
    let canonical_store = fs::canonicalize(&store_paths.root).expect("store should canonicalize");
    let expected_response_dir = canonical_store.join("queue").join("responses");

    let response = enqueue_and_wait_with_context(
        &root,
        &store_paths,
        DistributionMode::LocalOnly,
        Some("project-a".to_string()),
        "cache/state.sqlite",
        WriteOperation::QuickNew(QuickNewOperation {
            title: "Queued".to_string(),
            description: None,
            state: None,
            json: false,
        }),
        |request| {
            assert_eq!(request.repo_root, canonical_root.display().to_string());
            assert_eq!(request.store_root, canonical_store.display().to_string());
            assert_eq!(request.distribution, "local_only");
            assert_eq!(request.project_id.as_deref(), Some("project-a"));
            assert_eq!(
                request.db_path,
                canonical_store
                    .join("cache/state.sqlite")
                    .display()
                    .to_string()
            );
            assert!(Path::new(&request.response_path).starts_with(&expected_response_dir));
            QueuedWriteResponse::success("ok".to_string())
        },
    )
    .expect("local-only queued write should succeed");

    assert_eq!(response.output, "ok");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_and_wait_with_context_preserves_absolute_git_db_path() {
    let root = unique_root();
    let store_paths = StorePaths {
        root: root.join(".knots"),
    };
    fs::create_dir_all(&store_paths.root).expect("store should exist");
    let absolute_db = root.join("absolute-cache/state.sqlite");

    let response = enqueue_and_wait_with_context(
        &root,
        &store_paths,
        DistributionMode::Git,
        None,
        absolute_db.to_str().expect("absolute db should be utf8"),
        WriteOperation::QuickNew(QuickNewOperation {
            title: "Queued".to_string(),
            description: None,
            state: None,
            json: false,
        }),
        |request| {
            assert_eq!(request.distribution, "git");
            assert_eq!(request.db_path, absolute_db.display().to_string());
            QueuedWriteResponse::success("ok".to_string())
        },
    )
    .expect("git queued write should succeed");

    assert_eq!(response.output, "ok");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_request_persists_json_file_and_response_writer_creates_parent() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");
    let request = state_request(&root, &paths, "persisted", "K-persist");

    enqueue_request(&paths, &request).expect("request should enqueue");
    let files = list_request_files(&paths.requests_dir).expect("request files should list");
    assert_eq!(files.len(), 1);
    let raw = fs::read_to_string(&files[0]).expect("request json should read");
    assert!(raw.contains("\"request_id\":\"persisted\""));
    assert!(!files[0].with_extension("json.tmp").exists());

    let response_path = root.join("nested/responses/persisted.json");
    write_response_file(
        &response_path,
        &QueuedWriteResponse::failure("bad".to_string()),
    )
    .expect("response should write");
    let response = read_response_file(&response_path)
        .expect("response should read")
        .expect("response should exist");
    assert!(!response.success);
    assert_eq!(response.error.as_deref(), Some("bad"));
    assert!(!response_path.with_extension("json.tmp").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn list_request_files_filters_json_and_bad_response_errors() {
    let root = unique_root();
    let requests = root.join("requests");
    fs::create_dir_all(&requests).expect("request dir should exist");
    fs::write(requests.join("b.json"), "{}").expect("b request should write");
    fs::write(requests.join("a.json"), "{}").expect("a request should write");
    fs::write(requests.join("ignore.txt"), "{}").expect("ignored file should write");

    let files = list_request_files(&requests).expect("requests should list");
    let names: Vec<_> = files
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.json".to_string(), "b.json".to_string()]);

    let bad_response = root.join("bad-response.json");
    fs::write(&bad_response, "{not valid").expect("bad response should write");
    let err = read_response_file(&bad_response).expect_err("bad response should fail");
    assert!(matches!(err, QueueError::Serde(_)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn enqueue_request_errors_when_queue_dirs_are_missing() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    let request = state_request(&root, &paths, "missing-dirs", "K-missing-dirs");

    let err = enqueue_request(&paths, &request).expect_err("missing queue dirs should fail");
    assert!(matches!(err, QueueError::Io(_)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_pending_requests_propagates_response_write_errors() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");
    let mut request = state_request(&root, &paths, "bad-response-path", "K-bad-response-path");
    let response_parent = root.join("response-parent-is-file");
    fs::write(&response_parent, "not a directory").expect("file parent should write");
    request.response_path = response_parent.join("response.json").display().to_string();
    enqueue_request(&paths, &request).expect("request should enqueue");

    let err = drain_pending_requests(&paths, &mut |request| {
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect_err("response write should fail when parent path is a file");
    assert!(matches!(err, QueueError::Io(_)));
    let files = list_request_files(&paths.requests_dir)
        .expect("claimed request files should remain recoverable");
    assert_eq!(files.len(), 1);
    assert!(files[0]
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with(".json.processing")));
    let processing_files = fs::read_dir(&paths.requests_dir)
        .expect("request dir should list")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".json.processing")
        })
        .count();
    assert_eq!(processing_files, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn response_file_helpers_report_filesystem_errors() {
    let root = unique_root();
    let file_parent = root.join("file-parent");
    fs::write(&file_parent, "not a directory").expect("file parent should write");
    let response_path = file_parent.join("nested.json");

    let write_err = write_response_file(
        &response_path,
        &QueuedWriteResponse::success("ignored".to_string()),
    )
    .expect_err("file parent should block response write");
    assert!(matches!(write_err, QueueError::Io(_)));

    let read_err = read_response_file(&root).expect_err("directory response should not read");
    assert!(matches!(read_err, QueueError::Io(_)));

    let _ = fs::remove_dir_all(root);
}

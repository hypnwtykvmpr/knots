//! Crash-recovery behavior of the write-queue drain loop: committed work
//! must never re-execute, and leftover artifacts must be cleaned up.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn unique_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-queue-recovery-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).expect("temp root should be creatable");
    root
}

fn state_request(root: &Path, paths: &QueuePaths, request_id: &str) -> QueuedWriteRequest {
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
            id: "K-1".to_string(),
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

fn plant_claimed_file(paths: &QueuePaths, request: &QueuedWriteRequest) -> PathBuf {
    let claimed = paths
        .requests_dir
        .join(format!("1-{}.json.processing", request.request_id));
    let bytes = serde_json::to_vec(request).expect("request should serialize");
    fs::write(&claimed, bytes).expect("claimed fixture should write");
    claimed
}

#[test]
fn recovery_replays_done_marker_instead_of_reexecuting() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let request = state_request(&root, &paths, "r-done");
    let claimed = plant_claimed_file(&paths, &request);
    // Simulate a crash after execution committed (marker written) but where
    // the client already consumed the response file.
    let done_marker = paths.responses_dir.join("r-done.done");
    let committed = QueuedWriteResponse::success("committed-output".to_string());
    write_response_file(&done_marker, &committed).expect("done marker should write");

    let processed = drain_pending_requests(&paths, &mut |_request| {
        panic!("committed operations must not re-execute during recovery")
    })
    .expect("drain should succeed");

    assert_eq!(processed, 0, "recovery is delivery, not execution");
    let delivered = read_response_file(&paths.responses_dir.join("r-done.json"))
        .expect("response should read")
        .expect("response should be re-delivered");
    assert_eq!(delivered.output, "committed-output");
    assert!(!claimed.exists(), "claimed file should be cleaned up");
    assert!(!done_marker.exists(), "done marker should be cleaned up");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_without_marker_or_response_reexecutes_once() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let request = state_request(&root, &paths, "r-crash");
    let claimed = plant_claimed_file(&paths, &request);

    let mut executions = 0usize;
    let processed = drain_pending_requests(&paths, &mut |request| {
        executions += 1;
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("drain should succeed");

    assert_eq!(processed, 1);
    assert_eq!(executions, 1, "unfinished work is retried exactly once");
    assert!(!claimed.exists());
    assert!(
        !paths.responses_dir.join("r-crash.done").exists(),
        "done marker should not outlive a completed request"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_with_delivered_response_skips_reexecution() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let request = state_request(&root, &paths, "r-delivered");
    let claimed = plant_claimed_file(&paths, &request);
    // Crash landed after the response was delivered but before the claimed
    // file was removed; the response file is still waiting for its client.
    let delivered = QueuedWriteResponse::success("already-delivered".to_string());
    write_response_file(&paths.responses_dir.join("r-delivered.json"), &delivered)
        .expect("response fixture should write");

    let processed = drain_pending_requests(&paths, &mut |_request| {
        panic!("delivered operations must not re-execute during recovery")
    })
    .expect("drain should succeed");

    assert_eq!(processed, 0);
    assert!(!claimed.exists(), "claimed straggler should be cleaned up");
    let kept = read_response_file(&paths.responses_dir.join("r-delivered.json"))
        .expect("response should read")
        .expect("response should still await its client");
    assert_eq!(kept.output, "already-delivered");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn orphan_done_markers_are_swept() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let orphan = paths.responses_dir.join("long-gone.done");
    fs::write(&orphan, b"{}").expect("orphan marker fixture should write");

    let processed = drain_pending_requests(&paths, &mut |request| {
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("drain should succeed");

    assert_eq!(processed, 0);
    assert!(!orphan.exists(), "orphan marker should be swept");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn claim_and_removal_helpers_tolerate_missing_files() {
    let root = unique_root();
    let missing = root.join("queue").join("writes").join("9-r.json");

    let claimed = claim_request_file(&missing).expect("missing request should not error");
    assert_eq!(claimed, None, "a vanished request was claimed elsewhere");

    remove_file_with_retry(&missing).expect("removing a missing file is not an error");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn drain_handles_missing_queue_directories() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);

    let processed = drain_pending_requests(&paths, &mut |request| {
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("drain should tolerate a queue that was never created");

    assert_eq!(processed, 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn sweep_leaves_regular_response_files_alone() {
    let root = unique_root();
    let paths = QueuePaths::for_repo(&root);
    paths.create_dirs().expect("queue dirs should exist");

    let waiting = paths.responses_dir.join("waiting-client.json");
    let orphan = paths.responses_dir.join("gone.done");
    fs::write(&waiting, b"{}").expect("response fixture should write");
    fs::write(&orphan, b"{}").expect("marker fixture should write");

    drain_pending_requests(&paths, &mut |request| {
        QueuedWriteResponse::success(request.request_id.clone())
    })
    .expect("drain should succeed");

    assert!(waiting.exists(), "responses awaiting clients must survive");
    assert!(!orphan.exists(), "orphan markers must be swept");
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn sharing_violations_are_retried_as_transient() {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    let mut attempts = 0u32;
    let result = retry_transient(|| {
        attempts += 1;
        if attempts < 2 {
            Err(std::io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION))
        } else {
            Ok(())
        }
    });
    assert!(result.is_ok());
    assert_eq!(attempts, 2, "raw sharing violations should be retried");
}

#[test]
fn retry_transient_retries_access_errors_then_gives_up() {
    let mut attempts = 0u32;
    let result = retry_transient(|| {
        attempts += 1;
        if attempts < 3 {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    });
    assert!(result.is_ok());
    assert_eq!(attempts, 3, "transient errors should be retried");

    let mut exhausted = 0u32;
    let result = retry_transient(|| {
        exhausted += 1;
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    });
    assert!(result.is_err(), "persistent errors should surface");
    assert_eq!(exhausted, 5, "retries are bounded");

    let mut hard = 0u32;
    let result = retry_transient(|| {
        hard += 1;
        Err(std::io::Error::other("disk on fire"))
    });
    assert!(result.is_err());
    assert_eq!(hard, 1, "non-transient errors should not be retried");
}

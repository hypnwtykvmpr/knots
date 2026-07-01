use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;
use uuid::Uuid;

use crate::project::StorePaths;
use crate::sync::SyncError;

use super::ReplicationService;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-repl-local-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn service<'a>(
    conn: &'a Connection,
    repo_root: &Path,
    store_root: &Path,
) -> ReplicationService<'a> {
    ReplicationService::with_store_paths(
        conn,
        repo_root.to_path_buf(),
        StorePaths {
            root: store_root.to_path_buf(),
        },
    )
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("fixture should have parent"))
        .expect("fixture parent should be creatable");
    std::fs::write(path, bytes).expect("fixture should be writable");
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn collect_local_event_files_recurses_supported_roots_and_sorts_json() {
    let root = unique_workspace();
    let store_root = root.join("store");
    write(&store_root.join("events/2026/02/b.json"), b"{}");
    write(&store_root.join("index/2026/02/a.json"), b"{}");
    write(&store_root.join("snapshots/c.json"), b"{}");
    write(&store_root.join("events/2026/02/ignored.txt"), b"nope");
    write(&store_root.join("events/2026/02/ignored"), b"nope");

    let conn = Connection::open_in_memory().expect("in-memory db should open");
    let service = service(&conn, &root, &store_root);

    let files = service
        .collect_local_event_files()
        .expect("local event files should collect");
    let rendered = files.iter().map(|path| slash(path)).collect::<Vec<_>>();

    assert_eq!(
        rendered,
        vec![
            ".knots/events/2026/02/b.json",
            ".knots/index/2026/02/a.json",
            ".knots/snapshots/c.json",
        ]
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn copy_files_into_worktree_copies_new_files_skips_identical_and_rejects_conflicts() {
    let root = unique_workspace();
    let store_root = root.join("store");
    let worktree = root.join("worktree");
    let conn = Connection::open_in_memory().expect("in-memory db should open");
    let service = service(&conn, &root, &store_root);

    write(&store_root.join("index/new.json"), br#"{"new":true}"#);
    write(&store_root.join("events/same.json"), br#"{"same":true}"#);
    write(
        &worktree.join(".knots/events/same.json"),
        br#"{"same":true}"#,
    );

    let copied = service
        .copy_files_into_worktree(
            &worktree,
            &[
                PathBuf::from(".knots/index/new.json"),
                PathBuf::from(".knots/events/same.json"),
                PathBuf::from(".knots/snapshots/missing.json"),
            ],
        )
        .expect("copy should succeed");
    assert_eq!(copied, 1);
    assert_eq!(
        std::fs::read(worktree.join(".knots/index/new.json")).expect("copied file should read"),
        br#"{"new":true}"#
    );

    write(&store_root.join("events/conflict.json"), b"local");
    write(&worktree.join(".knots/events/conflict.json"), b"remote");
    let err = service
        .copy_files_into_worktree(&worktree, &[PathBuf::from(".knots/events/conflict.json")])
        .expect_err("different existing content should conflict");
    assert!(matches!(err, SyncError::FileConflict { .. }));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn event_file_missing_or_changed_compares_local_store_and_worktree_bytes() {
    let root = unique_workspace();
    let store_root = root.join("store");
    let worktree = root.join("worktree");
    let conn = Connection::open_in_memory().expect("in-memory db should open");
    let service = service(&conn, &root, &store_root);
    let relative = Path::new(".knots/index/item.json");

    assert!(!service
        .event_file_missing_or_changed(&worktree, relative)
        .expect("missing local file should not count"));

    write(&store_root.join("index/item.json"), b"local");
    assert!(service
        .event_file_missing_or_changed(&worktree, relative)
        .expect("missing worktree file should count"));

    write(&worktree.join(".knots/index/item.json"), b"local");
    assert!(!service
        .event_file_missing_or_changed(&worktree, relative)
        .expect("matching files should not count"));

    write(&worktree.join(".knots/index/item.json"), b"remote");
    assert!(service
        .event_file_missing_or_changed(&worktree, relative)
        .expect("changed worktree file should count"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn local_store_file_path_rejects_paths_outside_knots_root() {
    let root = unique_workspace();
    let conn = Connection::open_in_memory().expect("in-memory db should open");
    let service = service(&conn, &root, &root.join("store"));

    let err = service
        .local_store_file_path(Path::new("events/not-knots.json"))
        .expect_err("non .knots path should fail");
    assert!(matches!(err, SyncError::InvalidEvent { .. }));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn active_lease_replication_override_skips_database_check() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let root = unique_workspace();
    let conn = Connection::open_in_memory().expect("in-memory db should open");
    let service = service(&conn, &root, &root.join("store"));

    std::env::set_var("KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION", "1");
    service
        .require_no_active_leases()
        .expect("env override should bypass lease query");
    std::env::remove_var("KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn active_lease_requirement_allows_empty_db_and_rejects_active_leases() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    std::env::remove_var("KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION");
    let root = unique_workspace();
    let db_path = root.join("state.sqlite");
    let conn = crate::db::open_connection(&db_path.to_string_lossy())
        .expect("schema-backed connection should open");
    let service = service(&conn, &root, &root.join("store"));

    service
        .require_no_active_leases()
        .expect("empty schema should have no active leases");

    seed_active_lease(&conn, "lease-active");
    let err = service
        .require_no_active_leases()
        .expect_err("active lease should block replication");
    assert!(matches!(err, SyncError::ActiveLeasesExist(1)));

    let _ = std::fs::remove_dir_all(root);
}

fn seed_active_lease(conn: &Connection, id: &str) {
    let gate_data = crate::domain::gate::GateData::default();
    let lease_data = crate::domain::lease::LeaseData::default();
    let execution_plan = crate::domain::execution_plan::ExecutionPlanData::default();
    crate::db::upsert_knot_hot(
        conn,
        &crate::db::UpsertKnotHot {
            id,
            title: id,
            state: "lease_active",
            updated_at: "2026-03-12T00:00:00Z",
            body: None,
            description: None,
            acceptance: None,
            priority: None,
            knot_type: Some("lease"),
            tags: &[],
            notes: &[],
            handoff_capsules: &[],
            invariants: &[],
            verification_steps: &[],
            step_history: &[],
            gate_data: &gate_data,
            lease_data: &lease_data,
            execution_plan_data: &execution_plan,
            lease_id: None,
            workflow_id: "lease",
            profile_id: "lease/default",
            profile_etag: None,
            deferred_from_state: None,
            blocked_from_state: None,
            created_at: None,
        },
    )
    .expect("active lease should insert");
    let future = crate::lease_expiry::compute_expiry_ts(600);
    crate::db::update_lease_expiry_ts(conn, id, future).expect("expiry should update");
}

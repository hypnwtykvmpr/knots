use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use uuid::Uuid;

use crate::db::{self, EdgeDirection, UpsertKnotHot};
use crate::sync::{GitAdapter, SyncError};

use super::{FullApplyOutcome, IncrementalApplier};

pub(super) fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-sync-apply-local-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

pub(super) fn run_git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(super) fn setup_repo() -> PathBuf {
    let root = unique_workspace();
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "knots@example.com"]);
    run_git(&root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# apply\n").expect("readme should be writable");
    run_git(&root, &["add", "README.md"]);
    run_git(&root, &["commit", "-m", "init"]);
    root
}

pub(super) fn open_conn(root: &Path) -> rusqlite::Connection {
    let db_path = root.join(".knots/cache/state.sqlite");
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    db::open_connection(db_path.to_str().expect("utf8 db path")).expect("db should open")
}

pub(super) fn write_json(path: &Path, value: serde_json::Value) {
    std::fs::create_dir_all(path.parent().expect("fixture should have parent"))
        .expect("fixture parent should be creatable");
    std::fs::write(path, value.to_string()).expect("fixture should be writable");
}

pub(super) fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn seed_hot_knot(conn: &rusqlite::Connection, knot_id: &str) {
    db::upsert_knot_hot(
        conn,
        &UpsertKnotHot {
            id: knot_id,
            title: "Seed",
            state: "work_item",
            updated_at: "2026-02-25T10:00:00Z",
            body: None,
            description: None,
            acceptance: None,
            priority: None,
            knot_type: None,
            tags: &["alpha".to_string()],
            notes: &[],
            handoff_capsules: &[],
            invariants: &[],
            verification_steps: &[],
            step_history: &[],
            gate_data: &crate::domain::gate::GateData::default(),
            lease_data: &crate::domain::lease::LeaseData::default(),
            execution_plan_data: &crate::domain::execution_plan::ExecutionPlanData::default(),
            lease_id: None,
            workflow_id: "work_sdlc",
            profile_id: "autopilot",
            profile_etag: Some("etag-1"),
            deferred_from_state: None,
            blocked_from_state: None,
            created_at: Some("2026-02-25T10:00:00Z"),
        },
    )
    .expect("hot knot should upsert");
}

#[test]
fn changed_files_scans_missing_and_nested_json_prefixes_in_sorted_order() {
    let root = unique_workspace();
    let conn = open_conn(&root);
    let applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let missing = applier
        .changed_files("last_index_head_commit", ".knots/missing", "HEAD")
        .expect("missing prefix should scan as empty");
    assert!(missing.is_empty());

    write_json(&root.join(".knots/index/2026/02/b.json"), json!({}));
    write_json(&root.join(".knots/index/2026/01/a.json"), json!({}));
    std::fs::write(root.join(".knots/index/2026/02/ignore.txt"), "ignore")
        .expect("non-json fixture should write");

    let files = applier
        .changed_files("last_index_head_commit", ".knots/index", "HEAD")
        .expect("scan fallback should collect JSON files");
    let rendered = files.iter().map(|path| slash(path)).collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![".knots/index/2026/01/a.json", ".knots/index/2026/02/b.json",]
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn changed_files_uses_git_diff_filters_json_and_handles_meta_edges() {
    let root = setup_repo();
    let conn = open_conn(&root);
    let base = git_stdout(&root, &["rev-parse", "HEAD"]);

    write_json(
        &root.join(".knots/index/2026/02/z.json"),
        json!({ "z": true }),
    );
    write_json(
        &root.join(".knots/index/2026/01/a.json"),
        json!({ "a": true }),
    );
    std::fs::write(root.join(".knots/index/2026/02/skip.txt"), "skip")
        .expect("text fixture should write");
    run_git(&root, &["add", ".knots"]);
    run_git(&root, &["commit", "-m", "seed indexed files"]);
    let target = git_stdout(&root, &["rev-parse", "HEAD"]);

    db::set_meta(&conn, "last_index_head_commit", &base).expect("meta should set");
    let applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    let files = applier
        .changed_files("last_index_head_commit", ".knots/index", &target)
        .expect("git diff should collect changed JSON files");
    let rendered = files.iter().map(|path| slash(path)).collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![".knots/index/2026/01/a.json", ".knots/index/2026/02/z.json",]
    );

    db::set_meta(&conn, "last_index_head_commit", &target).expect("meta should update");
    let same = applier
        .changed_files("last_index_head_commit", ".knots/index", &target)
        .expect("same head should not inspect git");
    assert!(same.is_empty());

    let missing_worktree = IncrementalApplier::new_with_builtins(
        &conn,
        root.join("missing-worktree"),
        GitAdapter::new(),
    );
    db::set_meta(&conn, "last_index_head_commit", &base).expect("meta should reset");
    let err = missing_worktree
        .changed_files("last_index_head_commit", ".knots/index", &target)
        .expect_err("nonexistent git worktree should propagate git errors");
    assert!(matches!(err, SyncError::GitCommandFailed { .. }));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_full_event_handles_missing_invalid_edge_add_and_edge_remove_paths() {
    let root = setup_repo();
    let conn = open_conn(&root);
    let applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let missing = applier
        .apply_full_event(Path::new(".knots/events/missing.json"))
        .expect("missing full event should be ignored");
    assert!(matches!(missing, FullApplyOutcome::Ignored));

    write_json(
        &root.join(".knots/events/2026/02/25/1000-knot.description_set.json"),
        json!({
            "event_id": "1000",
            "occurred_at": "2026-02-25T10:00:00Z",
            "knot_id": "K-edge",
            "type": "knot.description_set",
            "data": "bad"
        }),
    );
    let err = applier
        .apply_full_event(Path::new(
            ".knots/events/2026/02/25/1000-knot.description_set.json",
        ))
        .expect_err("non-object full event data should fail");
    assert!(matches!(err, SyncError::InvalidEvent { .. }));

    write_json(
        &root.join(".knots/events/2026/02/25/1001-knot.edge_add.json"),
        json!({
            "event_id": "1001",
            "occurred_at": "2026-02-25T10:01:00Z",
            "knot_id": "K-edge",
            "type": "knot.edge_add",
            "data": { "kind": "blocks", "dst": "K-dst" }
        }),
    );
    let added = applier
        .apply_full_event(Path::new(
            ".knots/events/2026/02/25/1001-knot.edge_add.json",
        ))
        .expect("edge add should apply");
    assert!(matches!(added, FullApplyOutcome::EdgeAdded));
    let edges =
        db::list_edges(&conn, "K-edge", EdgeDirection::Outgoing).expect("edges should list");
    assert_eq!(edges.len(), 1);

    write_json(
        &root.join(".knots/events/2026/02/25/1002-knot.edge_remove.json"),
        json!({
            "event_id": "1002",
            "occurred_at": "2026-02-25T10:02:00Z",
            "knot_id": "K-edge",
            "type": "knot.edge_remove",
            "data": { "kind": "blocks", "dst": "K-dst" }
        }),
    );
    let removed = applier
        .apply_full_event(Path::new(
            ".knots/events/2026/02/25/1002-knot.edge_remove.json",
        ))
        .expect("edge remove should apply");
    assert!(matches!(removed, FullApplyOutcome::EdgeRemoved));
    let edges = db::list_edges(&conn, "K-edge", EdgeDirection::Outgoing)
        .expect("edges should list after remove");
    assert!(edges.is_empty());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_index_event_moves_unparseable_terminal_heads_to_warm_tier() {
    let root = setup_repo();
    let conn = open_conn(&root);
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    write_json(
        &root.join(".knots/index/2026/02/25/2000-idx.knot_head.json"),
        json!({
            "event_id": "2000",
            "occurred_at": "2026-02-25T10:00:00Z",
            "type": "idx.knot_head",
            "data": {
                "knot_id": "K-warm",
                "title": "Warm terminal",
                "state": "shipped",
                "workflow_id": "work_sdlc",
                "profile_id": "autopilot",
                "updated_at": "not-a-date",
                "terminal": true
            }
        }),
    );

    let updated = applier
        .apply_index_event(Path::new(".knots/index/2026/02/25/2000-idx.knot_head.json"))
        .expect("warm-tier index event should apply");
    assert!(updated);
    assert!(db::get_knot_hot(&conn, "K-warm")
        .expect("hot lookup")
        .is_none());
    let warm = db::get_knot_warm(&conn, "K-warm")
        .expect("warm lookup")
        .expect("warm record should exist");
    assert_eq!(warm.title, "Warm terminal");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_index_event_covers_missing_invalid_unknown_and_skipped_full_paths() {
    let root = setup_repo();
    let conn = open_conn(&root);
    let mut empty_known = IncrementalApplier::new(
        &conn,
        root.clone(),
        GitAdapter::new(),
        std::collections::HashSet::new(),
    );

    let missing = empty_known
        .apply_index_event(Path::new(".knots/index/missing.json"))
        .expect("missing index event should be ignored");
    assert!(!missing);

    write_json(
        &root.join(".knots/index/2026/02/25/2100-idx.other.json"),
        json!({
            "event_id": "2100",
            "occurred_at": "2026-02-25T10:00:00Z",
            "type": "idx.other",
            "data": {}
        }),
    );
    assert!(!empty_known
        .apply_index_event(Path::new(".knots/index/2026/02/25/2100-idx.other.json"))
        .expect("non-head index event should be ignored"));

    write_json(
        &root.join(".knots/index/2026/02/25/2101-idx.knot_head.json"),
        json!({
            "event_id": "2101",
            "occurred_at": "2026-02-25T10:00:00Z",
            "type": "idx.knot_head",
            "data": "bad"
        }),
    );
    let err = empty_known
        .apply_index_event(Path::new(".knots/index/2026/02/25/2101-idx.knot_head.json"))
        .expect_err("non-object index data should fail");
    assert!(matches!(err, SyncError::InvalidEvent { .. }));

    write_json(
        &root.join(".knots/index/2026/02/25/2102-idx.knot_head.json"),
        json!({
            "event_id": "2102",
            "occurred_at": "2026-02-25T10:00:00Z",
            "type": "idx.knot_head",
            "data": {
                "knot_id": "K-unknown",
                "title": "Unknown workflow",
                "state": "work_item",
                "workflow_id": "missing_flow",
                "profile_id": "autopilot",
                "updated_at": "2026-02-25T10:00:00Z",
                "terminal": false
            }
        }),
    );
    assert!(!empty_known
        .apply_index_event(Path::new(".knots/index/2026/02/25/2102-idx.knot_head.json"))
        .expect("unknown workflow index event should be skipped"));

    write_json(
        &root.join(".knots/events/2026/02/25/2103-knot.description_set.json"),
        json!({
            "event_id": "2103",
            "occurred_at": "2026-02-25T10:01:00Z",
            "knot_id": "K-unknown",
            "type": "knot.description_set",
            "data": { "description": "ignored because workflow was unknown" }
        }),
    );
    let skipped_full = empty_known
        .apply_full_event(Path::new(
            ".knots/events/2026/02/25/2103-knot.description_set.json",
        ))
        .expect("full event for unknown workflow knot should be ignored");
    assert!(matches!(skipped_full, FullApplyOutcome::Ignored));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_index_event_covers_stale_precondition_and_cold_tier_paths() {
    let root = setup_repo();
    let conn = open_conn(&root);
    seed_hot_knot(&conn, "K-meta");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    write_json(
        &root.join(".knots/index/2026/02/25/2104-idx.knot_head.json"),
        json!({
            "event_id": "2104",
            "occurred_at": "2026-02-25T10:00:00Z",
            "type": "idx.knot_head",
            "precondition": { "profile_etag": "different-etag" },
            "data": {
                "knot_id": "K-meta",
                "title": "Stale update",
                "state": "work_item",
                "workflow_id": "work_sdlc",
                "profile_id": "autopilot",
                "updated_at": "2026-02-25T10:00:00Z",
                "terminal": false
            }
        }),
    );
    assert!(!applier
        .apply_index_event(Path::new(".knots/index/2026/02/25/2104-idx.knot_head.json"))
        .expect("stale precondition should skip update"));
    let unchanged = db::get_knot_hot(&conn, "K-meta")
        .expect("hot lookup")
        .expect("seed knot should remain hot");
    assert_eq!(unchanged.title, "Seed");

    write_json(
        &root.join(".knots/index/2020/01/01/2105-idx.knot_head.json"),
        json!({
            "event_id": "2105",
            "occurred_at": "2020-01-01T00:00:00Z",
            "type": "idx.knot_head",
            "data": {
                "knot_id": "K-cold",
                "title": "Cold terminal",
                "state": "shipped",
                "workflow_id": "work_sdlc",
                "profile_id": "autopilot",
                "updated_at": "2020-01-01T00:00:00Z",
                "terminal": true
            }
        }),
    );
    assert!(applier
        .apply_index_event(Path::new(".knots/index/2020/01/01/2105-idx.knot_head.json"))
        .expect("old terminal head should apply into cold"));
    assert!(db::get_knot_hot(&conn, "K-cold")
        .expect("hot lookup")
        .is_none());
    let cold = db::get_cold_catalog(&conn, "K-cold")
        .expect("cold lookup")
        .expect("cold catalog row should exist");
    assert_eq!(cold.title, "Cold terminal");

    let _ = std::fs::remove_dir_all(root);
}

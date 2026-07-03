use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use uuid::Uuid;

use crate::db;
use crate::sync::GitAdapter;

use super::IncrementalApplier;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-sync-unknown-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn run_git(root: &Path, args: &[&str]) {
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

fn setup_repo() -> PathBuf {
    let root = unique_workspace();
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "knots@example.com"]);
    run_git(&root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# apply\n").expect("readme should be writable");
    run_git(&root, &["add", "README.md"]);
    run_git(&root, &["commit", "-m", "init"]);
    root
}

fn open_conn(root: &Path) -> rusqlite::Connection {
    let db_path = root.join(".knots/cache/state.sqlite");
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    db::open_connection(db_path.to_str().expect("utf8 db path")).expect("db should open")
}

fn seed_hot_knot(conn: &rusqlite::Connection, knot_id: &str) {
    db::upsert_knot_hot(
        conn,
        &db::UpsertKnotHot {
            id: knot_id,
            title: "Seed",
            state: "work_item",
            updated_at: "2026-02-25T10:00:00Z",
            body: None,
            description: None,
            acceptance: None,
            priority: None,
            knot_type: Some("work"),
            tags: &[],
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
            profile_etag: Some("seed-etag"),
            deferred_from_state: None,
            blocked_from_state: None,
            created_at: Some("2026-02-25T10:00:00Z"),
        },
    )
    .expect("hot knot should seed");
}

fn recent_ts() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("timestamp should format")
}

fn write_unknown_workflow_head(root: &Path, knot_id: &str, filename: &str) -> PathBuf {
    let idx_dir = root.join(".knots/index/2026/02/25");
    std::fs::create_dir_all(&idx_dir).expect("index directory should be creatable");
    let ts = recent_ts();
    let payload = serde_json::json!({
        "event_id": "9000",
        "occurred_at": ts,
        "type": "idx.knot_head",
        "data": {
            "knot_id": knot_id,
            "title": "Future workflow",
            "state": "work_item",
            "workflow_id": "future_sdlc",
            "profile_id": "autopilot",
            "updated_at": ts,
            "terminal": false
        }
    });
    std::fs::write(idx_dir.join(filename), payload.to_string()).expect("index event should write");
    Path::new(".knots/index/2026/02/25").join(filename)
}

#[test]
fn apply_index_event_skips_unknown_workflow_with_import_warning() {
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    let rel_path = write_unknown_workflow_head(&root, "K-future", "9000-idx.knot_head.json");

    let updated = applier
        .apply_index_event(&rel_path)
        .expect("unknown workflow should skip, not fail");
    assert!(!updated);
    assert!(
        db::get_knot_hot(&conn, "K-future")
            .expect("hot lookup should succeed")
            .is_none(),
        "skipped knot should not be imported"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_full_event_skips_knot_after_unknown_workflow_head() {
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    let head_path = write_unknown_workflow_head(&root, "K-future-full", "9001-idx.knot_head.json");

    let full_dir = root.join(".knots/events/2026/02/25");
    std::fs::create_dir_all(&full_dir).expect("full event directory should be creatable");
    let full_payload = serde_json::json!({
        "event_id": "9002",
        "occurred_at": recent_ts(),
        "knot_id": "K-future-full",
        "type": "knot.edge_add",
        "data": {
            "kind": "relates_to",
            "dst": "K-other"
        }
    });
    std::fs::write(
        full_dir.join("9002-knot.edge_add.json"),
        full_payload.to_string(),
    )
    .expect("full event should write");

    applier
        .apply_index_event(&head_path)
        .expect("unknown workflow should skip, not fail");
    let outcome = applier
        .apply_full_event(Path::new(
            ".knots/events/2026/02/25/9002-knot.edge_add.json",
        ))
        .expect("full event for skipped knot should not fail");
    assert!(matches!(outcome, super::FullApplyOutcome::Ignored));
    assert_eq!(
        db::list_edges(&conn, "K-future-full", db::EdgeDirection::Outgoing)
            .expect("edge lookup should succeed")
            .len(),
        0
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_full_event_updates_structured_metadata_payloads() {
    let root = setup_repo();
    let conn = open_conn(&root);
    seed_hot_knot(&conn, "K-structured");
    let applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    let events_dir = root.join(".knots/events/2026/02/25");
    std::fs::create_dir_all(&events_dir).expect("events dir should exist");

    for (index, (filename, event_type, data)) in [
        (
            "9100-knot.gate_data_set.json",
            "knot.gate_data_set",
            json!({"gate": {"owner_kind": "human", "failure_modes": {"broken": ["K-a"]}}}),
        ),
        (
            "9101-knot.lease_data_set.json",
            "knot.lease_data_set",
            json!({"lease_data": {"lease_type": "manual", "nickname": "Manual"}}),
        ),
        (
            "9102-knot.execution_plan_data_set.json",
            "knot.execution_plan_data_set",
            json!({"execution_plan": {"objective": "Coordinate work"}}),
        ),
        (
            "9103-knot.scope_set.json",
            "knot.scope_set",
            json!({"volume": 8, "scale": "fib_v1", "reliability": 95}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        std::fs::write(
            events_dir.join(filename),
            json!({
                "event_id": format!("91{index}"),
                "occurred_at": "2026-02-25T11:00:00Z",
                "knot_id": "K-structured",
                "type": event_type,
                "data": data
            })
            .to_string(),
        )
        .expect("structured event should write");
        applier
            .apply_full_event(
                Path::new(".knots/events/2026/02/25")
                    .join(filename)
                    .as_path(),
            )
            .expect("structured metadata should apply");
    }

    let updated = db::get_knot_hot(&conn, "K-structured")
        .expect("hot lookup should succeed")
        .expect("structured knot should still be hot");
    assert_eq!(updated.gate_data.owner_kind.as_str(), "human");
    assert_eq!(updated.lease_data.nickname, "Manual");
    assert_eq!(
        updated.execution_plan_data.objective.as_deref(),
        Some("Coordinate work")
    );
    assert_eq!(updated.scope_data.volume, Some(8));
    assert_eq!(updated.scope_data.reliability, Some(95));

    let _ = std::fs::remove_dir_all(root);
}

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::db;
use crate::domain::execution_plan::{
    ExecutionPlanAgent, ExecutionPlanData, ExecutionPlanKnot, ExecutionPlanStep, ExecutionPlanWave,
};
use crate::sync::GitAdapter;

use super::IncrementalApplier;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-sync-apply-plan-{}", Uuid::now_v7()));
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

fn legacy_ids_key() -> &'static str {
    concat!("be", "at", "_ids")
}

fn legacy_unassigned_ids_key() -> &'static str {
    concat!("unassigned_", "be", "at", "_ids")
}

#[test]
fn apply_index_event_reads_execution_plan_payload() {
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let idx_dir = root.join(".knots/index/2026/04/14");
    std::fs::create_dir_all(&idx_dir).expect("index directory should be creatable");
    let ts = "2026-04-14T10:00:00Z";
    let payload = serde_json::json!({
        "event_id": "7100",
        "occurred_at": ts,
        "type": "idx.knot_head",
        "data": {
            "knot_id": "K-plan",
            "title": "Execution plan",
            "state": "design",
            "workflow_id": "work_sdlc",
            "profile_id": "autopilot",
            "updated_at": ts,
            "terminal": false,
            "type": "execution_plan",
            "execution_plan": {
                "repo_path": "/repo",
                "objective": "Ship sync payload",
                "summary": "Execution plan for sync payload",
                "waves": [{
                    "wave_index": 1,
                    "name": "Persist",
                    "objective": "Store the payload",
                    "agents": [{
                        "role": "backend",
                        "count": 1
                    }],
                    "knots": [{
                        "id": "knot-1",
                        "title": "Persist payload"
                    }],
                    "steps": [{
                        "step_index": 1,
                        "knot_ids": ["knot-1"],
                        "notes": "Read index"
                    }],
                    "notes": "Wave note"
                }]
            }
        }
    });
    std::fs::write(idx_dir.join("7100-idx.knot_head.json"), payload.to_string())
        .expect("index event should write");

    let updated = applier
        .apply_index_event(Path::new(".knots/index/2026/04/14/7100-idx.knot_head.json"))
        .expect("index event should apply");
    assert!(updated);

    let record = db::get_knot_hot(&conn, "K-plan")
        .expect("hot lookup should succeed")
        .expect("record should exist");
    let expected = ExecutionPlanData {
        repo_path: Some("/repo".to_string()),
        objective: Some("Ship sync payload".to_string()),
        summary: Some("Execution plan for sync payload".to_string()),
        mode: None,
        model: None,
        assumptions: vec![],
        knot_ids: vec![],
        unassigned_knot_ids: vec![],
        waves: vec![ExecutionPlanWave {
            wave_index: 1,
            name: "Persist".to_string(),
            objective: "Store the payload".to_string(),
            agents: vec![ExecutionPlanAgent {
                role: "backend".to_string(),
                count: 1,
                specialty: None,
            }],
            knots: vec![ExecutionPlanKnot {
                id: "knot-1".to_string(),
                title: "Persist payload".to_string(),
            }],
            steps: vec![ExecutionPlanStep {
                step_index: 1,
                knot_ids: vec!["knot-1".to_string()],
                notes: Some("Read index".to_string()),
            }],
            notes: Some("Wave note".to_string()),
        }],
    };
    assert_eq!(record.execution_plan_data, expected);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_index_event_reads_legacy_ids_payload_and_emits_knot_ids() {
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let idx_dir = root.join(".knots/index/2026/04/15");
    std::fs::create_dir_all(&idx_dir).expect("index directory should be creatable");
    let ts = "2026-04-15T10:00:00Z";

    let mut step = Map::new();
    step.insert("step_index".to_string(), serde_json::json!(1));
    step.insert(legacy_ids_key().to_string(), serde_json::json!(["knot-1"]));
    step.insert("notes".to_string(), serde_json::json!("Read legacy index"));

    let mut execution_plan = Map::new();
    execution_plan.insert("repo_path".to_string(), serde_json::json!("/repo"));
    execution_plan.insert(
        "objective".to_string(),
        serde_json::json!("Ship legacy sync payload"),
    );
    execution_plan.insert(
        "summary".to_string(),
        serde_json::json!("Execution plan for legacy sync payload"),
    );
    execution_plan.insert(legacy_ids_key().to_string(), serde_json::json!(["knot-1"]));
    execution_plan.insert(
        legacy_unassigned_ids_key().to_string(),
        serde_json::json!(["knot-2"]),
    );
    execution_plan.insert(
        "waves".to_string(),
        serde_json::json!([{
            "wave_index": 1,
            "name": "Persist",
            "objective": "Store the payload",
            "steps": [Value::Object(step)]
        }]),
    );

    let payload = serde_json::json!({
        "event_id": "7200",
        "occurred_at": ts,
        "type": "idx.knot_head",
        "data": {
            "knot_id": "K-plan-legacy",
            "title": "Execution plan",
            "state": "design",
            "workflow_id": "work_sdlc",
            "profile_id": "autopilot",
            "updated_at": ts,
            "terminal": false,
            "type": "execution_plan",
            "execution_plan": Value::Object(execution_plan)
        }
    });
    std::fs::write(idx_dir.join("7200-idx.knot_head.json"), payload.to_string())
        .expect("index event should write");

    let updated = applier
        .apply_index_event(Path::new(".knots/index/2026/04/15/7200-idx.knot_head.json"))
        .expect("index event should apply");
    assert!(updated);

    let record = db::get_knot_hot(&conn, "K-plan-legacy")
        .expect("hot lookup should succeed")
        .expect("record should exist");
    assert_eq!(record.execution_plan_data.knot_ids, vec!["knot-1"]);
    assert_eq!(
        record.execution_plan_data.unassigned_knot_ids,
        vec!["knot-2"]
    );
    assert_eq!(
        record.execution_plan_data.waves[0].steps[0].knot_ids,
        vec!["knot-1"]
    );

    let serialized = serde_json::to_string(&record.execution_plan_data)
        .expect("canonical payload should serialize");
    assert!(serialized.contains("knot_ids"));
    assert!(!serialized.contains(legacy_ids_key()));

    let _ = std::fs::remove_dir_all(root);
}

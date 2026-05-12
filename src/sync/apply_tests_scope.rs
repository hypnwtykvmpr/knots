//! Tests for scope metadata in `idx.knot_head` apply path.
//!
//! Foolery's fast list/index path reads `.knots/index/**/*.json` head events
//! directly without rehydrating from the full event log. To make scope visible
//! there, the head payload must carry it under the `"scope"` key, and the
//! consumer must prefer that payload over the cached value when present.

use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

use crate::db;
use crate::sync::GitAdapter;

use super::IncrementalApplier;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-sync-scope-{}", Uuid::now_v7()));
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
    std::fs::write(root.join("README.md"), "# scope\n").expect("readme should be writable");
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

fn write_head_event(root: &Path, filename: &str, body: &str) -> PathBuf {
    let idx_dir = root.join(".knots/index/2026/05/12");
    std::fs::create_dir_all(&idx_dir).expect("index dir creatable");
    let path = idx_dir.join(filename);
    std::fs::write(&path, body).expect("event should be writable");
    Path::new(".knots/index/2026/05/12").join(filename)
}

fn recent_ts() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("timestamp should format")
}

#[test]
fn apply_index_event_persists_scope_from_payload_without_full_event() {
    // A pulled head with embedded scope must materialize knot_hot.scope_data
    // even when no paired knot.scope_set full event has been applied yet.
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let ts = recent_ts();
    let body = format!(
        concat!(
            "{{\n",
            "  \"event_id\": \"019c942e-0001-7883-bb13-27197273b8f5\",\n",
            "  \"occurred_at\": \"{ts}\",\n",
            "  \"type\": \"idx.knot_head\",\n",
            "  \"data\": {{\n",
            "    \"knot_id\": \"K-scope-payload\",\n",
            "    \"title\": \"Scoped from head\",\n",
            "    \"state\": \"implementation\",\n",
            "    \"terminal\": false,\n",
            "    \"workflow_id\": \"work_sdlc\",\n",
            "    \"profile_id\": \"autopilot\",\n",
            "    \"updated_at\": \"{ts}\",\n",
            "    \"scope\": {{\n",
            "      \"volume\": 8,\n",
            "      \"scale\": \"fib_v1\",\n",
            "      \"reliability\": 44,\n",
            "      \"reliability_band\": \"medium\"\n",
            "    }}\n",
            "  }}\n",
            "}}\n"
        ),
        ts = ts,
    );
    let rel = write_head_event(&root, "scope-payload-idx.knot_head.json", &body);

    applier
        .apply_index_event(&rel)
        .expect("idx event with scope payload should apply");

    let record = db::get_knot_hot(&conn, "K-scope-payload")
        .expect("hot lookup should succeed")
        .expect("knot should be cached");
    assert_eq!(record.scope_data.volume, Some(8));
    assert_eq!(record.scope_data.scale.as_deref(), Some("fib_v1"));
    assert_eq!(record.scope_data.reliability, Some(44));
    assert_eq!(
        record.scope_data.reliability_band.as_deref(),
        Some("medium")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_index_event_without_scope_preserves_existing_cached_scope() {
    // Older heads (or heads written before this feature) omit `scope`. Apply
    // must NOT clobber a previously-cached scope value with default/empty.
    let root = setup_repo();
    let conn = open_conn(&root);
    db::set_meta(&conn, "hot_window_days", "365").expect("hot window should be configurable");
    let mut applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());

    let ts = recent_ts();
    // First, an idx event WITH scope establishes the cached value.
    let initial = format!(
        concat!(
            "{{\n",
            "  \"event_id\": \"019c942e-0002-7883-bb13-27197273b8f5\",\n",
            "  \"occurred_at\": \"{ts}\",\n",
            "  \"type\": \"idx.knot_head\",\n",
            "  \"data\": {{\n",
            "    \"knot_id\": \"K-scope-preserve\",\n",
            "    \"title\": \"Preserve cached\",\n",
            "    \"state\": \"implementation\",\n",
            "    \"terminal\": false,\n",
            "    \"workflow_id\": \"work_sdlc\",\n",
            "    \"profile_id\": \"autopilot\",\n",
            "    \"updated_at\": \"{ts}\",\n",
            "    \"scope\": {{ \"volume\": 5, \"scale\": \"fib_v1\" }}\n",
            "  }}\n",
            "}}\n"
        ),
        ts = ts,
    );
    let rel = write_head_event(&root, "scope-preserve-1-idx.knot_head.json", &initial);
    applier
        .apply_index_event(&rel)
        .expect("first idx event should apply");

    // Then a follow-up idx event WITHOUT scope (e.g., a state change written
    // by an older binary) must preserve the prior scope.
    let followup = format!(
        concat!(
            "{{\n",
            "  \"event_id\": \"019c942e-0003-7883-bb13-27197273b8f5\",\n",
            "  \"occurred_at\": \"{ts}\",\n",
            "  \"type\": \"idx.knot_head\",\n",
            "  \"data\": {{\n",
            "    \"knot_id\": \"K-scope-preserve\",\n",
            "    \"title\": \"Preserve cached\",\n",
            "    \"state\": \"ready_for_implementation_review\",\n",
            "    \"terminal\": false,\n",
            "    \"workflow_id\": \"work_sdlc\",\n",
            "    \"profile_id\": \"autopilot\",\n",
            "    \"updated_at\": \"{ts}\"\n",
            "  }}\n",
            "}}\n"
        ),
        ts = ts,
    );
    let rel = write_head_event(&root, "scope-preserve-2-idx.knot_head.json", &followup);
    applier
        .apply_index_event(&rel)
        .expect("second idx event should apply");

    let record = db::get_knot_hot(&conn, "K-scope-preserve")
        .expect("hot lookup should succeed")
        .expect("knot should be cached");
    assert_eq!(record.state, "ready_for_implementation_review");
    assert_eq!(record.scope_data.volume, Some(5));
    assert_eq!(record.scope_data.scale.as_deref(), Some("fib_v1"));

    let _ = std::fs::remove_dir_all(root);
}

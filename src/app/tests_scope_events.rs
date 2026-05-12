use serde_json::{json, Value};

use super::rehydrate_from_events;
use crate::domain::scope::{ScopeData, ScopeFloat};

fn unique_root(prefix: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("root should be creatable");
    root
}

fn write_event(root: &std::path::Path, filename: &str, body: &Value) {
    let path = root
        .join(".knots")
        .join("events")
        .join("2026")
        .join("02")
        .join("25")
        .join(filename);
    std::fs::create_dir_all(path.parent().expect("event parent should exist"))
        .expect("event parent should be creatable");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(body).expect("event serializes"),
    )
    .expect("event should be writable");
}

fn created_event() -> Value {
    json!({
        "event_id": "1000",
        "type": "knot.created",
        "occurred_at": "2026-02-25T10:00:00Z",
        "knot_id": "K-scope",
        "data": {
            "title": "Scoped",
            "state": "implementation",
            "workflow_id": "work_sdlc",
            "profile_id": "autopilot",
        },
    })
}

fn full_scope() -> ScopeData {
    ScopeData {
        volume: Some(8),
        scale: Some("fib_v1".to_string()),
        volume_score_confidence: Some(ScopeFloat::new(0.72).expect("finite")),
        volume_stddev: Some(ScopeFloat::new(1.25).expect("finite")),
        volume_result_id: Some("vol-1".to_string()),
        reliability: Some(44),
        reliability_score_confidence: Some(ScopeFloat::new(0.91).expect("finite")),
        reliability_stddev: Some(ScopeFloat::new(2.5).expect("finite")),
        reliability_band: Some("medium".to_string()),
        reliability_result_id: Some("rel-1".to_string()),
    }
}

fn scope_event(event_id: &str, occurred_at: &str, scope: &ScopeData) -> Value {
    json!({
        "event_id": event_id,
        "type": "knot.scope_set",
        "occurred_at": occurred_at,
        "knot_id": "K-scope",
        "data": serde_json::to_value(scope).expect("scope serializes"),
    })
}

#[test]
fn rehydrate_restores_full_scope_metadata_from_events() {
    let root = unique_root("knots-rehydrate-scope-full");
    write_event(&root, "1000-knot.created.json", &created_event());
    let scope = full_scope();
    write_event(
        &root,
        "1100-knot.scope_set.json",
        &scope_event("1100", "2026-02-25T10:10:00Z", &scope),
    );

    let projection = rehydrate_from_events(
        &[root.join(".knots").as_path()],
        "K-scope",
        "Scoped".to_string(),
        "implementation".to_string(),
        "2026-02-25T10:00:00Z".to_string(),
    )
    .expect("rehydrate should succeed");

    assert_eq!(projection.scope_data, scope);
    assert_eq!(projection.updated_at, "2026-02-25T10:10:00Z");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rehydrate_merges_partial_scope_event_into_prior_scope() {
    let root = unique_root("knots-rehydrate-scope-partial");
    write_event(&root, "1000-knot.created.json", &created_event());
    let initial = full_scope();
    write_event(
        &root,
        "1100-knot.scope_set.json",
        &scope_event("1100", "2026-02-25T10:10:00Z", &initial),
    );

    // The emission side pre-merges patches into ScopeData before writing,
    // so the second event carries the FULL merged state. Replay's
    // replace-semantics must reproduce that exact merged state.
    let merged = ScopeData {
        volume: Some(13),
        reliability: Some(55),
        reliability_band: Some("high".to_string()),
        ..initial.clone()
    };
    write_event(
        &root,
        "1200-knot.scope_set.json",
        &scope_event("1200", "2026-02-25T11:00:00Z", &merged),
    );

    let projection = rehydrate_from_events(
        &[root.join(".knots").as_path()],
        "K-scope",
        "Scoped".to_string(),
        "implementation".to_string(),
        "2026-02-25T10:00:00Z".to_string(),
    )
    .expect("rehydrate should succeed");

    assert_eq!(projection.scope_data, merged);
    assert_eq!(projection.scope_data.volume, Some(13));
    assert_eq!(projection.scope_data.reliability, Some(55));
    assert_eq!(
        projection.scope_data.reliability_band.as_deref(),
        Some("high")
    );
    assert_eq!(projection.scope_data.scale.as_deref(), Some("fib_v1"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rehydrate_preserves_field_absence_through_round_trip() {
    let root = unique_root("knots-rehydrate-scope-absence");
    write_event(&root, "1000-knot.created.json", &created_event());
    let sparse = ScopeData {
        volume: Some(3),
        reliability_band: Some("low".to_string()),
        ..ScopeData::default()
    };
    let payload = serde_json::to_value(&sparse).expect("sparse scope serializes");
    let payload_object = payload
        .as_object()
        .expect("sparse scope payload is an object");
    assert!(!payload_object.contains_key("scale"));
    assert!(!payload_object.contains_key("reliability"));

    write_event(
        &root,
        "1100-knot.scope_set.json",
        &scope_event("1100", "2026-02-25T10:10:00Z", &sparse),
    );

    let projection = rehydrate_from_events(
        &[root.join(".knots").as_path()],
        "K-scope",
        "Scoped".to_string(),
        "implementation".to_string(),
        "2026-02-25T10:00:00Z".to_string(),
    )
    .expect("rehydrate should succeed");

    assert_eq!(projection.scope_data, sparse);
    assert!(projection.scope_data.scale.is_none());
    assert!(projection.scope_data.volume_score_confidence.is_none());
    assert!(projection.scope_data.reliability.is_none());
    assert!(projection.scope_data.reliability_result_id.is_none());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rehydrate_without_scope_events_yields_default_scope() {
    let root = unique_root("knots-rehydrate-scope-none");
    write_event(&root, "1000-knot.created.json", &created_event());

    let projection = rehydrate_from_events(
        &[root.join(".knots").as_path()],
        "K-scope",
        "Scoped".to_string(),
        "implementation".to_string(),
        "2026-02-25T10:00:00Z".to_string(),
    )
    .expect("rehydrate should succeed");

    assert_eq!(projection.scope_data, ScopeData::default());
    assert!(projection.scope_data.is_empty());

    let _ = std::fs::remove_dir_all(root);
}

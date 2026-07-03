//! Full-event metadata application, split from apply_tests_local_files for size.

use serde_json::json;

use crate::db::{self};
use crate::sync::GitAdapter;

use super::tests_local_files::{open_conn, seed_hot_knot, setup_repo, write_json};
use super::FullApplyOutcome;
use super::IncrementalApplier;
use std::path::Path;

#[test]
fn apply_full_event_updates_description_acceptance_lease_id_and_unknown_events() {
    let root = setup_repo();
    let conn = open_conn(&root);
    seed_hot_knot(&conn, "K-meta");
    let applier = IncrementalApplier::new_with_builtins(&conn, root.clone(), GitAdapter::new());
    let events_dir = root.join(".knots/events/2026/02/25");

    let events = [
        (
            "3000-knot.description_set.json",
            json!({"description": "  synced body  "}),
            "knot.description_set",
        ),
        (
            "3001-knot.acceptance_set.json",
            json!({"acceptance": "  accepted  "}),
            "knot.acceptance_set",
        ),
        (
            "3002-knot.lease_id_set.json",
            json!({"lease_id": "lease-1"}),
            "knot.lease_id_set",
        ),
        (
            "3003-knot.tag_add.json",
            json!({"tag": "   "}),
            "knot.tag_add",
        ),
        (
            "3004-knot.unknown.json",
            json!({"ignored": true}),
            "knot.unknown",
        ),
    ];

    for (index, (filename, data, event_type)) in events.into_iter().enumerate() {
        write_json(
            &events_dir.join(filename),
            json!({
                "event_id": format!("300{index}"),
                "occurred_at": "2026-02-25T10:00:00Z",
                "knot_id": "K-meta",
                "type": event_type,
                "data": data
            }),
        );
        let outcome = applier
            .apply_full_event(
                Path::new(".knots/events/2026/02/25")
                    .join(filename)
                    .as_path(),
            )
            .expect("metadata event should apply");
        assert!(matches!(outcome, FullApplyOutcome::Ignored));
    }

    let updated = db::get_knot_hot(&conn, "K-meta")
        .expect("hot lookup should succeed")
        .expect("hot knot should still exist");
    assert_eq!(updated.description.as_deref(), Some("synced body"));
    assert_eq!(updated.body.as_deref(), Some("synced body"));
    assert_eq!(updated.acceptance.as_deref(), Some("accepted"));
    assert_eq!(updated.lease_id.as_deref(), Some("lease-1"));
    assert_eq!(updated.tags, vec!["alpha".to_string()]);

    let _ = std::fs::remove_dir_all(root);
}

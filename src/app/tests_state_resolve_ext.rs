use super::tests_coverage_ext::{open_app, unique_workspace};
use super::{AppError, StateActorMetadata};
use crate::db::{self, UpsertKnotHot};

fn seed_state(app: &super::App, id: &str, state: &str) {
    db::upsert_knot_hot(
        app.conn_for_test(),
        &UpsertKnotHot {
            id,
            title: "Seed",
            state,
            updated_at: "2026-05-01T00:00:00Z",
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
            created_at: Some("2026-05-01T00:00:00Z"),
        },
    )
    .expect("seed knot should upsert");
}

#[test]
fn terminal_resolution_helper_reports_only_new_terminal_transitions() {
    let root = unique_workspace();
    let (app, _) = open_app(&root);

    let active = app
        .create_knot("active", None, Some("implementation"), Some("default"))
        .expect("active knot should be created");
    let active_record = db::get_knot_hot(app.conn_for_test(), &active.id)
        .expect("active lookup should succeed")
        .expect("active record should exist");
    let shipped = app
        .set_state(&active.id, "shipped", true, active.profile_etag.as_deref())
        .expect("active knot should ship");
    let shipped_record = db::get_knot_hot(app.conn_for_test(), &shipped.id)
        .expect("shipped lookup should succeed")
        .expect("shipped record should exist");

    assert!(app
        .transitioned_to_terminal_resolution_state(&active_record, &shipped_record)
        .expect("terminal transition helper should succeed"));
    assert!(!app
        .transitioned_to_terminal_resolution_state(&shipped_record, &shipped_record)
        .expect("same terminal state should not count as a transition"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn set_state_reports_missing_resume_provenance() {
    let root = unique_workspace();
    let (app, _) = open_app(&root);
    seed_state(&app, "K-deferred-missing", "deferred");
    seed_state(&app, "K-blocked-missing", "blocked");

    for (id, expected) in [
        ("K-deferred-missing", "deferred_from_state provenance"),
        ("K-blocked-missing", "blocked_from_state provenance"),
    ] {
        let err = app
            .set_state(id, "implementation", false, Some("seed-etag"))
            .expect_err("missing provenance should reject resume");
        assert!(matches!(err, AppError::InvalidArgument(_)));
        assert!(err.to_string().contains(expected));
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn set_state_rejects_resume_to_wrong_provenance_state() {
    let root = unique_workspace();
    let (app, _) = open_app(&root);
    let created = app
        .create_knot(
            "Blocked wrong target",
            None,
            Some("planning"),
            Some("default"),
        )
        .expect("knot should create");
    let blocked = app
        .set_state(
            &created.id,
            "blocked",
            false,
            created.profile_etag.as_deref(),
        )
        .expect("knot should block");

    let err = app
        .set_state(
            &blocked.id,
            "implementation",
            false,
            blocked.profile_etag.as_deref(),
        )
        .expect_err("blocked knot should only resume to recorded state");
    assert!(matches!(err, AppError::InvalidArgument(_)));
    assert!(err.to_string().contains("blocked knots may only resume"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resume_blocked_dependents_waits_for_all_blockers_then_restores_provenance_state() {
    let root = unique_workspace();
    let (app, _) = open_app(&root);

    let shipped_blocker = app
        .create_knot("shipped blocker", None, Some("shipped"), Some("default"))
        .expect("shipped blocker should be created");
    let pending_blocker = app
        .create_knot(
            "pending blocker",
            None,
            Some("implementation"),
            Some("default"),
        )
        .expect("pending blocker should be created");
    let dependent = app
        .create_knot("blocked dependent", None, Some("planning"), Some("default"))
        .expect("dependent should be created");
    app.set_state(
        &dependent.id,
        "blocked",
        false,
        dependent.profile_etag.as_deref(),
    )
    .expect("dependent should enter blocked");
    app.add_edge(&dependent.id, "blocked_by", &shipped_blocker.id)
        .expect("first blocker edge should be added");
    app.add_edge(&dependent.id, "blocked_by", &pending_blocker.id)
        .expect("second blocker edge should be added");

    app.resume_blocked_dependents_locked(&shipped_blocker.id, &StateActorMetadata::default())
        .expect("resume check should no-op while one blocker is active");
    assert_eq!(
        app.show_knot(&dependent.id).unwrap().unwrap().state,
        "blocked"
    );

    app.set_state(
        &pending_blocker.id,
        "shipped",
        true,
        pending_blocker.profile_etag.as_deref(),
    )
    .expect("shipping final blocker should resume dependent");
    let resumed = app.show_knot(&dependent.id).unwrap().unwrap();
    assert_eq!(resumed.state, "ready_for_planning");
    assert_eq!(resumed.blocked_from_state, None);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resume_blocked_dependents_ignores_missing_or_active_dependents_and_errors_without_provenance() {
    let root = unique_workspace();
    let (app, _) = open_app(&root);

    let blocker = app
        .create_knot("blocker", None, Some("shipped"), Some("default"))
        .expect("blocker should be created");
    let active = app
        .create_knot("active dependent", None, Some("planning"), Some("default"))
        .expect("active dependent should be created");
    let no_provenance = app
        .create_knot(
            "blocked without provenance",
            None,
            Some("blocked"),
            Some("default"),
        )
        .expect("blocked fixture should be created");

    db::insert_edge(
        app.conn_for_test(),
        "missing-dependent",
        "blocked_by",
        &blocker.id,
    )
    .expect("missing dependent edge should insert");
    app.add_edge(&active.id, "blocked_by", &blocker.id)
        .expect("active dependent edge should be added");
    app.add_edge(&no_provenance.id, "blocked_by", &blocker.id)
        .expect("blocked dependent edge should be added");

    let err = app
        .resume_blocked_dependents_locked(&blocker.id, &StateActorMetadata::default())
        .expect_err("blocked dependent without provenance should fail");
    assert!(
        matches!(err, AppError::InvalidArgument(message) if message.contains(
            "blocked_from_state"
        ))
    );
    assert_eq!(
        app.show_knot(&active.id).unwrap().unwrap().state,
        "planning"
    );

    let _ = std::fs::remove_dir_all(root);
}

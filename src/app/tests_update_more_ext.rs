use super::{App, AppError, StateActorMetadata, UpdateKnotPatch};
use crate::db::{self, UpsertKnotHot};
use crate::domain::knot_type::KnotType;

fn unique_workspace() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("knots-update-more-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn empty_patch() -> UpdateKnotPatch {
    UpdateKnotPatch {
        title: None,
        description: None,
        acceptance: None,
        priority: None,
        status: None,
        knot_type: None,
        add_tags: vec![],
        remove_tags: vec![],
        add_invariants: vec![],
        remove_invariants: vec![],
        clear_invariants: false,
        add_verification_steps: vec![],
        remove_verification_steps: vec![],
        clear_verification_steps: false,
        gate_owner_kind: None,
        gate_failure_modes: None,
        clear_gate_failure_modes: false,
        execution_plan_objective: None,
        execution_plan_data: None,
        add_note: None,
        add_handoff_capsule: None,
        expected_profile_etag: None,
        force: false,
        state_actor: StateActorMetadata::default(),
    }
}

fn open_app(root: &std::path::Path) -> App {
    let db_path = root.join(".knots/cache/state.sqlite");
    App::open(
        db_path.to_str().expect("db path should be utf8"),
        root.to_path_buf(),
    )
    .expect("app should open")
}

fn seed_state(app: &App, id: &str, state: &str) {
    db::upsert_knot_hot(
        app.conn_for_test(),
        &UpsertKnotHot {
            id,
            title: "Seed",
            state,
            updated_at: "2026-04-05T00:00:00Z",
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
            created_at: Some("2026-04-05T00:00:00Z"),
        },
    )
    .expect("seed knot should upsert");
}

#[test]
fn update_status_reports_missing_resume_provenance() {
    let root = unique_workspace();
    let app = open_app(&root);
    seed_state(&app, "K-deferred-missing", "deferred");
    seed_state(&app, "K-blocked-missing", "blocked");

    for (id, expected) in [
        ("K-deferred-missing", "deferred_from_state provenance"),
        ("K-blocked-missing", "blocked_from_state provenance"),
    ] {
        let err = app
            .update_knot(
                id,
                UpdateKnotPatch {
                    status: Some("implementation".to_string()),
                    ..empty_patch()
                },
            )
            .expect_err("missing provenance should reject resume");
        assert!(matches!(err, AppError::InvalidArgument(_)));
        assert!(err.to_string().contains(expected));
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn update_type_switches_to_default_profile_and_initial_state() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot("Type switch", None, Some("implementation"), Some("default"))
        .expect("knot should create");

    let updated = app
        .update_knot(
            &created.id,
            UpdateKnotPatch {
                knot_type: Some(KnotType::Gate),
                ..empty_patch()
            },
        )
        .expect("type change should apply");

    assert_eq!(updated.knot_type, KnotType::Gate);
    assert_eq!(updated.workflow_id, "gate_sdlc");
    assert_eq!(updated.profile_id, "evaluate");
    assert_eq!(updated.state, "ready_to_evaluate");

    let _ = std::fs::remove_dir_all(root);
}

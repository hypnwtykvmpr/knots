use super::execute_operation;
use super::tests_lease_ext::{open_app, setup_repo, unique_workspace};
use crate::app::{App, AppError, StateActorMetadata};
use crate::write_queue::{RollbackOperation, UpdateOperation, WriteOperation};

fn base_update(id: &str) -> UpdateOperation {
    UpdateOperation {
        id: id.to_string(),
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
        gate_failure_modes: vec![],
        clear_gate_failure_modes: false,
        scope: crate::cli_scope::ScopeArgs::default(),
        execution_plan_file: None,
        objective: None,
        add_note: None,
        note_username: None,
        note_datetime: None,
        note_agentname: None,
        note_model: None,
        note_version: None,
        add_handoff_capsule: None,
        handoff_username: None,
        handoff_datetime: None,
        handoff_agentname: None,
        handoff_model: None,
        handoff_version: None,
        if_match: None,
        actor_kind: None,
        agent_name: None,
        agent_model: None,
        agent_version: None,
        force: false,
        approve_terminal_cascade: false,
        lease_id: None,
        json: false,
    }
}

fn create_work(app: &App) -> crate::app::KnotView {
    app.create_knot(
        "Dispatch coverage",
        None,
        Some("ready_for_implementation"),
        None,
    )
    .expect("work knot should be created")
}

#[test]
fn execute_update_rejects_noop_and_returns_json_for_real_update() {
    let root = unique_workspace();
    setup_repo(&root);
    let app = open_app(&root);
    let knot = create_work(&app);

    let err = execute_operation(&app, &WriteOperation::Update(base_update(&knot.id)))
        .expect_err("no-op update should fail");
    match err {
        AppError::InvalidArgument(msg) => {
            assert!(msg.contains("update requires at least one field change"));
        }
        other => panic!("unexpected update error: {other}"),
    }

    let mut update = base_update(&knot.id);
    update.title = Some("Updated title".to_string());
    update.json = true;
    let output = execute_operation(&app, &WriteOperation::Update(update))
        .expect("json update should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["title"], "Updated title");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn execute_update_reports_invalid_invariants_and_execution_plan_file_errors() {
    let root = unique_workspace();
    setup_repo(&root);
    let app = open_app(&root);
    let knot = create_work(&app);

    let mut add_bad_invariant = base_update(&knot.id);
    add_bad_invariant.add_invariants = vec!["invalid".to_string()];
    let err = execute_operation(&app, &WriteOperation::Update(add_bad_invariant))
        .expect_err("bad invariant should fail");
    assert!(err
        .to_string()
        .contains("expected '<Scope|State>:<condition>'"));

    let mut remove_bad_invariant = base_update(&knot.id);
    remove_bad_invariant.remove_invariants = vec!["also-invalid".to_string()];
    let err = execute_operation(&app, &WriteOperation::Update(remove_bad_invariant))
        .expect_err("bad removed invariant should fail");
    assert!(err
        .to_string()
        .contains("expected '<Scope|State>:<condition>'"));

    let mut missing_plan = base_update(&knot.id);
    missing_plan.execution_plan_file = Some(root.join("missing-plan.json").display().to_string());
    let err = execute_operation(&app, &WriteOperation::Update(missing_plan))
        .expect_err("missing execution plan should fail");
    assert!(err
        .to_string()
        .contains("failed to read execution plan file"));

    let invalid_plan_path = root.join("invalid-plan.json");
    std::fs::write(&invalid_plan_path, "{bad json").expect("invalid plan should write");
    let mut invalid_plan = base_update(&knot.id);
    invalid_plan.execution_plan_file = Some(invalid_plan_path.display().to_string());
    let err = execute_operation(&app, &WriteOperation::Update(invalid_plan))
        .expect_err("invalid execution plan should fail");
    assert!(err.to_string().contains("invalid execution plan JSON"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn execute_rollback_json_paths_parse() {
    let root = unique_workspace();
    setup_repo(&root);
    let app = open_app(&root);
    let knot = create_work(&app);
    let claimed = app
        .set_state_with_actor(
            &knot.id,
            "implementation",
            false,
            knot.profile_etag.as_deref(),
            StateActorMetadata::default(),
        )
        .expect("claiming action state should succeed");

    let dry_run = execute_operation(
        &app,
        &WriteOperation::Rollback(RollbackOperation {
            id: claimed.id.clone(),
            dry_run: true,
            actor_kind: None,
            agent_name: None,
            agent_model: None,
            agent_version: None,
            json: true,
        }),
    )
    .expect("json dry-run rollback should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&dry_run).expect("valid JSON");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["target_state"], "ready_for_implementation");

    let real = execute_operation(
        &app,
        &WriteOperation::Rollback(RollbackOperation {
            id: claimed.id.clone(),
            dry_run: false,
            actor_kind: Some("agent".to_string()),
            agent_name: None,
            agent_model: None,
            agent_version: None,
            json: true,
        }),
    )
    .expect("json rollback should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&real).expect("valid JSON");
    assert_eq!(parsed["dry_run"], false);
    assert_eq!(parsed["state"], "ready_for_implementation");

    let _ = std::fs::remove_dir_all(root);
}

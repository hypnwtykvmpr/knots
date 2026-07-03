use crate::app::AppError;
use crate::domain::execution_plan_edit::CascadeInfo;
use crate::domain::gate::GateOwnerKind;
use crate::domain::knot_type::KnotType;
use crate::state_hierarchy::HierarchyKnot;

use super::helpers;

#[test]
fn terminal_cascade_executor_uses_preapproval_and_propagates_other_errors() {
    let mut calls = 0;
    let value = helpers::execute_with_terminal_cascade_prompt(true, |approved| {
        calls += 1;
        assert!(approved);
        Ok::<_, AppError>("done")
    })
    .expect("preapproved action should run");
    assert_eq!(value, "done");
    assert_eq!(calls, 1);

    let err = helpers::execute_with_terminal_cascade_prompt(false, |_| -> Result<(), AppError> {
        Err(AppError::InvalidArgument("plain failure".to_string()))
    })
    .expect_err("ordinary errors should propagate");
    assert!(err.to_string().contains("plain failure"));

    let descendants = vec![HierarchyKnot {
        id: "knots-child".to_string(),
        state: "ready_for_review".to_string(),
        deferred_from_state: None,
        blocked_from_state: None,
    }];
    let err = helpers::execute_with_terminal_cascade_prompt(false, |_| {
        Err::<(), _>(AppError::TerminalCascadeApprovalRequired {
            knot_id: "knots-parent".to_string(),
            target_state: "shipped".to_string(),
            descendants: descendants.clone(),
        })
    })
    .expect_err("non-interactive approval should be returned to caller");
    assert!(matches!(
        err,
        AppError::TerminalCascadeApprovalRequired { .. }
    ));
}

#[test]
fn parse_helpers_cover_aliases_defaults_and_errors() {
    assert!(matches!(
        helpers::parse_gate_decision("PASS").expect("pass should parse"),
        crate::app::GateDecision::Yes
    ));
    assert!(matches!(
        helpers::parse_gate_decision(" fail ").expect("fail should parse"),
        crate::app::GateDecision::No
    ));

    assert_eq!(
        helpers::parse_knot_type_arg(Some("gate")).expect("gate type should parse"),
        KnotType::Gate
    );
    assert_eq!(
        helpers::parse_gate_owner_kind_arg(None).expect("missing owner should parse"),
        None
    );
    assert_eq!(
        helpers::parse_gate_owner_kind_arg(Some("")).expect("blank owner defaults"),
        Some(GateOwnerKind::Agent)
    );

    let raw_modes = vec![
        "release blocked=knots-1".to_string(),
        "review failed=knots-2,knots-3".to_string(),
    ];
    let modes = helpers::parse_gate_failure_modes_option(&raw_modes)
        .expect("failure modes should parse")
        .expect("failure modes should exist");
    assert_eq!(
        modes.get("release blocked").unwrap(),
        &vec!["knots-1".to_string()]
    );
    assert_eq!(
        modes.get("review failed").unwrap(),
        &vec!["knots-2".to_string(), "knots-3".to_string()]
    );

    let gate = helpers::parse_gate_data_args(None, &[], KnotType::Gate)
        .expect("empty gate data should parse");
    assert_eq!(gate.owner_kind, GateOwnerKind::Agent);
    assert!(gate.failure_modes.is_empty());

    let err = helpers::parse_gate_data_args(None, &raw_modes, KnotType::Work)
        .expect_err("work knots should reject failure modes");
    assert!(err.to_string().contains("require knot type 'gate'"));
}

#[test]
fn format_json_pretty_prints_and_trails_newline() {
    let rendered = helpers::format_json(&serde_json::json!({"state": "ready"}));
    assert!(rendered.contains("\"state\": \"ready\""));
    assert!(rendered.ends_with('\n'));
}

#[test]
fn prompt_helpers_render_context_and_parse_answers() {
    let descendants = vec![HierarchyKnot {
        id: "knots-child".to_string(),
        state: "ready_for_shipment".to_string(),
        deferred_from_state: None,
        blocked_from_state: None,
    }];
    let mut output = Vec::new();
    let mut input = std::io::Cursor::new(b"yes\n".to_vec());

    assert!(helpers::terminal_cascade_prompt(
        &mut output,
        &mut input,
        "knots-parent",
        "shipped",
        &descendants,
    )
    .expect("prompt should parse yes"));
    let rendered = String::from_utf8(output).expect("prompt should be utf8");
    assert!(rendered.contains("knots-parent"));
    assert!(rendered.contains("knots-child"));

    let cascade = CascadeInfo {
        affected_knot_ids: vec!["knots-a".to_string(), "knots-b".to_string()],
        step_count: 3,
    };
    let mut output = Vec::new();
    let mut input = std::io::Cursor::new(b"n\n".to_vec());
    assert!(
        !helpers::plan_cascade_prompt(&mut output, &mut input, "removing wave", &cascade)
            .expect("prompt should parse no")
    );
    let rendered = String::from_utf8(output).expect("prompt should be utf8");
    assert!(rendered.contains("3 step(s)"));
    assert!(rendered.contains("knots-a, knots-b"));
}

#[test]
fn lease_metadata_helpers_cover_flags_actor_and_validation() {
    assert!(helpers::supplied_agent_flag_names(None, None, None).is_empty());
    assert_eq!(
        helpers::supplied_agent_flag_names(Some("name"), None, Some("version")),
        vec!["agent-name", "agent-version"]
    );
    assert_eq!(
        helpers::supplied_agent_flag_names(Some("name"), Some("model"), Some("version")),
        vec!["agent-name", "agent-model", "agent-version"]
    );

    let info = crate::domain::lease::AgentInfo {
        agent_type: "implementation".to_string(),
        provider: "openai".to_string(),
        agent_name: "Codex".to_string(),
        model: "gpt-5".to_string(),
        model_version: "".to_string(),
    };
    let actor = helpers::state_actor_from_agent_info(Some("agent".to_string()), Some(&info));
    assert_eq!(actor.agent_name.as_deref(), Some("Codex"));
    assert_eq!(actor.agent_model.as_deref(), Some("gpt-5"));
    assert_eq!(actor.agent_version, None);

    let mut knot = minimal_knot_view();
    assert!(helpers::validate_non_claim_lease(&knot, None).is_ok());
    knot.lease_id = Some("lease-1".to_string());
    assert!(helpers::validate_non_claim_lease(&knot, Some("lease-1")).is_ok());
    let err = helpers::validate_non_claim_lease(&knot, Some("lease-2"))
        .expect_err("mismatched lease should fail");
    assert!(err.to_string().contains("lease mismatch"));

    knot.lease_id = None;
    let err = helpers::validate_non_claim_lease(&knot, Some("lease-2"))
        .expect_err("claim-only binding should fail");
    assert!(err.to_string().contains("lease binding is only allowed"));
}

#[test]
fn gate_failure_modes_empty_input_returns_none() {
    assert!(helpers::parse_gate_failure_modes_option(&[])
        .expect("empty failure modes should parse")
        .is_none());
}

fn minimal_knot_view() -> crate::app::KnotView {
    crate::app::KnotView {
        id: "knots-1".to_string(),
        alias: None,
        title: "title".to_string(),
        state: "ready_for_implementation".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        body: None,
        description: None,
        acceptance: None,
        priority: None,
        knot_type: KnotType::Work,
        tags: Vec::new(),
        notes: Vec::new(),
        handoff_capsules: Vec::new(),
        invariants: Vec::new(),
        verification_steps: Vec::new(),
        step_history: Vec::new(),
        gate: None,
        lease: None,
        execution_plan: None,
        scope: None,
        lease_id: None,
        lease_expiry_ts: 0,
        lease_agent: None,
        workflow_id: "sdlc".to_string(),
        profile_id: "sdlc/default".to_string(),
        profile_etag: None,
        deferred_from_state: None,
        blocked_from_state: None,
        created_at: None,
        step_metadata: None,
        next_step_metadata: None,
        edges: Vec::new(),
        child_summaries: Vec::new(),
    }
}

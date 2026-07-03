use super::*;

#[test]
fn parse_owner_defaults_to_agent() {
    assert_eq!(parse_owner_filter(None), OwnerKind::Agent);
    assert_eq!(parse_owner_filter(Some("")), OwnerKind::Agent);
    assert_eq!(parse_owner_filter(Some("agent")), OwnerKind::Agent);
}

#[test]
fn parse_owner_recognizes_human() {
    assert_eq!(parse_owner_filter(Some("human")), OwnerKind::Human);
    assert_eq!(parse_owner_filter(Some("Human")), OwnerKind::Human);
}

#[test]
fn normalize_ready_type_none_returns_none() {
    assert_eq!(normalize_ready_type(None), None);
}

#[test]
fn normalize_ready_type_empty_returns_none() {
    assert_eq!(normalize_ready_type(Some("")), None);
    assert_eq!(normalize_ready_type(Some("  ")), None);
}

#[test]
fn normalize_ready_type_strips_prefix() {
    assert_eq!(
        normalize_ready_type(Some("ready_for_planning")),
        Some("planning".to_string())
    );
}

#[test]
fn normalize_ready_type_passes_through_stage() {
    assert_eq!(normalize_ready_type(Some("plan")), Some("plan".to_string()));
    assert_eq!(
        normalize_ready_type(Some("implementation")),
        Some("implementation".to_string())
    );
}

#[test]
fn normalize_ready_type_lowercases_and_replaces_dashes() {
    assert_eq!(
        normalize_ready_type(Some("Plan-Review")),
        Some("plan_review".to_string())
    );
}

#[test]
fn normalize_ready_type_maps_ready_to_evaluate_to_stage() {
    assert_eq!(
        normalize_ready_type(Some("ready-to-evaluate")),
        Some("evaluate".to_string())
    );
}

#[test]
fn queue_stage_matches_direct_state_and_rejects_unknown_profiles() {
    let registry = ProfileRegistry::load().expect("registry should load");
    let mut knot = sample_poll_knot();
    knot.state = "ready_for_implementation".to_string();

    assert!(ready::queue_stage_matches(
        &registry,
        &knot,
        "ready_for_implementation"
    ));

    knot.profile_id = "missing_profile".to_string();
    assert!(!ready::queue_stage_matches(&registry, &knot, "missing"));
}

#[test]
fn ready_labels_fall_back_to_workflow_metadata() {
    let registry = ProfileRegistry::load().expect("registry should load");
    let mut knot = sample_poll_knot();
    knot.state = "ready_for_implementation".to_string();
    knot.next_step_metadata = None;

    assert_eq!(
        ready::ready_action_label(&registry, &knot),
        "implementation"
    );
    assert_eq!(ready::ready_owner_label(&registry, &knot), "agent");
}

#[test]
fn completion_command_omits_deprecated_agent_metadata_flags() {
    let cmd = completion_command("knots-27ef", "implementation", None, false);
    assert_eq!(
        cmd,
        "kno next knots-27ef --expected-state implementation --actor-kind agent"
    );
    assert!(!cmd.contains("--agent-name"));
    assert!(!cmd.contains("--agent-model"));
    assert!(!cmd.contains("--agent-version"));
}

#[test]
fn completion_command_does_not_emit_e2e_flag_on_next() {
    // `kno next` has no --e2e flag; it is only meaningful on `kno claim`
    // and `kno poll`. The boundary text (not the completion command) is
    // responsible for telling agents to re-claim with --e2e.
    let cmd = completion_command("knots-27ef", "implementation", None, true);
    assert!(!cmd.contains("--e2e"));
}

#[test]
fn completion_command_includes_bound_lease_when_present() {
    let cmd = completion_command("knots-27ef", "implementation", Some("lease-1"), false);
    assert_eq!(
        cmd,
        "kno next knots-27ef --expected-state implementation --lease lease-1 --actor-kind agent"
    );
}

#[test]
fn render_helpers_delegate_to_prompt_renderers() {
    let result = PollResult {
        knot: sample_poll_knot(),
        skill: "Use the implementation skill.".to_string(),
        completion_cmd: "kno next knots-27ef --expected-state implementation".to_string(),
        e2e: true,
    };

    let text = render_text(&result);
    assert!(text.contains("# Claimed knot"));
    assert!(text.contains("Use the implementation skill."));
    assert!(text.contains("e2e_continuation"));

    let verbose_text = render_text_verbose(&result, true);
    assert!(verbose_text.contains("## Completion"));

    let json = render_json(&result);
    assert_eq!(json["id"], "knots-27ef");
    assert_eq!(json["workflow_boundary_kind"], "e2e_continuation");

    let verbose_json = render_json_verbose(&result, true);
    assert!(verbose_json["prompt"]
        .as_str()
        .unwrap()
        .contains("Claimed knot"));
}

#[test]
fn match_pollable_skips_terminal_state_without_next_action() {
    let registry = ProfileRegistry::load().expect("registry should load");
    let mut knot = sample_poll_knot();
    knot.state = "shipped".to_string();

    let result = match_pollable(&knot, &registry, &OwnerKind::Agent, false)
        .expect("terminal state should be valid but not pollable");

    assert!(result.is_none());
}

#[test]
fn require_queue_state_reports_unknown_state() {
    let registry = ProfileRegistry::load().expect("registry should load");
    let mut knot = sample_poll_knot();
    knot.state = "not_a_real_state".to_string();

    let err = require_queue_state(&registry, &knot)
        .expect_err("unknown state should reject queue validation");

    assert!(err.to_string().contains("not a claimable queue state"));
}

#[test]
fn prompt_body_for_state_rejects_non_prompt_state() {
    let registry = ProfileRegistry::load().expect("registry should load");
    let err = prompt_body_for_state(&registry, "autopilot", "ready_for_implementation")
        .expect_err("queue state should not render as action prompt");

    assert!(err
        .to_string()
        .contains("not an action state with a prompt"));
}

fn sample_poll_knot() -> crate::app::KnotView {
    crate::app::KnotView {
        id: "knots-27ef".to_string(),
        alias: Some("root.1".to_string()),
        title: "Claimed knot".to_string(),
        state: "implementation".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
        body: Some("Context body".to_string()),
        description: None,
        acceptance: Some("Acceptance".to_string()),
        priority: Some(1),
        knot_type: crate::domain::knot_type::KnotType::Work,
        tags: vec![],
        notes: vec![],
        handoff_capsules: vec![],
        invariants: vec![],
        verification_steps: vec![],
        step_history: vec![],
        gate: None,
        lease: None,
        execution_plan: None,
        scope: None,
        lease_id: Some("lease-1".to_string()),
        lease_expiry_ts: 0,
        lease_agent: None,
        workflow_id: "work_sdlc".to_string(),
        profile_id: "autopilot".to_string(),
        profile_etag: None,
        deferred_from_state: None,
        blocked_from_state: None,
        created_at: None,
        step_metadata: None,
        next_step_metadata: None,
        edges: vec![],
        child_summaries: vec![],
    }
}

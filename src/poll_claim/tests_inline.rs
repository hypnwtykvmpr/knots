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

use serde_json::json;

use super::{apply_rehydrate_event, RehydrateProjection};
use crate::domain::gate::GateOwnerKind;
use crate::domain::knot_type::KnotType;
use crate::events::{FullEvent, FullEventKind};

fn seed_projection() -> RehydrateProjection {
    RehydrateProjection {
        title: "seed".to_string(),
        state: "idea".to_string(),
        updated_at: "2026-02-25T10:00:00Z".to_string(),
        body: None,
        description: None,
        acceptance: None,
        priority: None,
        knot_type: KnotType::default(),
        tags: Vec::new(),
        notes: Vec::new(),
        handoff_capsules: Vec::new(),
        invariants: Vec::new(),
        verification_steps: Vec::new(),
        step_history: Vec::new(),
        gate_data: crate::domain::gate::GateData::default(),
        lease_data: crate::domain::lease::LeaseData::default(),
        execution_plan_data: crate::domain::execution_plan::ExecutionPlanData::default(),
        execution_plan_data_from_full: false,
        scope_data: crate::domain::scope::ScopeData::default(),
        lease_id: None,
        workflow_id: String::new(),
        profile_id: String::new(),
        profile_etag: None,
        deferred_from_state: None,
        blocked_from_state: None,
        created_at: None,
    }
}

fn event(kind: FullEventKind, data: serde_json::Value) -> FullEvent {
    FullEvent::with_identity(
        format!("event-{}", kind.as_str()),
        "2026-02-25T10:01:00Z",
        "K-1",
        kind.as_str(),
        data,
    )
}

#[test]
fn created_event_applies_optional_and_legacy_fields() {
    let mut projection = seed_projection();
    let created = event(
        FullEventKind::KnotCreated,
        json!({
            "title": "Created",
            "state": "planning",
            "workflow_id": " compatibility ",
            "profile_id": " Autopilot ",
            "deferred_from_state": "ready_for_planning",
            "blocked_from_state": "implementation",
            "invariants": [{"type": "Scope", "condition": "volume <= 5"}],
            "verification_steps": ["cargo test"],
            "type": "gate",
            "gate": {
                "owner_kind": "human",
                "failure_modes": {"scope": ["K-2"]}
            },
            "execution_plan": {
                "objective": "Coordinate rollout",
                "summary": "Use the stored plan"
            },
            "body": " legacy description "
        }),
    );

    apply_rehydrate_event(&mut projection, &created);

    assert_eq!(projection.title, "Created");
    assert_eq!(projection.state, "planning");
    assert_eq!(projection.workflow_id, "compatibility");
    assert_eq!(projection.profile_id, "autopilot");
    assert_eq!(
        projection.deferred_from_state.as_deref(),
        Some("ready_for_planning")
    );
    assert_eq!(
        projection.blocked_from_state.as_deref(),
        Some("implementation")
    );
    assert_eq!(projection.invariants[0].condition, "volume <= 5");
    assert_eq!(
        projection.verification_steps,
        vec!["cargo test".to_string()]
    );
    assert_eq!(projection.knot_type, KnotType::Gate);
    assert_eq!(projection.gate_data.owner_kind, GateOwnerKind::Human);
    assert_eq!(
        projection.execution_plan_data.objective.as_deref(),
        Some("Coordinate rollout")
    );
    assert!(projection.execution_plan_data_from_full);
    assert_eq!(
        projection.description.as_deref(),
        Some("legacy description")
    );
    assert_eq!(projection.body.as_deref(), Some("legacy description"));
    assert_eq!(
        projection.created_at.as_deref(),
        Some("2026-02-25T10:01:00Z")
    );
}

#[test]
fn standalone_rehydrate_events_update_profile_structured_fields_and_duplicates() {
    let mut projection = seed_projection();
    projection.workflow_id = "work_sdlc".to_string();
    projection.profile_id = "default".to_string();

    apply_rehydrate_event(
        &mut projection,
        &event(
            FullEventKind::KnotProfileSet,
            json!({
                "workflow_id": " custom ",
                "to_profile_id": " Autopilot ",
                "to_state": "ready_for_implementation",
                "deferred_from_state": "planning",
                "blocked_from_state": "implementation"
            }),
        ),
    );
    assert_eq!(projection.workflow_id, "custom");
    assert_eq!(projection.profile_id, "autopilot");
    assert_eq!(projection.state, "ready_for_implementation");

    apply_rehydrate_event(
        &mut projection,
        &event(
            FullEventKind::KnotGateDataSet,
            json!({"gate": {"owner_kind": "human"}}),
        ),
    );
    apply_rehydrate_event(
        &mut projection,
        &event(
            FullEventKind::KnotExecutionPlanDataSet,
            json!({"execution_plan": {"objective": "Later objective"}}),
        ),
    );
    apply_rehydrate_event(
        &mut projection,
        &event(
            FullEventKind::KnotInvariantsSet,
            json!({"invariants": [{"type": "State", "condition": "must be active"}]}),
        ),
    );
    apply_rehydrate_event(
        &mut projection,
        &event(
            FullEventKind::KnotVerificationStepsSet,
            json!({"verification_steps": ["make sanity"]}),
        ),
    );
    let handoff = event(
        FullEventKind::KnotHandoffCapsuleAdded,
        json!({
            "entry_id": "h1",
            "content": "handoff",
            "username": "u",
            "datetime": "2026-02-25T10:01:00Z",
            "agentname": "a",
            "model": "m",
            "version": "v"
        }),
    );
    apply_rehydrate_event(&mut projection, &handoff);
    apply_rehydrate_event(&mut projection, &handoff);

    assert_eq!(projection.gate_data.owner_kind, GateOwnerKind::Human);
    assert_eq!(
        projection.execution_plan_data.objective.as_deref(),
        Some("Later objective")
    );
    assert_eq!(projection.invariants[0].condition, "must be active");
    assert_eq!(
        projection.verification_steps,
        vec!["make sanity".to_string()]
    );
    assert_eq!(projection.handoff_capsules.len(), 1);
}

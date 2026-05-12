use crate::app::helpers::{build_knot_head_data, KnotHeadData};
use crate::domain::execution_plan::ExecutionPlanData;
use crate::domain::gate::GateData;
use crate::domain::knot_type::KnotType;
use crate::domain::scope::{ScopeData, ScopeFloat};

fn base_head<'a>(
    gate_data: &'a GateData,
    execution_plan_data: &'a ExecutionPlanData,
) -> KnotHeadData<'a> {
    KnotHeadData {
        knot_id: "K-test",
        title: "title",
        state: "implementation",
        workflow_id: "work_sdlc",
        profile_id: "autopilot",
        updated_at: "2026-05-12T00:00:00Z",
        terminal: false,
        deferred_from_state: None,
        blocked_from_state: None,
        invariants: &[],
        knot_type: KnotType::Work,
        gate_data,
        execution_plan_data,
        scope_data: None,
        step_metadata: None,
        next_step_metadata: None,
    }
}

#[test]
fn build_knot_head_data_omits_scope_key_when_none() {
    let gate_data = GateData::default();
    let execution_plan_data = ExecutionPlanData::default();
    let head = base_head(&gate_data, &execution_plan_data);
    let value = build_knot_head_data(head);
    assert!(value.get("scope").is_none());
}

#[test]
fn build_knot_head_data_omits_scope_key_when_empty() {
    let gate_data = GateData::default();
    let execution_plan_data = ExecutionPlanData::default();
    let empty = ScopeData::default();
    let mut head = base_head(&gate_data, &execution_plan_data);
    head.scope_data = Some(&empty);
    let value = build_knot_head_data(head);
    assert!(value.get("scope").is_none());
}

#[test]
fn build_knot_head_data_includes_scope_when_present() {
    let gate_data = GateData::default();
    let execution_plan_data = ExecutionPlanData::default();
    let scope = ScopeData {
        volume: Some(8),
        scale: Some("fib_v1".to_string()),
        volume_score_confidence: Some(ScopeFloat::new(0.5).expect("finite")),
        ..ScopeData::default()
    };
    let mut head = base_head(&gate_data, &execution_plan_data);
    head.scope_data = Some(&scope);
    let value = build_knot_head_data(head);
    let payload = value.get("scope").expect("scope key present");
    assert_eq!(payload.get("volume").and_then(|v| v.as_u64()), Some(8));
    assert_eq!(
        payload.get("scale").and_then(|v| v.as_str()),
        Some("fib_v1")
    );
    assert_eq!(
        payload
            .get("volume_score_confidence")
            .and_then(|v| v.as_f64()),
        Some(0.5)
    );
    assert!(payload.get("reliability").is_none());
}

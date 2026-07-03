use serde_json::json;

use super::execution_plan::{ExecutionPlanData, ExecutionPlanStep};

#[test]
fn execution_plan_legacy_id_deserializers_report_invalid_shapes() {
    let bad_step = serde_json::from_value::<ExecutionPlanStep>(json!({
        "step_index": 1,
        "beat_ids": "not a list"
    }));
    assert!(bad_step.is_err());

    let bad_plan = serde_json::from_value::<ExecutionPlanData>(json!({
        "unassigned_beat_ids": "not a list"
    }));
    assert!(bad_plan.is_err());
}

#[test]
fn execution_plan_direct_step_deserializer_accepts_legacy_ids() {
    let step: ExecutionPlanStep = serde_json::from_value(json!({
        "step_index": 3,
        "beat_ids": ["legacy-step"],
        "notes": "legacy source"
    }))
    .expect("legacy step ids should deserialize");

    assert_eq!(step.step_index, 3);
    assert_eq!(step.knot_ids, vec!["legacy-step"]);
    assert_eq!(step.notes.as_deref(), Some("legacy source"));
}

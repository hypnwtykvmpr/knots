use super::execution_plan::{ExecutionPlanData, ExecutionPlanStep, ExecutionPlanWave};
use super::execution_plan_edit::{add_step, remove_step};

fn sparse_plan() -> ExecutionPlanData {
    ExecutionPlanData {
        objective: Some("Coordinate sparse indexes".to_string()),
        waves: vec![
            ExecutionPlanWave {
                wave_index: 1,
                name: "One".to_string(),
                objective: "First".to_string(),
                ..Default::default()
            },
            ExecutionPlanWave {
                wave_index: 5,
                name: "Five".to_string(),
                objective: "Fifth".to_string(),
                steps: vec![ExecutionPlanStep {
                    step_index: 4,
                    knot_ids: vec!["K-gate".to_string()],
                    notes: Some("stored index".to_string()),
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn step_remove_uses_stored_wave_and_step_indexes() {
    let (next, cascade) = remove_step(&sparse_plan(), 5, 4).expect("remove stored step");

    assert_eq!(cascade.affected_knot_ids, vec!["K-gate"]);
    assert_eq!(next.waves.len(), 2);
    assert_eq!(next.waves[1].wave_index, 5);
    assert!(next.waves[1].steps.is_empty());
}

#[test]
fn step_add_uses_stored_wave_index() {
    let next = add_step(&sparse_plan(), 5, vec!["K-new".to_string()], None, None)
        .expect("add to stored wave");

    assert_eq!(next.waves[1].wave_index, 5);
    assert_eq!(next.waves[1].steps.len(), 2);
    assert_eq!(next.waves[1].steps[1].knot_ids, vec!["K-new"]);
    assert_eq!(next.waves[1].steps[1].step_index, 2);
}

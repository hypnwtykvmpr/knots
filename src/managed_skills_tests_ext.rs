use super::{managed_skills, render_skill};

#[test]
fn managed_skill_inventory_includes_knots_create() {
    let names = managed_skills()
        .iter()
        .map(|skill| skill.deploy_name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "knots",
            "knots-e2e",
            "knots-create",
            "knots-plan-orchestrator"
        ]
    );
}

#[test]
fn knots_create_skill_describes_structured_creation_inputs() {
    let skill = managed_skills()
        .iter()
        .copied()
        .find(|skill| skill.deploy_name == "knots-create")
        .expect("knots-create should be managed");
    let rendered = render_skill(skill);

    assert!(rendered.contains("name: knots-create"));
    assert!(rendered.contains("Put the"));
    assert!(rendered.contains("goal, verification steps, and constraints in `-d`"));
    assert!(rendered.contains("Put only numbered"));
    assert!(rendered.contains("acceptance criteria in `--acceptance`"));
    assert!(rendered.contains("Goal:"));
    assert!(rendered.contains("Verification:"));
    assert!(rendered.contains("Constraints:"));
    assert!(rendered.contains("exact commands or"));
    assert!(rendered.contains("UI actions"));
    assert!(rendered.contains("API routes"));
    assert!(rendered.contains("file paths"));
    assert!(rendered.contains("kno new \"<title>\""));
    assert!(rendered.contains("--acceptance"));
}

#[test]
fn knots_plan_orchestrator_skill_describes_plan_execution_protocol() {
    let skill = managed_skills()
        .iter()
        .copied()
        .find(|skill| skill.deploy_name == "knots-plan-orchestrator")
        .expect("knots-plan-orchestrator should be managed");
    let rendered = render_skill(skill);

    assert!(rendered.contains("name: knots-plan-orchestrator"));
    assert!(rendered.contains("# Knots Plan Orchestrator"));
    assert!(rendered.contains("kno show <plan-id> --json"));
    assert!(rendered.contains("execution_plan"));
    assert!(rendered.contains("Waves are sequential"));
    assert!(rendered.contains("Steps within a wave are sequential"));
    assert!(rendered.contains("Knots within a step are concurrent"));
    assert!(rendered.contains("kno show <knot-id> --json"));
    assert!(rendered.contains("kno next <plan-id>"));
    assert!(rendered.contains("kno rollback <plan-id>"));
    assert!(rendered.contains("kno -C <path_to_repo>"));
    assert!(rendered.contains("SHIPPED"));
    assert!(rendered.contains("BLOCKED"));
    assert!(rendered.contains("DEFERRED"));
    assert!(rendered.contains("your own protocol for launching and managing coding"));
}

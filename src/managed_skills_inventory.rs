use super::ManagedSkill;

const KNOTS: &str = include_str!("../skills/knots.md");
const KNOTS_E2E: &str = include_str!("../skills/knots_e2e.md");
const KNOTS_CREATE: &str = include_str!("../skills/knots_create.md");
const KNOTS_PLAN_ORCHESTRATOR: &str = include_str!("../skills/knots_plan_orchestrator.md");

pub(super) fn managed_skills() -> &'static [ManagedSkill] {
    &[
        ManagedSkill {
            deploy_name: "knots",
            contents: KNOTS,
        },
        ManagedSkill {
            deploy_name: "knots-e2e",
            contents: KNOTS_E2E,
        },
        ManagedSkill {
            deploy_name: "knots-create",
            contents: KNOTS_CREATE,
        },
        ManagedSkill {
            deploy_name: "knots-plan-orchestrator",
            contents: KNOTS_PLAN_ORCHESTRATOR,
        },
    ]
}

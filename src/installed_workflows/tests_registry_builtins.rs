//! Builtin-registration behavior split from tests_registry_ext for size.

use super::operations::write_repo_config;
use super::tests_helpers::{unique_workspace, SAMPLE_BUNDLE};
use super::*;
use crate::domain::knot_type::KnotType;

fn ensure_builtin_registry(root: &std::path::Path) -> WorkflowRepoConfig {
    ensure_builtin_workflows_registered(root).expect("builtin registration")
}

#[test]
fn ensure_builtin_registration_adds_missing_entries_without_changing_defaults() {
    let root = unique_workspace("knots-installed-workflows-register-builtins");
    let source = root.join("custom-flow.toml");
    std::fs::write(&source, SAMPLE_BUNDLE).expect("bundle should write");
    install_bundle(&root, &source).expect("bundle should install");

    let mut config = ensure_builtin_registry(&root);
    config.knot_type_workflows.insert(
        KnotType::Work.as_str().to_string(),
        KnotTypeWorkflowConfig {
            default: WorkflowRef::new("custom_flow", Some(3)),
            registered: vec![WorkflowRef::new("custom_flow", Some(3))],
        },
    );
    config.knot_type_workflows.insert(
        KnotType::Explore.as_str().to_string(),
        KnotTypeWorkflowConfig {
            default: WorkflowRef::new("custom_flow", Some(3)),
            registered: vec![WorkflowRef::new("custom_flow", Some(3))],
        },
    );
    write_repo_config(&root, &config).expect("config writes");

    let config = ensure_builtin_workflows_registered(&root).expect("builtin registration");
    assert_eq!(
        config
            .current_workflow_ref_for_knot_type(KnotType::Work)
            .map(|workflow| workflow.workflow_id),
        Some("custom_flow".to_string())
    );
    assert_eq!(
        config
            .current_workflow_ref_for_knot_type(KnotType::Explore)
            .map(|workflow| workflow.workflow_id),
        Some("custom_flow".to_string())
    );
    assert_eq!(
        config
            .current_workflow_ref_for_knot_type(KnotType::Gate)
            .map(|workflow| workflow.workflow_id),
        Some("gate_sdlc".to_string())
    );
    assert_eq!(
        config
            .current_workflow_ref_for_knot_type(KnotType::Lease)
            .map(|workflow| workflow.workflow_id),
        Some("lease_sdlc".to_string())
    );
    assert_eq!(
        config
            .current_workflow_ref_for_knot_type(KnotType::ExecutionPlan)
            .map(|workflow| workflow.workflow_id),
        Some("execution_plan_sdlc".to_string())
    );
    let registry = InstalledWorkflowRegistry::load(&root).expect("registry should load");
    let work_ids = registry
        .registered_workflows_for_knot_type(KnotType::Work)
        .iter()
        .map(|workflow| workflow.id.as_str())
        .collect::<Vec<_>>();
    assert!(work_ids.contains(&"custom_flow"));
    assert!(work_ids.contains(&"work_sdlc"));

    let explore_ids = registry
        .registered_workflows_for_knot_type(KnotType::Explore)
        .iter()
        .map(|workflow| workflow.id.as_str())
        .collect::<Vec<_>>();
    assert!(explore_ids.contains(&"custom_flow"));
    assert!(explore_ids.contains(&"explore_sdlc"));

    let execution_plan_ids = registry
        .registered_workflows_for_knot_type(KnotType::ExecutionPlan)
        .iter()
        .map(|workflow| workflow.id.as_str())
        .collect::<Vec<_>>();
    assert!(execution_plan_ids.contains(&"execution_plan_sdlc"));

    let _ = std::fs::remove_dir_all(root);
}

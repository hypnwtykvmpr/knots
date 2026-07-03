use super::operations::{
    read_bundle_source, repo_config_path, resolve_bundle_source_path, write_repo_config,
};
use super::tests_helpers::{unique_workspace, SAMPLE_BUNDLE};
use super::*;
use crate::domain::knot_type::KnotType;

#[test]
fn read_repo_config_rejects_legacy_config_until_migrated() {
    let root = unique_workspace("knots-installed-workflows-legacy-read");
    let path = repo_config_path(&root);
    std::fs::create_dir_all(path.parent().expect("config parent should exist"))
        .expect("config parent should be creatable");
    std::fs::write(
        &path,
        "current_workflow = \"compatibility\"\ncurrent_profile = \"human_gate\"\n",
    )
    .expect("legacy config should write");

    let err = read_repo_config(&root).expect_err("legacy config should require migration");
    assert!(err.to_string().contains("requires migration"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn register_workflow_and_default_profile_noop_write_expected_config() {
    let root = unique_workspace("knots-installed-workflows-register-op");
    let source = root.join("custom-flow.toml");
    std::fs::write(&source, SAMPLE_BUNDLE).expect("bundle should write");
    assert_eq!(
        install_bundle(&root, &source).expect("install wrapper should succeed"),
        "custom_flow"
    );

    let registered =
        register_workflow_for_knot_type(&root, KnotType::Explore, "custom_flow", Some(3), false)
            .expect("workflow should register for explore");
    let explore_config = registered
        .knot_type_workflows
        .get(KnotType::Explore.as_str())
        .expect("explore workflow config should exist");
    assert_eq!(explore_config.default.workflow_id, "explore_sdlc");
    assert!(explore_config
        .registered
        .iter()
        .any(|workflow| workflow.workflow_id == "custom_flow"));

    let unchanged = set_workflow_default_profile(&root, "custom_flow", None)
        .expect("missing profile should leave config unchanged");
    assert_eq!(
        unchanged
            .current_workflow_ref_for_knot_type(KnotType::Explore)
            .map(|workflow| workflow.workflow_id),
        Some("explore_sdlc".to_string())
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bundle_source_resolution_checks_files_candidates_and_missing_dirs() {
    let root = unique_workspace("knots-installed-workflows-source-resolution");
    let direct = root.join("direct.json");
    std::fs::write(&direct, "{\"format\":\"knots-bundle\"}\n").expect("json should write");
    let (raw, format) = read_bundle_source(&direct).expect("direct file should read");
    assert!(matches!(format, BundleFormat::Json));
    assert!(raw.contains("knots-bundle"));

    let package = root.join("package");
    std::fs::create_dir_all(package.join("dist")).expect("dist should exist");
    let candidate = package.join("dist").join(TOML_BUNDLE_FILE);
    std::fs::write(&candidate, SAMPLE_BUNDLE).expect("candidate bundle should write");
    assert_eq!(
        resolve_bundle_source_path(&package).expect("candidate should resolve"),
        candidate
    );

    let missing = root.join("missing");
    let err = resolve_bundle_source_path(&missing).expect_err("missing source should fail");
    assert!(err.to_string().contains("does not exist"));

    let empty = root.join("empty");
    std::fs::create_dir_all(&empty).expect("empty source should exist");
    let err = resolve_bundle_source_path(&empty).expect_err("empty source should fail");
    assert!(err.to_string().contains("no Loom bundle found"));

    let mut config = WorkflowRepoConfig::default();
    config.register_workflow_for_knot_type(
        KnotType::Work,
        WorkflowRef::new("custom_flow", Some(3)),
        true,
    );
    write_repo_config(&root, &config).expect("config should write");
    assert!(repo_config_path(&root).exists());

    let _ = std::fs::remove_dir_all(root);
}

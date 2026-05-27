use super::{App, CreateKnotOptions, StateActorMetadata, UpdateKnotPatch};
use std::path::PathBuf;
use uuid::Uuid;

fn unique_workspace() -> PathBuf {
    let r = std::env::temp_dir().join(format!("knots-app-test-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&r).expect("mkdir");
    r
}

fn empty_patch() -> UpdateKnotPatch {
    UpdateKnotPatch {
        title: None,
        description: None,
        acceptance: None,
        priority: None,
        status: None,
        knot_type: None,
        add_tags: vec![],
        remove_tags: vec![],
        add_invariants: vec![],
        remove_invariants: vec![],
        clear_invariants: false,
        add_verification_steps: vec![],
        remove_verification_steps: vec![],
        clear_verification_steps: false,
        gate_owner_kind: None,
        gate_failure_modes: None,
        clear_gate_failure_modes: false,
        execution_plan_objective: None,
        execution_plan_data: None,
        add_note: None,
        add_handoff_capsule: None,
        expected_profile_etag: None,
        force: false,
        state_actor: StateActorMetadata::default(),
    }
}

#[test]
fn create_and_update_preserve_tag_casing_with_case_insensitive_matching() {
    let root = unique_workspace();
    let db = root.join(".knots/cache/state.sqlite");
    let app = App::open(db.to_str().expect("u"), root.clone()).expect("o");
    let c = app
        .create_knot_with_options(
            "Tag casing",
            None,
            Some("work_item"),
            Some("default"),
            None,
            CreateKnotOptions {
                tags: vec![
                    "Journey-Github-Connect".to_string(),
                    "journey-github-connect".to_string(),
                ],
                ..CreateKnotOptions::default()
            },
        )
        .expect("c");
    assert_eq!(c.tags, vec!["Journey-Github-Connect".to_string()]);

    let mixed = app
        .update_knot(
            &c.id,
            UpdateKnotPatch {
                add_tags: vec!["MixedCase-Tag".to_string(), "mixedcase-tag".to_string()],
                ..empty_patch()
            },
        )
        .expect("u");
    assert_eq!(
        mixed.tags,
        vec![
            "Journey-Github-Connect".to_string(),
            "MixedCase-Tag".to_string()
        ]
    );

    let removed = app
        .update_knot(
            &c.id,
            UpdateKnotPatch {
                remove_tags: vec!["MIXEDCASE-TAG".to_string()],
                ..empty_patch()
            },
        )
        .expect("r");
    assert_eq!(removed.tags, vec!["Journey-Github-Connect".to_string()]);
    let _ = std::fs::remove_dir_all(root);
}

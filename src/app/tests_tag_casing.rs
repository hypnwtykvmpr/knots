use super::{App, CreateKnotOptions, StateActorMetadata, UpdateKnotPatch};
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn unique_workspace() -> PathBuf {
    let r = std::env::temp_dir().join(format!("knots-tag-casing-{}", Uuid::now_v7()));
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

fn open_app(root: &Path) -> App {
    let db = root.join(".knots/cache/state.sqlite");
    App::open(db.to_str().expect("u"), root.to_path_buf()).expect("o")
}

#[test]
fn create_knot_preserves_mixed_case_tag() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot_with_options(
            "Tag casing smoke",
            None,
            Some("idea"),
            Some("default"),
            None,
            CreateKnotOptions {
                tags: vec!["Journey-Github-Connect".into()],
                ..CreateKnotOptions::default()
            },
        )
        .expect("create");
    assert_eq!(
        created.tags,
        vec!["Journey-Github-Connect".to_string()],
        "tag casing should be preserved on create"
    );
    let shown = app.show_knot(&created.id).expect("show").expect("exists");
    assert_eq!(shown.tags, vec!["Journey-Github-Connect".to_string()]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn update_knot_preserves_added_mixed_case_tag() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot("Updateable", None, Some("idea"), Some("default"))
        .expect("create");

    let updated = app
        .update_knot(
            &created.id,
            UpdateKnotPatch {
                add_tags: vec!["MixedCase-Tag".into()],
                ..empty_patch()
            },
        )
        .expect("add tag");
    assert_eq!(updated.tags, vec!["MixedCase-Tag".to_string()]);
    let shown = app.show_knot(&created.id).expect("show").expect("exists");
    assert_eq!(shown.tags, vec!["MixedCase-Tag".to_string()]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn add_tag_with_different_case_is_idempotent_and_keeps_first_spelling() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot_with_options(
            "Idempotent",
            None,
            Some("idea"),
            Some("default"),
            None,
            CreateKnotOptions {
                tags: vec!["MixedCase-Tag".into()],
                ..CreateKnotOptions::default()
            },
        )
        .expect("create");

    let updated = app
        .update_knot(
            &created.id,
            UpdateKnotPatch {
                add_tags: vec!["mixedcase-tag".into()],
                ..empty_patch()
            },
        )
        .expect("re-adding same tag (different case) should succeed as a no-op");
    assert_eq!(
        updated.tags,
        vec!["MixedCase-Tag".to_string()],
        "case-insensitive dedup must not add a duplicate; first spelling wins"
    );

    let shown = app.show_knot(&created.id).expect("show").expect("exists");
    assert_eq!(shown.tags, vec!["MixedCase-Tag".to_string()]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn remove_tag_is_case_insensitive() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot_with_options(
            "Removable",
            None,
            Some("idea"),
            Some("default"),
            None,
            CreateKnotOptions {
                tags: vec!["MixedCase-Tag".into(), "Other".into()],
                ..CreateKnotOptions::default()
            },
        )
        .expect("create");
    let updated = app
        .update_knot(
            &created.id,
            UpdateKnotPatch {
                remove_tags: vec!["MIXEDCASE-TAG".into()],
                ..empty_patch()
            },
        )
        .expect("remove");
    assert_eq!(updated.tags, vec!["Other".to_string()]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rehydrate_preserves_mixed_case_tag_from_events() {
    let root = unique_workspace();
    let app = open_app(&root);
    let created = app
        .create_knot_with_options(
            "Rehydrate me",
            None,
            Some("idea"),
            Some("default"),
            None,
            CreateKnotOptions {
                tags: vec!["Journey-Github-Connect".into()],
                ..CreateKnotOptions::default()
            },
        )
        .expect("create");

    let db_path = root.join(".knots/cache/state.sqlite");
    let conn = crate::db::open_connection(db_path.to_str().expect("u")).expect("db");
    crate::db::delete_knot_hot(&conn, &created.id).expect("delete hot");
    crate::db::upsert_knot_warm(&conn, &created.id, &created.title).expect("warm");
    crate::db::upsert_cold_catalog(
        &conn,
        &created.id,
        &created.title,
        &created.state,
        &created.updated_at,
    )
    .expect("cold");
    drop(conn);

    let rehydrated = app
        .rehydrate(&created.id)
        .expect("rehydrate")
        .expect("exists");
    assert_eq!(
        rehydrated.tags,
        vec!["Journey-Github-Connect".to_string()],
        "rehydrate must preserve the original tag casing from events"
    );
    let _ = std::fs::remove_dir_all(root);
}

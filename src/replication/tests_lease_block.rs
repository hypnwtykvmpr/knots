//! Push/pull/sync deferral when active leases exist.

use crate::db;
use crate::remote_init::init_remote_knots_branch;
use crate::sync::SyncError;

use super::tests::{setup_origin_and_dev1, unique_workspace};
use super::ReplicationService;

#[test]
fn push_blocks_with_active_leases() {
    let root = unique_workspace();
    let (_origin, dev1) = setup_origin_and_dev1(&root);
    init_remote_knots_branch(&dev1).expect("remote knots branch should initialize");

    let db_path = dev1.join(".knots/cache/state.sqlite");
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    let conn = db::open_connection(db_path.to_str().expect("utf8 path")).expect("db should open");

    let gate_data = crate::domain::gate::GateData::default();
    db::upsert_knot_hot(
        &conn,
        &db::UpsertKnotHot {
            id: "K-lease-block",
            title: "Lease: blocking",
            state: "lease_active",
            updated_at: "2026-03-12T00:00:00Z",
            body: None,
            description: None,
            acceptance: None,
            priority: None,
            knot_type: Some("lease"),
            tags: &[],
            notes: &[],
            handoff_capsules: &[],
            invariants: &[],
            verification_steps: &[],
            step_history: &[],
            gate_data: &gate_data,
            lease_data: &crate::domain::lease::LeaseData::default(),
            execution_plan_data: &crate::domain::execution_plan::ExecutionPlanData::default(),
            lease_id: None,
            workflow_id: "lease_sdlc",
            profile_id: "autopilot",
            profile_etag: None,
            deferred_from_state: None,
            blocked_from_state: None,
            created_at: None,
        },
    )
    .expect("lease upsert should succeed");
    db::update_lease_expiry_ts(
        &conn,
        "K-lease-block",
        crate::lease_expiry::compute_expiry_ts(600),
    )
    .expect("expiry update should succeed");

    let service = ReplicationService::new(&conn, dev1.clone());
    let err = service
        .push()
        .expect_err("push should fail with active leases");
    assert!(
        matches!(err, SyncError::ActiveLeasesExist(1)),
        "expected ActiveLeasesExist(1), got {:?}",
        err
    );

    let pull_err = service
        .pull()
        .expect_err("pull should fail with active leases");
    assert!(pull_err.is_active_leases());

    let sync_err = service
        .sync()
        .expect_err("sync should fail with active leases");
    assert!(sync_err.is_active_leases());

    // sync_or_defer returns Deferred instead of erroring
    let mut reporter = None;
    let outcome = service
        .sync_or_defer_with_progress(&mut reporter)
        .expect("sync_or_defer should succeed");
    assert_eq!(
        outcome,
        crate::replication::SyncOutcome::Deferred { active_leases: 1 }
    );

    let _ = std::fs::remove_dir_all(root);
}

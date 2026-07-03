use std::path::{Path, PathBuf};
use std::process::Command;

use super::{App, AppError};
use crate::project::{DistributionMode, ProjectContext, StorePaths};
use crate::replication::SyncOutcome;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-app-sync-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn run_git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_git_repo_with_remote(root: &Path) {
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "knots@example.com"]);
    run_git(root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# knots\n").expect("readme should write");
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(root, &["branch", "-M", "main"]);

    let remote = root.join("remote.git");
    run_git(
        root,
        &[
            "init",
            "--bare",
            remote.to_str().expect("remote should be utf8"),
        ],
    );
    run_git(
        root,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote should be utf8"),
        ],
    );
    run_git(root, &["push", "-u", "origin", "main"]);
}

fn local_context(root: &Path) -> ProjectContext {
    ProjectContext {
        project_id: Some("local".to_string()),
        repo_root: root.join("repo"),
        store_paths: StorePaths {
            root: root.join("store"),
        },
        distribution: DistributionMode::LocalOnly,
    }
}

fn open_local_app(root: &Path) -> App {
    let context = local_context(root);
    std::fs::create_dir_all(&context.store_paths.root).expect("store root should exist");
    let db_path = context.store_paths.db_path();
    App::open_with_context(&context, db_path.to_str().expect("db path should be utf8"))
        .expect("local-only app should open")
}

fn unsupported<T>(result: Result<T, AppError>, action: &str) {
    assert!(
        matches!(
            result,
            Err(AppError::UnsupportedDistribution {
                action: actual,
                mode
            }) if actual == action && mode == "local-only"
        ),
        "{action} should reject local-only distribution"
    );
}

#[test]
fn local_only_distribution_rejects_git_replication_commands_before_locking() {
    let root = unique_workspace();
    let app = open_local_app(&root);

    unsupported(app.pull(), "pull");
    unsupported(app.pull_with_progress(None), "pull");
    unsupported(app.pull_drift_warning(), "pull");
    unsupported(app.push(), "push");
    unsupported(app.push_with_progress(None), "push");
    unsupported(app.sync(), "sync");
    unsupported(app.sync_with_progress(None), "sync");
    unsupported(app.sync_or_defer_with_progress(None), "sync");
    unsupported(app.init_remote(None), "init-remote");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn local_only_maintenance_commands_do_not_require_git_distribution() {
    let root = unique_workspace();
    let app = open_local_app(&root);

    let fsck = app.fsck().expect("fsck should run against local store");
    assert!(fsck.ok());

    crate::doctor_fix::set_version_fix_applied_for_tests(true);
    let doctor = app
        .doctor_with_progress(false, None)
        .expect("doctor should inspect local store");
    crate::doctor_fix::set_version_fix_applied_for_tests(false);
    assert!(!doctor.checks.is_empty());

    let snapshots = app
        .compact_write_snapshots()
        .expect("snapshot compaction should run");
    assert_eq!(snapshots.hot_count, 0);
    assert_eq!(snapshots.warm_count, 0);
    assert_eq!(snapshots.cold_count, 0);

    let perf = app.perf_harness(1).expect("perf harness should run");
    assert_eq!(perf.iterations, 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sync_or_defer_marks_sync_pending_when_active_leases_exist() {
    let root = unique_workspace();
    std::fs::create_dir_all(root.join(".knots")).expect(".knots should exist");
    let db_path = root.join(".knots/cache/state.sqlite");
    let app = App::open(
        db_path.to_str().expect("db path should be utf8"),
        root.clone(),
    )
    .expect("git-distribution app should open");

    crate::lease::create_lease(
        &app,
        "sync-pending",
        crate::domain::lease::LeaseType::Agent,
        None,
        3600,
    )
    .expect("active lease fixture should be created");

    let outcome = app
        .sync_or_defer_with_progress(None)
        .expect("sync should defer with active leases");
    assert!(matches!(outcome, SyncOutcome::Deferred { active_leases } if active_leases == 1));

    let pending = crate::db::get_meta(app.conn_for_test(), "sync_pending")
        .expect("sync_pending meta should read");
    assert_eq!(pending.as_deref(), Some("true"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn git_distribution_sync_methods_cover_direct_success_paths() {
    let root = unique_workspace();
    setup_git_repo_with_remote(&root);
    let db_path = root.join(".knots/cache/state.sqlite");
    let app = App::open(
        db_path.to_str().expect("db path should be utf8"),
        root.clone(),
    )
    .expect("git-distribution app should open");

    app.init_remote(Some("refs/heads/knots-alt"))
        .expect("custom remote ref should initialize");
    let push = app
        .push()
        .expect("push should succeed without local events");
    assert!(!push.pushed);
    let push_progress = app
        .push_with_progress(None)
        .expect("push with progress should succeed without local events");
    assert!(!push_progress.pushed);
    app.pull().expect("pull should succeed");
    let drift = app
        .pull_drift_warning()
        .expect("drift warning should compute");
    assert!(drift.is_none());
    app.sync().expect("sync should succeed");
    app.sync_with_progress(None)
        .expect("sync with progress slot should succeed");
    let outcome = app
        .sync_or_defer_with_progress(None)
        .expect("sync_or_defer should complete without active leases");
    assert!(matches!(outcome, SyncOutcome::Completed(_)));
    let compact = app
        .compact_write_snapshots()
        .expect("snapshot compaction should succeed");
    assert_eq!(compact.hot_count, 0);

    let _ = std::fs::remove_dir_all(root);
}

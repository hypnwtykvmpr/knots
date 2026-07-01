use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::{announce_and_apply_fixes, apply_fixes_with_progress, sync_after_fixes};
use crate::doctor::{DoctorCheck, DoctorReport, DoctorStatus};
use crate::progress::{ProgressKind, ProgressReporter};
use crate::project::DistributionMode;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-doctor-fix-progress-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn sample_check(name: &str, status: DoctorStatus) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status,
        detail: "detail".to_string(),
        data: None,
    }
}

fn open_cache(root: &Path) {
    let db_path = root.join(".knots/cache/state.sqlite");
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    let _conn = crate::db::open_connection(db_path.to_str().expect("db path should be utf8"))
        .expect("db should open");
}

fn write_stale_worktree_head(root: &Path) {
    let dir = root
        .join(".knots")
        .join("_worktree")
        .join(".knots")
        .join("index")
        .join("2026")
        .join("03")
        .join("12");
    std::fs::create_dir_all(&dir).expect("worktree index dir should be creatable");
    std::fs::write(
        dir.join("0001-idx.knot_head.json"),
        r#"{
  "event_id": "1",
  "occurred_at": "2026-03-12T10:00:00Z",
  "type": "idx.knot_head",
  "data": {
    "knot_id": "K-fail",
    "title": "Failing repair",
    "state": "implementation",
    "profile_id": "autopilot",
    "updated_at": "2026-03-12T10:00:00Z",
    "type": "work",
    "terminal": false
  }
}
"#,
    )
    .expect("stale worktree event should be writable");
}

#[derive(Default)]
struct CapturingReporter {
    events: Vec<(ProgressKind, String)>,
}

impl ProgressReporter for CapturingReporter {
    fn emit(&mut self, kind: ProgressKind, message: &str) -> std::io::Result<()> {
        self.events.push((kind, message.to_string()));
        Ok(())
    }
}

#[test]
fn apply_fixes_with_progress_emits_line_per_non_pass_check() {
    let root = unique_workspace();
    let checks = vec![
        sample_check("lock_health", DoctorStatus::Pass),
        sample_check("hooks", DoctorStatus::Warn),
        sample_check("remote", DoctorStatus::Fail),
        sample_check("unknown_check", DoctorStatus::Warn),
    ];

    let mut reporter = CapturingReporter::default();
    let mut dyn_reporter: Option<&mut dyn ProgressReporter> = Some(&mut reporter);
    apply_fixes_with_progress(&root, &checks, &mut dyn_reporter);

    let messages: Vec<&str> = reporter
        .events
        .iter()
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "Fixing hooks... ok",
            "Fixing remote... ok",
            "Fixing unknown_check... skip",
        ],
        "every non-pass check should emit one ordered result line; pass checks should be skipped"
    );
    for (kind, _) in &reporter.events {
        assert_eq!(*kind, ProgressKind::Info);
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_fixes_with_progress_none_matches_silent_apply_fixes() {
    let root = unique_workspace();
    let checks = vec![sample_check("unknown_check", DoctorStatus::Warn)];
    let progress = apply_fixes_with_progress(&root, &checks, &mut None);
    assert!(!progress.outcome.event_log_touched);
    assert_eq!(progress.summary.fixed, 0);
    assert_eq!(progress.summary.skipped, 1);
    assert_eq!(progress.summary.failed, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn workflow_id_parity_noop_is_skipped_not_counted_fixed() {
    let root = unique_workspace();
    let checks = vec![sample_check("workflow_id_parity", DoctorStatus::Warn)];
    let progress = apply_fixes_with_progress(&root, &checks, &mut None);

    assert!(!progress.outcome.event_log_touched);
    assert_eq!(progress.summary.fixed, 0);
    assert_eq!(progress.summary.skipped, 1);
    assert_eq!(progress.summary.failed, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn workflow_id_parity_write_failure_is_counted_and_reported() {
    let root = unique_workspace();
    open_cache(&root);
    write_stale_worktree_head(&root);
    std::fs::write(root.join(".knots").join("index"), "not a directory")
        .expect("conflicting index file should be writable");
    let checks = vec![sample_check("workflow_id_parity", DoctorStatus::Warn)];
    let mut reporter = CapturingReporter::default();
    let mut dyn_reporter: Option<&mut dyn ProgressReporter> = Some(&mut reporter);

    let progress = apply_fixes_with_progress(&root, &checks, &mut dyn_reporter);

    assert!(!progress.outcome.event_log_touched);
    assert_eq!(progress.summary.fixed, 0);
    assert_eq!(progress.summary.skipped, 0);
    assert_eq!(progress.summary.failed, 1);
    let message = reporter
        .events
        .iter()
        .map(|(_, message)| message.as_str())
        .find(|message| message.contains("workflow_id_parity"))
        .expect("workflow_id_parity progress should be emitted");
    assert!(message.contains("failed:"));
    assert!(message.contains("K-fail"));
    assert!(message.contains("0001-idx.knot_head.json"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn missing_cache_repo_fixes_are_safe_noops_but_counted_fixed() {
    let root = unique_workspace();
    let checks = vec![
        sample_check("schema_version", DoctorStatus::Warn),
        sample_check("stuck_leases", DoctorStatus::Warn),
        sample_check("terminal_parents", DoctorStatus::Warn),
    ];

    let progress = apply_fixes_with_progress(&root, &checks, &mut None);

    assert!(progress.outcome.event_log_touched);
    assert_eq!(progress.summary.fixed, 3);
    assert_eq!(progress.summary.skipped, 0);
    assert_eq!(progress.summary.failed, 0);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn announce_and_apply_fixes_reports_noop_local_only_and_git_summaries() {
    let root = unique_workspace();
    let mut reporter = CapturingReporter::default();

    let pass_report = DoctorReport {
        checks: vec![sample_check("lock_health", DoctorStatus::Pass)],
    };
    let mut dyn_reporter: Option<&mut dyn ProgressReporter> = Some(&mut reporter);
    announce_and_apply_fixes(
        &root,
        DistributionMode::Git,
        &pass_report,
        &mut dyn_reporter,
    );

    let local_report = DoctorReport {
        checks: vec![sample_check("schema_version", DoctorStatus::Warn)],
    };
    announce_and_apply_fixes(
        &root,
        DistributionMode::LocalOnly,
        &local_report,
        &mut dyn_reporter,
    );

    let git_report = DoctorReport {
        checks: vec![
            sample_check("gitignore", DoctorStatus::Warn),
            sample_check("stuck_leases", DoctorStatus::Warn),
            sample_check("knot_type_backfill", DoctorStatus::Warn),
            sample_check("cold_tier_imbalance", DoctorStatus::Warn),
        ],
    };
    announce_and_apply_fixes(&root, DistributionMode::Git, &git_report, &mut dyn_reporter);

    let messages = reporter
        .events
        .iter()
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();
    assert!(messages.contains(&"No issues found."));
    assert!(messages
        .iter()
        .any(|message| message.contains("local-only mode does not apply fixes")));
    assert!(messages
        .iter()
        .any(|message| message.contains("fixed") && message.contains("skipped")));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sync_after_fixes_ignores_missing_or_invalid_local_store() {
    let missing = unique_workspace();
    sync_after_fixes(&missing);

    let invalid = unique_workspace();
    let db_path = invalid.join(".knots/cache/state.sqlite");
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    std::fs::write(&db_path, b"not a sqlite database").expect("invalid db should write");
    sync_after_fixes(&invalid);

    let _ = std::fs::remove_dir_all(missing);
    let _ = std::fs::remove_dir_all(invalid);
}

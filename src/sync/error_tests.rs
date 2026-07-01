use std::error::Error;
use std::path::PathBuf;

use super::SyncError;

#[test]
fn sync_error_classifiers_detect_expected_git_failures() {
    let missing_remote = SyncError::GitCommandFailed {
        command: "git fetch origin".to_string(),
        code: Some(128),
        stderr: "fatal: No such remote 'origin'".to_string(),
    };
    assert!(missing_remote.is_missing_remote());
    assert!(!missing_remote.is_unknown_revision());
    assert!(!missing_remote.is_non_fast_forward());

    let unknown_revision = SyncError::GitCommandFailed {
        command: "git rev-parse bad".to_string(),
        code: Some(128),
        stderr: "fatal: bad object deadbeef".to_string(),
    };
    assert!(!unknown_revision.is_missing_remote());
    assert!(unknown_revision.is_unknown_revision());
    assert!(!unknown_revision.is_non_fast_forward());

    let non_fast_forward = SyncError::GitCommandFailed {
        command: "git push".to_string(),
        code: Some(1),
        stderr: "rejected non-fast-forward".to_string(),
    };
    assert!(!non_fast_forward.is_missing_remote());
    assert!(!non_fast_forward.is_unknown_revision());
    assert!(non_fast_forward.is_non_fast_forward());

    let ref_policy = SyncError::GitCommandFailed {
        command: "git push".to_string(),
        code: Some(1),
        stderr: concat!(
            "remote: diffinite: Agent Personas cannot push this ref\n",
            "! HEAD:refs/heads/knots [remote rejected] (pre-receive hook declined)"
        )
        .to_string(),
    };
    assert!(ref_policy.is_ref_policy_rejection());
    assert!(!ref_policy.is_non_fast_forward());
}

#[test]
fn sync_error_classifiers_cover_alternate_git_messages() {
    for stderr in [
        "fatal: Could not read from remote repository.",
        "fatal: 'origin' does not appear to be a git repository",
    ] {
        let err = SyncError::GitCommandFailed {
            command: "git fetch".to_string(),
            code: Some(128),
            stderr: stderr.to_string(),
        };
        assert!(err.is_missing_remote(), "{stderr}");
    }

    for stderr in [
        "fatal: unknown revision or path not in the working tree.",
        "fatal: bad revision 'origin/knots'",
        "fatal: couldn't find remote ref refs/heads/knots",
    ] {
        let err = SyncError::GitCommandFailed {
            command: "git rev-parse".to_string(),
            code: Some(128),
            stderr: stderr.to_string(),
        };
        assert!(err.is_unknown_revision(), "{stderr}");
    }

    let fetch_first = SyncError::GitCommandFailed {
        command: "git push".to_string(),
        code: Some(1),
        stderr: "Updates were rejected because the remote contains work; fetch first".to_string(),
    };
    assert!(fetch_first.is_non_fast_forward());

    let not_git = SyncError::GitUnavailable;
    assert!(!not_git.is_missing_remote());
    assert!(!not_git.is_unknown_revision());
    assert!(!not_git.is_non_fast_forward());
    assert!(!not_git.is_ref_policy_rejection());
}

#[test]
fn sync_error_display_source_and_from_cover_all_variants() {
    let io: SyncError = std::io::Error::other("disk").into();
    assert!(io.to_string().contains("I/O error"));
    assert!(io.source().is_some());

    let db: SyncError = rusqlite::Error::InvalidQuery.into();
    assert!(db.to_string().contains("database error"));
    assert!(db.source().is_some());

    let unavailable = SyncError::GitUnavailable;
    assert!(unavailable.to_string().contains("git CLI is not installed"));
    assert!(unavailable.source().is_none());

    let command_failed = SyncError::GitCommandFailed {
        command: "git fetch".to_string(),
        code: Some(1),
        stderr: "bad".to_string(),
    };
    assert!(command_failed.to_string().contains("git command failed"));
    assert!(command_failed.source().is_none());

    let dirty = SyncError::DirtyWorktree(PathBuf::from("/tmp/worktree"));
    assert!(dirty.to_string().contains("has uncommitted changes"));
    assert!(dirty.source().is_none());

    let invalid_event = SyncError::InvalidEvent {
        path: PathBuf::from("/tmp/event.json"),
        message: "bad payload".to_string(),
    };
    assert!(invalid_event.to_string().contains("invalid event"));
    assert!(invalid_event.source().is_none());

    let conflict = SyncError::FileConflict {
        path: PathBuf::from("/tmp/file.json"),
    };
    assert!(conflict.to_string().contains("push conflict"));
    assert!(conflict.source().is_none());

    let escalation = SyncError::MergeConflictEscalation {
        message: "needs manual merge".to_string(),
    };
    assert!(escalation.to_string().contains("merge conflict escalation"));
    assert!(escalation.source().is_none());

    let snapshot = SyncError::SnapshotLoad {
        message: "missing snapshot".to_string(),
    };
    assert!(snapshot.to_string().contains("snapshot load failed"));
    assert!(snapshot.source().is_none());

    let active_leases = SyncError::ActiveLeasesExist(2);
    assert!(active_leases.to_string().contains("2 active lease(s)"));
    assert!(active_leases
        .to_string()
        .contains("terminate leases before syncing"));
    assert!(active_leases.source().is_none());
    assert!(active_leases.is_active_leases());
}

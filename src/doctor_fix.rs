use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::App;
use crate::doctor::{DoctorCheck, DoctorStatus};
use crate::progress::{emit_progress, ProgressKind, ProgressReporter};
use crate::project::StorePaths;
use crate::remote_init::init_remote_knots_branch;
use crate::sync::{GitAdapter, KnotsWorktree, SyncError};

static VERSION_FIX_APPLIED: AtomicBool = AtomicBool::new(false);

pub(crate) fn has_non_pass_checks(checks: &[DoctorCheck]) -> bool {
    checks
        .iter()
        .any(|check| check.status != DoctorStatus::Pass)
}

pub(crate) fn version_fix_applied() -> bool {
    VERSION_FIX_APPLIED.load(Ordering::Relaxed)
}

fn set_version_fix_applied(applied: bool) {
    VERSION_FIX_APPLIED.store(applied, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn set_version_fix_applied_for_tests(applied: bool) {
    set_version_fix_applied(applied);
}

/// Result of running `apply_fixes`. Lets the caller decide whether to take
/// pipeline-level follow-up actions (e.g. a sync to publish repair events)
/// before re-running the checks.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FixOutcome {
    /// Set when a fix wrote to `.knots/index/` or `.knots/events/`. Those
    /// events are only visible to doctor checks after `kno sync` publishes
    /// them into the shared `_worktree`.
    pub event_log_touched: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FixProgressSummary {
    pub fixed: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_fixes(repo_root: &Path, checks: &[DoctorCheck]) -> FixOutcome {
    apply_fixes_with_progress(repo_root, checks, &mut None).outcome
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FixProgress {
    pub outcome: FixOutcome,
    pub summary: FixProgressSummary,
}

enum FixResult {
    Fixed,
    Skipped(Option<String>),
}

pub(crate) fn apply_fixes_with_progress(
    repo_root: &Path,
    checks: &[DoctorCheck],
    reporter: &mut Option<&mut dyn ProgressReporter>,
) -> FixProgress {
    set_version_fix_applied(false);
    let mut outcome = FixOutcome::default();
    let mut summary = FixProgressSummary::default();
    for check in checks {
        if check.status == DoctorStatus::Pass {
            continue;
        }

        let fix_result = dispatch_fix(repo_root, check, &mut outcome);

        let result = match fix_result {
            Ok(FixResult::Fixed) => {
                summary.fixed += 1;
                "ok".to_string()
            }
            Ok(FixResult::Skipped(reason)) => {
                summary.skipped += 1;
                reason
                    .map(|message| format!("skip: {message}"))
                    .unwrap_or_else(|| "skip".to_string())
            }
            Err(err) => {
                summary.failed += 1;
                format!("failed: {err}")
            }
        };
        let _ = emit_progress(
            reporter,
            ProgressKind::Info,
            format!("Fixing {}... {result}", check.name),
        );
    }
    FixProgress { outcome, summary }
}

fn dispatch_fix(
    repo_root: &Path,
    check: &DoctorCheck,
    outcome: &mut FixOutcome,
) -> Result<FixResult, String> {
    match check.name.as_str() {
        "lock_health" => fixed_repo(fix_lock_health, repo_root),
        "worktree" => fixed_repo(fix_worktree, repo_root),
        "remote" => fixed_repo(fix_remote, repo_root),
        "gitignore" => fixed_repo(fix_gitignore, repo_root),
        "version" => fixed_noarg(fix_version),
        "hooks" => fixed_repo(fix_hooks, repo_root),
        "workflow_registry" => fixed_repo(fix_workflow_registry, repo_root),
        "schema_version" => fixed_repo(fix_schema_version, repo_root),
        "stuck_leases" => fixed_repo(fix_stuck_leases, repo_root),
        "terminal_parents" => {
            outcome.event_log_touched = true;
            fixed_repo(fix_terminal_parents, repo_root)
        }
        "cold_tier_imbalance" => {
            fixed_repo(crate::doctor_cold_tier::fix_cold_tier_imbalance, repo_root)
        }
        "workflow_id_parity" => fix_workflow_id_parity(repo_root, outcome),
        "knot_type_backfill" => fixed_repo(
            crate::doctor_knot_type_backfill::fix_knot_type_backfill,
            repo_root,
        ),
        name if name.starts_with("skills_") => fix_managed_skills(repo_root, name),
        _ => Ok(FixResult::Skipped(None)),
    }
}

fn fixed_repo(fix: fn(&Path), repo_root: &Path) -> Result<FixResult, String> {
    fix(repo_root);
    Ok(FixResult::Fixed)
}

fn fixed_noarg(fix: fn()) -> Result<FixResult, String> {
    fix();
    Ok(FixResult::Fixed)
}

fn fix_workflow_id_parity(repo_root: &Path, outcome: &mut FixOutcome) -> Result<FixResult, String> {
    let parity = crate::doctor_workflow_parity::fix_workflow_id_parity(repo_root);
    outcome.event_log_touched |= parity.needs_sync();
    if parity.failed > 0 && parity.emitted == 0 && parity.pending == 0 {
        Err(parity
            .first_message()
            .unwrap_or("workflow_id_parity repair failed")
            .to_string())
    } else if parity.emitted > 0 {
        Ok(FixResult::Fixed)
    } else {
        Ok(FixResult::Skipped(
            parity.first_message().map(str::to_string),
        ))
    }
}

fn fix_managed_skills(repo_root: &Path, name: &str) -> Result<FixResult, String> {
    crate::managed_skills::try_fix_doctor_check(repo_root, name)
        .map(|fixed| {
            if fixed {
                FixResult::Fixed
            } else {
                FixResult::Skipped(None)
            }
        })
        .map_err(|err| err.to_string())
}

fn fix_gitignore(repo_root: &Path) {
    let _ = crate::init::ensure_knots_gitignore(repo_root);
}

pub(crate) fn announce_and_apply_fixes(
    repo_root: &Path,
    distribution: crate::project::DistributionMode,
    report: &crate::doctor::DoctorReport,
    reporter: &mut Option<&mut dyn ProgressReporter>,
) {
    if !has_non_pass_checks(&report.checks) {
        let _ = emit_progress(reporter, ProgressKind::Success, "No issues found.");
        return;
    }

    let fix_count = report
        .checks
        .iter()
        .filter(|check| check.status != DoctorStatus::Pass)
        .count();

    if distribution == crate::project::DistributionMode::Git {
        let progress = apply_fixes_with_progress(repo_root, &report.checks, reporter);
        if progress.outcome.event_log_touched {
            let _ = emit_progress(reporter, ProgressKind::Info, "Syncing fix events...");
            sync_after_fixes(repo_root);
        }
        let _ = emit_progress(
            reporter,
            ProgressKind::Success,
            format!(
                "{} fixed, {} skipped, {} failed",
                progress.summary.fixed, progress.summary.skipped, progress.summary.failed
            ),
        );
    } else {
        let _ = emit_progress(
            reporter,
            ProgressKind::Info,
            format!(
                "{} issue(s) found; local-only mode does not apply fixes.",
                fix_count
            ),
        );
    }
}

/// Best-effort sync to publish repair events emitted by `apply_fixes` into
/// the shared `_worktree` so a subsequent doctor check can observe them.
///
/// Swallows all errors: if the sync fails (offline, diverged branch, missing
/// remote), the warn will simply persist into the re-check — same as if we
/// never ran sync at all. The user can rerun `kno doctor --fix` once the
/// underlying cause is addressed.
pub(crate) fn sync_after_fixes(repo_root: &Path) {
    let db_path = repo_root.join(".knots").join("cache").join("state.sqlite");
    if !db_path.exists() {
        return;
    }
    let Some(db_str) = db_path.to_str() else {
        return;
    };
    let Ok(app) = App::open(db_str, repo_root.to_path_buf()) else {
        return;
    };
    let _ = app.sync();
}

fn fix_lock_health(repo_root: &Path) {
    let locks = [
        repo_root.join(".knots").join("locks").join("repo.lock"),
        repo_root.join(".knots").join("cache").join("cache.lock"),
        repo_root
            .join(".knots")
            .join("locks")
            .join("write_queue_worker.lock"),
    ];
    for lock in locks {
        let _ = std::fs::remove_file(&lock);
        // Also clear any abandoned reclaim guard for this lock.
        let mut guard = lock.into_os_string();
        guard.push(".reclaim");
        let _ = std::fs::remove_file(std::path::PathBuf::from(guard));
    }
}

fn fix_worktree(repo_root: &Path) {
    if !repo_root.join(".git").exists() {
        return;
    }

    let git = GitAdapter::new();
    let worktree = KnotsWorktree::with_store_paths(
        repo_root.to_path_buf(),
        &StorePaths {
            root: repo_root.join(".knots"),
        },
    );

    match worktree.ensure_exists(&git) {
        Ok(()) => {}
        Err(SyncError::DirtyWorktree(path)) => {
            if path.exists() && !path.join(".git").exists() {
                let _ = std::fs::remove_dir_all(&path);
                if worktree.ensure_exists(&git).is_err() {
                    return;
                }
            } else {
                return;
            }
        }
        Err(_) => return,
    }

    let worktree_path = worktree.path();
    let _ = run_git(worktree_path, &["reset", "--hard", "HEAD"]);
    let _ = run_git(worktree_path, &["clean", "-fd"]);
}

fn fix_remote(repo_root: &Path) {
    if !repo_root.join(".git").exists() {
        return;
    }
    let _ = init_remote_knots_branch(repo_root);
}

fn fix_hooks(repo_root: &Path) {
    crate::git_hooks::cleanup_legacy_hooks(repo_root);
    let _ = crate::git_hooks::install_hooks(repo_root);
}

fn fix_workflow_registry(repo_root: &Path) {
    let _ = crate::installed_workflows::ensure_builtin_workflows_registered(repo_root);
}
fn fix_schema_version(repo_root: &Path) {
    let db_path = repo_root.join(".knots").join("cache").join("state.sqlite");
    if !db_path.exists() {
        return;
    }
    let db_str = db_path.to_str().unwrap_or(".knots/cache/state.sqlite");
    // Re-opening the connection triggers apply_migrations automatically
    let _ = crate::db::open_connection(db_str);
}
fn fix_stuck_leases(repo_root: &Path) {
    let db_path = repo_root.join(".knots").join("cache").join("state.sqlite");
    if !db_path.exists() {
        return;
    }
    let db_str = db_path.to_str().unwrap_or(".knots/cache/state.sqlite");
    let Ok(conn) = crate::db::open_connection(db_str) else {
        return;
    };
    let _ = conn.execute(
        r#"
UPDATE knot_hot
SET state = 'lease_terminated', lease_expiry_ts = 0
WHERE knot_type = 'lease'
  AND state IN ('lease_ready', 'lease_active')
"#,
        [],
    );
    // Unbind knots referencing now-terminated leases
    let _ = conn.execute(
        r#"
UPDATE knot_hot SET lease_id = NULL
WHERE lease_id IS NOT NULL
  AND lease_id IN (
    SELECT id FROM knot_hot
    WHERE knot_type = 'lease' AND state = 'lease_terminated'
  )
"#,
        [],
    );
}

fn fix_terminal_parents(repo_root: &Path) {
    let db_path = repo_root.join(".knots").join("cache").join("state.sqlite");
    let Some(db_path) = db_path.to_str() else {
        return;
    };
    let Ok(app) = App::open(db_path, repo_root.to_path_buf()) else {
        return;
    };

    loop {
        let Ok(conn) = crate::db::open_connection(db_path) else {
            return;
        };
        let Ok(resolutions) = crate::state_hierarchy::find_terminal_parent_resolutions(&conn)
        else {
            return;
        };
        drop(conn);

        if resolutions.is_empty() {
            return;
        }

        let mut progressed = false;
        for resolution in resolutions {
            if app
                .reconcile_terminal_parent_state(&resolution.parent.id, &resolution.target_state)
                .is_ok()
            {
                progressed = true;
            }
        }

        if !progressed {
            return;
        }
    }
}

fn run_git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
fn fix_version() {
    set_version_fix_applied(true);
}

#[cfg(not(test))]
fn fix_version() {
    if std::env::var_os("KNOTS_SKIP_DOCTOR_UPGRADE").is_some() {
        set_version_fix_applied(true);
        return;
    }

    let applied = if let Ok(exe_path) = std::env::current_exe() {
        Command::new(exe_path)
            .arg("upgrade")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    } else {
        Command::new("kno")
            .arg("upgrade")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };
    set_version_fix_applied(applied);
}

#[cfg(test)]
#[path = "doctor_fix_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "doctor_fix_progress_tests.rs"]
mod progress_tests;

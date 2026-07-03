use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::progress::{emit_progress, ProgressKind, ProgressReporter};
use crate::project::StorePaths;
use crate::sync::{GitAdapter, KnotsWorktree, SyncError, SyncService, SyncSummary};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct PushSummary {
    pub local_event_files: u64,
    pub copied_files: u64,
    pub committed: bool,
    pub pushed: bool,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReplicationSummary {
    pub push: PushSummary,
    pub pull: SyncSummary,
}

/// Result of a `kno sync` that gracefully handles active leases.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum SyncOutcome {
    #[serde(rename = "completed")]
    Completed(ReplicationSummary),
    #[serde(rename = "deferred")]
    Deferred { active_leases: i64 },
}

enum PushAttemptResult {
    Success(PushSummary),
    AlreadySynced(PushSummary),
    Retry(SyncError),
}

pub struct ReplicationService<'a> {
    conn: &'a Connection,
    repo_root: PathBuf,
    store_paths: StorePaths,
    git: GitAdapter,
}

impl<'a> ReplicationService<'a> {
    #[cfg(test)]
    pub fn new(conn: &'a Connection, repo_root: PathBuf) -> Self {
        let store_paths = StorePaths {
            root: repo_root.join(".knots"),
        };
        Self::with_store_paths(conn, repo_root, store_paths)
    }

    pub fn with_store_paths(
        conn: &'a Connection,
        repo_root: PathBuf,
        store_paths: StorePaths,
    ) -> Self {
        Self {
            conn,
            repo_root,
            store_paths,
            git: GitAdapter::new(),
        }
    }

    #[allow(dead_code)]
    pub fn pull(&self) -> Result<SyncSummary, SyncError> {
        self.require_no_active_leases()?;
        self.sync_service().sync()
    }

    pub fn pull_with_progress(
        &self,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<SyncSummary, SyncError> {
        self.require_no_active_leases()?;
        self.sync_service().sync_with_progress(reporter)
    }

    pub fn push(&self) -> Result<PushSummary, SyncError> {
        let mut reporter = None;
        self.push_with_progress(&mut reporter)
    }

    pub fn push_with_progress(
        &self,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<PushSummary, SyncError> {
        self.require_no_active_leases()?;
        const MAX_ATTEMPTS: usize = 3;

        emit_progress(
            reporter,
            ProgressKind::Stage,
            "publishing local knots events",
        )?;
        let worktree = KnotsWorktree::with_store_paths(self.repo_root.clone(), &self.store_paths);
        emit_progress(reporter, ProgressKind::Info, "preparing knots worktree")?;
        worktree.ensure_exists(&self.git)?;

        emit_progress(
            reporter,
            ProgressKind::Info,
            "scanning local knots event files",
        )?;
        let local_files = self.collect_local_event_files()?;
        let local_event_files = local_files.len() as u64;
        if local_event_files == 0 {
            let message = "no local knots events found; nothing to push";
            emit_progress(reporter, ProgressKind::Success, message)?;
            return Ok(unpushed_summary(local_event_files, 0));
        }

        for attempt in 0..MAX_ATTEMPTS {
            match self.attempt_push(&worktree, &local_files, local_event_files, reporter)? {
                PushAttemptResult::Success(summary) => return Ok(summary),
                PushAttemptResult::AlreadySynced(summary) => return Ok(summary),
                PushAttemptResult::Retry(err) if attempt + 1 < MAX_ATTEMPTS => {
                    let message = push_retry_message(attempt, MAX_ATTEMPTS);
                    emit_progress(reporter, ProgressKind::Warn, message)?;
                    let _ = err;
                    continue;
                }
                PushAttemptResult::Retry(_) => {
                    return Err(push_rejected_after_retries());
                }
            }
        }

        Err(push_retries_exhausted())
    }

    fn attempt_push(
        &self,
        worktree: &KnotsWorktree,
        local_files: &[PathBuf],
        local_event_files: u64,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<PushAttemptResult, SyncError> {
        self.reset_worktree_to_remote_or_local(worktree, reporter)?;
        worktree.ensure_clean(&self.git)?;

        let check_message = push_check_message(local_event_files);
        emit_progress(reporter, ProgressKind::Info, check_message)?;
        let copied_files = self.copy_files_into_worktree(worktree.path(), local_files)?;
        if copied_files > 0 {
            emit_progress(reporter, ProgressKind::Info, copied_message(copied_files))?;
        }
        let stage_paths = stage_paths(worktree.path());
        if stage_paths.is_empty() {
            let message = "remote knots already includes the local events";
            emit_progress(reporter, ProgressKind::Success, message)?;
            let summary = unpushed_summary(local_event_files, copied_files);
            return Ok(PushAttemptResult::AlreadySynced(summary));
        }

        self.git.add_paths(worktree.path(), &stage_paths)?;

        if !self.git.has_staged_changes(worktree.path(), &stage_paths)? {
            let message = "remote knots already includes the local events";
            emit_progress(reporter, ProgressKind::Success, message)?;
            let summary = unpushed_summary(local_event_files, copied_files);
            return Ok(PushAttemptResult::AlreadySynced(summary));
        }

        emit_progress(reporter, ProgressKind::Info, "creating a publish commit")?;
        let commit = self
            .git
            .commit(worktree.path(), "knots: publish local events")?;

        let push_message = format!("pushing knots data to {}", worktree.remote_display());
        emit_progress(reporter, ProgressKind::Info, push_message)?;
        let push_refspec = worktree.push_refspec();
        let push_result = self
            .git
            .push_refspec(worktree.path(), worktree.remote(), &push_refspec);
        match push_result {
            Ok(()) => {
                let message = format!("push complete at {}", short_commit(&commit));
                emit_progress(reporter, ProgressKind::Success, message)?;
                Ok(PushAttemptResult::Success(PushSummary {
                    local_event_files,
                    copied_files,
                    committed: true,
                    pushed: true,
                    commit: Some(commit),
                }))
            }
            Err(err) if err.is_non_fast_forward() => Ok(PushAttemptResult::Retry(err)),
            Err(err) if err.is_ref_policy_rejection() => Err(err),
            Err(err) => Err(err),
        }
    }

    fn sync_service(&self) -> SyncService<'a> {
        SyncService::with_store_paths(self.conn, self.repo_root.clone(), self.store_paths.clone())
    }

    pub fn sync(&self) -> Result<ReplicationSummary, SyncError> {
        let mut reporter = None;
        self.sync_with_progress(&mut reporter)
    }

    pub fn sync_with_progress(
        &self,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<ReplicationSummary, SyncError> {
        let push = self.push_with_progress(reporter)?;
        let pull = self.pull_with_progress(reporter)?;
        Ok(ReplicationSummary { push, pull })
    }

    /// Like `sync_with_progress` but returns `Deferred` instead of erroring
    /// when active leases exist.
    pub fn sync_or_defer_with_progress(
        &self,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<SyncOutcome, SyncError> {
        let count = crate::db::count_active_leases(self.conn)?;
        if count > 0 {
            return Ok(SyncOutcome::Deferred {
                active_leases: count,
            });
        }
        let summary = self.sync_with_progress(reporter)?;
        Ok(SyncOutcome::Completed(summary))
    }

    pub fn count_unpushed_event_files(&self) -> Result<u64, SyncError> {
        let worktree = KnotsWorktree::with_store_paths(self.repo_root.clone(), &self.store_paths);
        worktree.ensure_exists(&self.git)?;
        let mut reporter = None;
        self.reset_worktree_to_remote_or_local(&worktree, &mut reporter)?;
        worktree.ensure_clean(&self.git)?;

        let local_files = self.collect_local_event_files()?;
        let mut unpushed = 0u64;
        for relative in local_files {
            if self.event_file_missing_or_changed(worktree.path(), &relative)? {
                unpushed += 1;
            }
        }
        Ok(unpushed)
    }

    fn reset_worktree_to_remote_or_local(
        &self,
        worktree: &KnotsWorktree,
        reporter: &mut Option<&mut dyn ProgressReporter>,
    ) -> Result<(), SyncError> {
        let refresh_message = format!(
            "refreshing knots worktree from {}",
            worktree.remote_display()
        );
        emit_progress(reporter, ProgressKind::Info, refresh_message)?;
        let fetch_limit_kb = crate::db::get_sync_fetch_blob_limit_kb(self.conn)?;
        let fetch_result = self.fetch_worktree_refspec(worktree, fetch_limit_kb);
        match fetch_result {
            Ok(()) => {
                let reset_message =
                    format!("resetting knots worktree to {}", worktree.tracking_rev());
                emit_progress(reporter, ProgressKind::Info, reset_message)?;
                let tracking_rev = worktree.tracking_rev();
                let head = self.git.rev_parse(&self.repo_root, &tracking_rev)?;
                self.git.reset_hard(worktree.path(), &head)?;
                Ok(())
            }
            Err(err) if err.is_missing_remote() || err.is_unknown_revision() => {
                let fallback_message = local_worktree_fallback_message(worktree);
                emit_progress(reporter, ProgressKind::Warn, fallback_message)?;
                let head = self.git.rev_parse(worktree.path(), "HEAD")?;
                self.git.reset_hard(worktree.path(), &head)?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn fetch_worktree_refspec(
        &self,
        worktree: &KnotsWorktree,
        fetch_limit_kb: Option<u64>,
    ) -> Result<(), SyncError> {
        let fetch_refspec = worktree.fetch_refspec();
        self.git.fetch_refspec_with_filter(
            &self.repo_root,
            worktree.remote(),
            &fetch_refspec,
            fetch_limit_kb,
        )
    }

    fn collect_local_event_files(&self) -> Result<Vec<PathBuf>, SyncError> {
        let mut files = Vec::new();
        for rel_root in ["index", "events", "snapshots"] {
            let root = self.store_paths.root.join(rel_root);
            if !root.exists() {
                continue;
            }
            let mut stack = vec![root];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir)? {
                    let path = entry?.path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    if path.extension().is_none_or(|ext| ext != "json") {
                        continue;
                    }
                    let relative = self.store_relative_event_path(&path)?;
                    files.push(Path::new(".knots").join(relative));
                }
            }
        }

        files.sort();
        Ok(files)
    }

    fn store_relative_event_path(&self, path: &Path) -> Result<PathBuf, SyncError> {
        path.strip_prefix(&self.store_paths.root)
            .map(Path::to_path_buf)
            .map_err(|err| SyncError::InvalidEvent {
                path: path.to_path_buf(),
                message: format!("failed to relativize event file: {}", err),
            })
    }

    fn copy_files_into_worktree(
        &self,
        worktree_root: &Path,
        relative_files: &[PathBuf],
    ) -> Result<u64, SyncError> {
        let mut copied = 0u64;
        for relative in relative_files {
            let src = self.local_store_file_path(relative)?;
            if !src.exists() {
                continue;
            }
            let dst = worktree_root.join(relative);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let src_bytes = std::fs::read(&src)?;
            if dst.exists() {
                let dst_bytes = std::fs::read(&dst)?;
                if dst_bytes == src_bytes {
                    continue;
                }
                return Err(SyncError::FileConflict {
                    path: relative.clone(),
                });
            }

            std::fs::write(&dst, src_bytes)?;
            copied += 1;
        }

        Ok(copied)
    }

    fn event_file_missing_or_changed(
        &self,
        worktree_root: &Path,
        relative_file: &Path,
    ) -> Result<bool, SyncError> {
        let src = self.local_store_file_path(relative_file)?;
        if !src.exists() {
            return Ok(false);
        }

        let dst = worktree_root.join(relative_file);
        let src_bytes = std::fs::read(&src)?;
        if !dst.exists() {
            return Ok(true);
        }
        let dst_bytes = std::fs::read(&dst)?;
        Ok(dst_bytes != src_bytes)
    }

    fn local_store_file_path(&self, relative_file: &Path) -> Result<PathBuf, SyncError> {
        let store_relative =
            relative_file
                .strip_prefix(".knots")
                .map_err(|err| SyncError::InvalidEvent {
                    path: relative_file.to_path_buf(),
                    message: format!("expected .knots-relative event path: {}", err),
                })?;
        Ok(self.store_paths.root.join(store_relative))
    }

    fn require_no_active_leases(&self) -> Result<(), SyncError> {
        if std::env::var_os("KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION").is_some() {
            return Ok(());
        }
        let count = crate::db::count_active_leases(self.conn)?;
        if count > 0 {
            return Err(SyncError::ActiveLeasesExist(count));
        }
        Ok(())
    }
}

fn push_retry_message(attempt: usize, max_attempts: usize) -> String {
    format!(
        "push was rejected; refreshing remote state and retrying ({}/{})",
        attempt + 2,
        max_attempts
    )
}

fn push_rejected_after_retries() -> SyncError {
    SyncError::MergeConflictEscalation {
        message: "push rejected as non-fast-forward after retries".to_string(),
    }
}

fn push_retries_exhausted() -> SyncError {
    SyncError::MergeConflictEscalation {
        message: "push retries exhausted".to_string(),
    }
}

fn push_check_message(local_event_files: u64) -> String {
    format!("checking {local_event_files} local knot file(s) against the publish worktree")
}

fn copied_message(copied_files: u64) -> String {
    format!("copied {copied_files} local knot file(s) into the publish worktree")
}

fn local_worktree_fallback_message(worktree: &KnotsWorktree) -> String {
    format!(
        "{} is unavailable; using local knots worktree state",
        worktree.remote_display()
    )
}

fn short_commit(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

fn unpushed_summary(local_event_files: u64, copied_files: u64) -> PushSummary {
    PushSummary {
        local_event_files,
        copied_files,
        ..PushSummary::default()
    }
}

fn stage_paths(worktree_root: &Path) -> Vec<&'static str> {
    let mut out = Vec::new();
    for path in [".knots/index", ".knots/events", ".knots/snapshots"] {
        if worktree_root.join(path).exists() {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "replication/tests_local_files.rs"]
mod tests_local_files;

#[cfg(test)]
#[path = "replication/tests_lease_block.rs"]
mod tests_lease_block;
#[cfg(test)]
#[path = "replication/tests_ref_policy.rs"]
mod tests_ref_policy;

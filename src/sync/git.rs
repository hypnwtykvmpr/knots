use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::SyncError;

#[derive(Debug, Clone, Default)]
pub struct GitAdapter;

impl GitAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn fetch_refspec_with_filter(
        &self,
        repo_root: &Path,
        remote: &str,
        refspec: &str,
        blob_limit_kb: Option<u64>,
    ) -> Result<(), SyncError> {
        let mut args = vec![
            "fetch".to_string(),
            "--no-tags".to_string(),
            "--prune".to_string(),
        ];
        if let Some(limit_kb) = blob_limit_kb {
            args.push(format!("--filter=blob:limit={}k", limit_kb));
        }
        args.push(remote.to_string());
        args.push(refspec.to_string());
        self.run_checked(repo_root, args)?;
        Ok(())
    }

    pub fn rev_parse(&self, cwd: &Path, rev: &str) -> Result<String, SyncError> {
        self.run_checked(cwd, vec!["rev-parse".to_string(), rev.to_string()])
    }

    pub fn reset_hard(&self, cwd: &Path, rev: &str) -> Result<(), SyncError> {
        self.run_checked(
            cwd,
            vec!["reset".to_string(), "--hard".to_string(), rev.to_string()],
        )?;
        Ok(())
    }

    pub fn status_clean(&self, cwd: &Path) -> Result<bool, SyncError> {
        let output = self.run_checked(
            cwd,
            vec![
                "status".to_string(),
                "--porcelain".to_string(),
                "-uno".to_string(),
            ],
        )?;
        Ok(output.trim().is_empty())
    }

    pub fn branch_exists(&self, cwd: &Path, branch: &str) -> Result<bool, SyncError> {
        let output = self.run_allow_failure(
            cwd,
            vec![
                "show-ref".to_string(),
                "--verify".to_string(),
                format!("refs/heads/{}", branch),
            ],
        )?;
        Ok(output.status.success())
    }

    pub fn current_branch(&self, cwd: &Path) -> Result<String, SyncError> {
        self.run_checked(
            cwd,
            vec![
                "rev-parse".to_string(),
                "--abbrev-ref".to_string(),
                "HEAD".to_string(),
            ],
        )
    }

    pub fn checkout_branch(&self, cwd: &Path, branch: &str) -> Result<(), SyncError> {
        self.run_checked(cwd, vec!["checkout".to_string(), branch.to_string()])?;
        Ok(())
    }

    pub fn worktree_add_existing_branch(
        &self,
        repo_root: &Path,
        worktree: &Path,
        branch: &str,
    ) -> Result<(), SyncError> {
        self.run_checked(
            repo_root,
            vec![
                "worktree".to_string(),
                "add".to_string(),
                "--force".to_string(),
                git_path(worktree),
                branch.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn worktree_add_new_branch(
        &self,
        repo_root: &Path,
        worktree: &Path,
        branch: &str,
    ) -> Result<(), SyncError> {
        self.run_checked(
            repo_root,
            vec![
                "worktree".to_string(),
                "add".to_string(),
                "-B".to_string(),
                branch.to_string(),
                git_path(worktree),
            ],
        )?;
        Ok(())
    }

    pub fn diff_name_only(
        &self,
        cwd: &Path,
        from: &str,
        to: &str,
        pathspec: &str,
    ) -> Result<Vec<PathBuf>, SyncError> {
        let stdout = self.run_checked(
            cwd,
            vec![
                "diff".to_string(),
                "--name-only".to_string(),
                "--diff-filter=AM".to_string(),
                format!("{}..{}", from, to),
                "--".to_string(),
                pathspec.to_string(),
            ],
        )?;
        Ok(parse_lines(&stdout))
    }

    pub fn add_paths(&self, cwd: &Path, paths: &[&str]) -> Result<(), SyncError> {
        let mut args = vec!["add".to_string(), "-f".to_string(), "--".to_string()];
        for path in paths {
            args.push((*path).to_string());
        }
        self.run_checked(cwd, args)?;
        Ok(())
    }

    pub fn has_staged_changes(&self, cwd: &Path, paths: &[&str]) -> Result<bool, SyncError> {
        let mut args = vec![
            "diff".to_string(),
            "--cached".to_string(),
            "--quiet".to_string(),
            "--".to_string(),
        ];
        for path in paths {
            args.push((*path).to_string());
        }
        let output = self.run_allow_failure(cwd, args.clone())?;
        match output.status.code() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(SyncError::GitCommandFailed {
                    command: display_command(cwd, &args),
                    code: output.status.code(),
                    stderr,
                })
            }
        }
    }

    pub fn commit(&self, cwd: &Path, message: &str) -> Result<String, SyncError> {
        self.run_checked(
            cwd,
            vec![
                "commit".to_string(),
                "--no-verify".to_string(),
                "--no-gpg-sign".to_string(),
                "-m".to_string(),
                message.to_string(),
            ],
        )?;
        self.rev_parse(cwd, "HEAD")
    }

    pub fn push_refspec(&self, cwd: &Path, remote: &str, refspec: &str) -> Result<(), SyncError> {
        self.run_checked(
            cwd,
            vec![
                "push".to_string(),
                "--no-verify".to_string(),
                remote.to_string(),
                refspec.to_string(),
            ],
        )?;
        Ok(())
    }

    fn run_checked(&self, cwd: &Path, args: Vec<String>) -> Result<String, SyncError> {
        let phase_name = trace_name(&args);
        let output =
            crate::trace::measure(&phase_name, || self.run_allow_failure(cwd, args.clone()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SyncError::GitCommandFailed {
                command: display_command(cwd, &args),
                code: output.status.code(),
                stderr,
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn run_allow_failure(&self, cwd: &Path, args: Vec<String>) -> Result<Output, SyncError> {
        let mut cmd = Command::new("git");
        // Synced event files are compared byte-for-byte against the local
        // store, so every adapter command must treat them as opaque bytes.
        // Without this, core.autocrlf=true (the Git for Windows default)
        // smudges LF to CRLF on checkout/merge and every later push fails
        // with a persistent FileConflict.
        cmd.arg("-C")
            .arg(repo_path_arg(cwd))
            .args(["-c", "core.autocrlf=false"])
            .args(&args);
        cmd.output().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                SyncError::GitUnavailable
            } else {
                SyncError::Io(err)
            }
        })
    }
}

fn trace_name(args: &[String]) -> String {
    match args.first().map(String::as_str) {
        Some("fetch") => "git_fetch".to_string(),
        Some("reset") => "git_reset".to_string(),
        Some("rev-parse") => "git_rev_parse".to_string(),
        Some("diff") => "git_diff".to_string(),
        Some("status") => "git_status".to_string(),
        Some("show-ref") => "git_show_ref".to_string(),
        Some("checkout") => "git_checkout".to_string(),
        Some("worktree") => "git_worktree".to_string(),
        Some("add") => "git_add".to_string(),
        Some("commit") => "git_commit".to_string(),
        Some("push") => "git_push".to_string(),
        Some(other) => format!("git_{other}"),
        None => "git".to_string(),
    }
}

fn parse_lines(value: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in value.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }
    out
}

pub(crate) fn git_path(path: &Path) -> String {
    strip_windows_verbatim_prefix(&path.to_string_lossy())
}

/// The `-C` repo path is passed through as OS-native bytes on Unix so
/// non-UTF-8 repo paths survive; only Windows needs the lossy round-trip,
/// where `\\?\` verbatim prefixes must be stripped for git.
fn repo_path_arg(cwd: &Path) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        std::ffi::OsString::from(git_path(cwd))
    }
    #[cfg(not(windows))]
    {
        cwd.as_os_str().to_os_string()
    }
}

pub(crate) fn strip_windows_verbatim_prefix(path: &str) -> String {
    #[cfg(windows)]
    {
        const VERBATIM: &str = r"\\?\";
        const UNC: &str = r"UNC\";
        if let Some(rest) = path.strip_prefix(VERBATIM) {
            if let Some(unc) = rest.strip_prefix(UNC) {
                return format!(r"\\{unc}");
            }
            return rest.to_string();
        }
    }
    path.to_string()
}

fn display_command(cwd: &Path, args: &[String]) -> String {
    format!("git -C {} {}", git_path(cwd), args.join(" "))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn unique_temp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("knots-git-adapter-{nanos}"));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    fn run_git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {args:?} should succeed");
    }

    #[test]
    fn adapter_commands_bypass_autocrlf_conversion() {
        let dir = unique_temp_dir();
        run_git(&dir, &["init", "-b", "main"]);
        run_git(&dir, &["config", "core.autocrlf", "true"]);
        run_git(&dir, &["config", "user.email", "test@example.invalid"]);
        run_git(&dir, &["config", "user.name", "Knots Test"]);

        let event = dir.join("event.json");
        std::fs::write(&event, b"{\n  \"id\": 1\n}\n").expect("event fixture should write");

        let adapter = super::GitAdapter::new();
        adapter
            .add_paths(&dir, &["event.json"])
            .expect("add should succeed");
        adapter.commit(&dir, "init").expect("commit should succeed");

        std::fs::remove_file(&event).expect("event file should be removable");
        adapter
            .reset_hard(&dir, "HEAD")
            .expect("reset should succeed");

        let restored = std::fs::read(&event).expect("event file should be restored");
        assert!(
            !restored.contains(&b'\r'),
            "checkout via the adapter must not smudge LF to CRLF"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn strips_windows_verbatim_paths_for_git() {
        if cfg!(windows) {
            assert_eq!(
                super::strip_windows_verbatim_prefix(r"\\?\C:\tmp\repo"),
                r"C:\tmp\repo"
            );
            assert_eq!(
                super::strip_windows_verbatim_prefix(r"\\?\UNC\server\share"),
                r"\\server\share"
            );
        } else {
            assert_eq!(
                super::strip_windows_verbatim_prefix("/tmp/repo"),
                "/tmp/repo"
            );
        }
    }
}

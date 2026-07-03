//! Shared helpers for CLI integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use uuid::Uuid;

pub fn unique_workspace(prefix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&path).expect("workspace should be creatable");
    path
}

pub fn run_git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn setup_repo(root: &Path) {
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "knots@example.com"]);
    run_git(root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# knots\n").expect("readme should be writable");
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(root, &["branch", "-M", "main"]);
}

pub fn knots_binary() -> PathBuf {
    let configured = PathBuf::from(env!("CARGO_BIN_EXE_knots"));
    if configured.is_absolute() && configured.exists() {
        return configured;
    }
    if configured.exists() {
        return std::fs::canonicalize(&configured).unwrap_or(configured);
    }
    let manifest_relative = Path::new(env!("CARGO_MANIFEST_DIR")).join(&configured);
    if manifest_relative.exists() {
        return std::fs::canonicalize(&manifest_relative).unwrap_or(manifest_relative);
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if !configured.is_absolute() {
            for ancestor in current_exe.ancestors().skip(1) {
                let candidate = ancestor.join(&configured);
                if candidate.exists() {
                    return std::fs::canonicalize(&candidate).unwrap_or(candidate);
                }
            }
        }

        if let Some(debug_dir) = current_exe.parent().and_then(|deps| deps.parent()) {
            for name in ["knots", "knots.exe"] {
                let candidate = debug_dir.join(name);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    configured
}

pub fn configure_coverage_env(command: &mut Command) {
    if let Some(profile_file) = std::env::var_os("LLVM_PROFILE_FILE") {
        let profile_file = PathBuf::from(profile_file);
        if let Some(parent) = profile_file.parent() {
            command.env(
                "LLVM_PROFILE_FILE",
                parent.join("knots-child-%p-%m.profraw"),
            );
        }
    }
}

pub fn run_knots(repo_root: &Path, db_path: &Path, args: &[&str]) -> Output {
    run_knots_with_path(repo_root, db_path, args, None)
}

pub fn run_knots_with_current_dir(
    current_dir: &Path,
    repo_root: &Path,
    db_path: Option<&Path>,
    args: &[&str],
) -> Output {
    let mut command = Command::new(knots_binary());
    command
        .current_dir(current_dir)
        .arg("-C")
        .arg(repo_root)
        .env("KNOTS_SKIP_DOCTOR_UPGRADE", "1");
    if let Some(db_path) = db_path {
        command.arg("--db").arg(db_path);
    }
    command.args(args);
    configure_coverage_env(&mut command);
    command.output().expect("knots command should run")
}

pub fn run_knots_with_path(
    repo_root: &Path,
    db_path: &Path,
    args: &[&str],
    path_override: Option<&Path>,
) -> Output {
    let mut command = Command::new(knots_binary());
    command
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--db")
        .arg(db_path)
        .env("KNOTS_SKIP_DOCTOR_UPGRADE", "1")
        .args(args);
    if let Some(path) = path_override {
        command.env("KNOTS_LOOM_BIN", path.join(loom_file_name()));
    }
    configure_coverage_env(&mut command);
    command.output().expect("knots command should run")
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success but failed.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure but command succeeded.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn parse_created_id(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .nth(1)
        .expect("created output should include knot id")
        .to_string()
}

const LOOM_COMPAT_BUNDLE: &str = r#"
[workflow]
name = "custom_flow"
version = 1
default_profile = "autopilot"

[states.ready_for_work]
display_name = "Ready for Work"
kind = "queue"

[states.work]
display_name = "Work"
kind = "action"
action_type = "produce"
executor = "agent"
prompt = "work"

[states.ready_for_review]
display_name = "Ready for Review"
kind = "queue"

[states.review]
display_name = "Review"
kind = "action"
action_type = "gate"
executor = "human"
prompt = "review"

[states.blocked]
display_name = "Blocked"
kind = "escape"

[states.deferred]
display_name = "Deferred"
kind = "escape"

[states.done]
display_name = "Done"
kind = "terminal"

[steps.work_step]
queue = "ready_for_work"
action = "work"

[steps.review_step]
queue = "ready_for_review"
action = "review"

[phases.main]
produce = "work_step"
gate = "review_step"

[profiles.autopilot]
phases = ["main"]
output = "remote_main"

[prompts.work]
accept = ["Working change"]
body = """
# Work

Perform the work.
"""

[prompts.work.success]
complete = "ready_for_review"

[prompts.work.failure]
blocked = "blocked"

[prompts.review]
accept = ["Reviewed change"]
body = """
# Review

Review the work.
"""

[prompts.review.success]
approved = "done"

[prompts.review.failure]
changes = "ready_for_work"
"#;

pub fn install_stub_loom(root: &Path) -> PathBuf {
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("bin dir should exist");
    let script = stub_loom_script();
    let loom = bin_dir.join(loom_file_name());
    std::fs::write(&loom, script).expect("loom script should write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&loom).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&loom, perms).expect("permissions");
    }
    bin_dir
}

pub fn stub_loom_script() -> String {
    #[cfg(windows)]
    {
        format!(
            "$ErrorActionPreference = 'Stop'\n\
             if ($args[0] -eq '--version') {{ 'loom 0.1.0'; exit 0 }}\n\
             if ($args[0] -eq 'init') {{\n\
               if (-not $args[1]) {{ exit 1 }}\n\
               New-Item -ItemType File -Path 'loom.toml' -Force | Out-Null\n\
               exit 0\n\
             }}\n\
             if ($args[0] -eq 'validate') {{ exit 0 }}\n\
             if ($args[0] -eq 'build') {{\n\
             @'\n\
{LOOM_COMPAT_BUNDLE}\n\
'@\n\
               exit 0\n\
             }}\n\
             [Console]::Error.WriteLine('unexpected args')\n\
             exit 1\n"
        )
    }
    #[cfg(not(windows))]
    {
        format!(
            "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'loom 0.1.0'; exit 0; fi\n\
         if [ \"$1\" = \"init\" ]; then test -n \"$2\" || exit 1; touch loom.toml; exit 0; fi\n\
         if [ \"$1\" = \"validate\" ]; then exit 0; fi\n\
         if [ \"$1\" = \"build\" ]; then\n\
           cat <<'EOF'\n\
{LOOM_COMPAT_BUNDLE}\n\
EOF\n\
           exit 0\n\
         fi\n\
         echo 'unexpected args' >&2\n\
         exit 1\n"
        )
    }
}

pub fn loom_file_name() -> &'static str {
    if cfg!(windows) {
        "loom.ps1"
    } else {
        "loom"
    }
}

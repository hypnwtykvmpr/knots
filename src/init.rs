use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::app::AppError;
use crate::db;
use crate::remote_init::{
    detect_beads_hooks, init_remote_knots_branch, remote_knots_ref_exists,
    uninit_remote_knots_branch, RemoteInitError,
};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_BOLD_GREEN: &str = "\x1b[1;32m";
const ANSI_BOLD_MAGENTA: &str = "\x1b[1;35m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_DIM: &str = "\x1b[2m";

const KNOTS_IGNORE_RULE: &str = "/.knots/";

pub(crate) fn init_all(repo_root: &Path, db_path: &str) -> Result<(), AppError> {
    print_banner("FIT TO BE TIED 🎉")?;
    progress("initializing local store")?;
    init_local_store(repo_root, db_path)?;
    progress_ok("local store initialized")?;
    warn_if_beads_hooks_present(repo_root)?;
    let config = crate::sync_ref::SyncRefConfig::for_repo(repo_root);
    if remote_knots_ref_exists(repo_root)? {
        progress(&format!(
            "found existing remote Knots ref {}",
            config.remote_display()
        ))?;
        progress("pulling knots from remote")?;
        pull_knots_from_remote(repo_root.to_path_buf(), db_path)?;
        progress_ok("knots pulled from remote")?;
    } else {
        progress(&format!(
            "initializing remote Knots ref {}",
            config.remote_display()
        ))?;
        progress_note("this can take a bit...")?;
        init_remote_knots_branch(repo_root)?;
        progress_ok("remote Knots ref initialized")?;
    }
    progress("installing sync hooks (post-merge)")?;
    match crate::git_hooks::install_hooks(repo_root) {
        Ok(_) => progress_ok("sync hooks installed")?,
        Err(err) => progress_warn(&format!("sync hook install failed: {err}"))?,
    }
    Ok(())
}

pub(crate) fn uninit_all(repo_root: &Path, db_path: &str) -> Result<(), AppError> {
    print_banner("UNTYING THE KNOT 🎉")?;
    progress("removing local store")?;
    uninit_local_store(repo_root, db_path)?;
    progress_ok("local store removed")?;
    progress("removing remote branch origin/knots")?;
    progress_note("this can take a bit...")?;
    match uninit_remote_knots_branch(repo_root, "origin", "knots") {
        Ok(true) => progress_ok("remote branch origin/knots removed")?,
        Ok(false) => progress_warn("remote branch origin/knots not present")?,
        Err(RemoteInitError::NotGitRepository) => {
            progress_warn("not a git repository; skipping remote branch cleanup")?;
        }
        Err(RemoteInitError::MissingRemote(_)) => {
            progress_warn("origin remote is not configured; skipping remote branch cleanup")?;
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

pub(crate) fn init_local_store(repo_root: &Path, db_path: &str) -> Result<(), AppError> {
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    progress(&format!("opening cache database at {db_path}"))?;
    let _ = db::open_connection(db_path)?;
    let store_root = crate::project::canonical_or_original(&store_root_for_db(db_path));
    let repo_root = crate::project::canonical_or_original(repo_root);
    let git_store_root = crate::project::canonical_or_original(&repo_root.join(".knots"));

    if store_root == git_store_root || store_root == repo_root || store_root.exists() {
        let workflow_root = if store_root == git_store_root {
            repo_root.as_path()
        } else {
            store_root.as_path()
        };
        progress("registering builtin workflows by knot type")?;
        crate::installed_workflows::ensure_builtin_workflows_registered(workflow_root)?;
    }
    if store_root == git_store_root {
        progress("ensuring gitignore includes .knots rule")?;
        ensure_knots_gitignore(&repo_root)?;
    }
    progress_ok("local store ready")?;
    Ok(())
}

fn pull_knots_from_remote(repo_root: PathBuf, db_path: &str) -> Result<(), AppError> {
    let app = crate::app::App::open(db_path, repo_root)?;
    let _ = app.pull()?;
    Ok(())
}

pub(crate) fn uninit_local_store(repo_root: &Path, db_path: &str) -> Result<(), AppError> {
    let store_root = store_root_for_db(db_path);
    if crate::project::canonical_or_original(&store_root)
        == crate::project::canonical_or_original(&repo_root.join(".knots"))
    {
        remove_gitignore_entries(repo_root)?;
    }
    remove_db_file(db_path)?;
    if store_root.exists() {
        std::fs::remove_dir_all(&store_root)?;
    }
    progress_ok("local store removed")?;
    Ok(())
}

fn progress(message: &str) -> Result<(), AppError> {
    println!("{ANSI_BOLD_CYAN}•{ANSI_RESET} {message}");
    io::stdout().flush()?;
    Ok(())
}

fn progress_ok(message: &str) -> Result<(), AppError> {
    println!("{ANSI_BOLD_GREEN}✓{ANSI_RESET} {message}");
    io::stdout().flush()?;
    Ok(())
}

fn progress_warn(message: &str) -> Result<(), AppError> {
    println!("{ANSI_BOLD_YELLOW}!{ANSI_RESET} {message}");
    io::stdout().flush()?;
    Ok(())
}

fn progress_note(message: &str) -> Result<(), AppError> {
    println!("{ANSI_DIM}{message}{ANSI_RESET}");
    io::stdout().flush()?;
    Ok(())
}

fn print_banner(title: &str) -> Result<(), AppError> {
    println!("{ANSI_BOLD_MAGENTA}{title}{ANSI_RESET}");
    println!("{ANSI_BOLD_CYAN}Welcome to Knots!{ANSI_RESET}");
    println!(
        "{ANSI_DIM}version {}{ANSI_RESET}",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    io::stdout().flush()?;
    Ok(())
}

fn warn_if_beads_hooks_present(repo_root: &Path) -> Result<(), AppError> {
    let report = detect_beads_hooks(repo_root);
    if report.is_empty() {
        return Ok(());
    }

    progress("found bd/beads hook-related setup in this repository")?;
    for hook in &report.hook_files {
        progress(&format!("  - hook: {}", hook.display()))?;
    }
    if report.has_beads_config {
        progress("  - git config section: [beads]")?;
    }

    progress("to disable bd/beads hooks and stop these push checks:")?;
    if !report.hook_files.is_empty() {
        for hook in &report.hook_files {
            progress(&format!("  {}", hook_removal_hint(hook)))?;
        }
    } else {
        progress("  (no hook files matched; likely hooks are configured elsewhere)")?;
    }
    if report.has_beads_config {
        progress("  git config --remove-section beads")?;
    }
    Ok(())
}

/// Platform-appropriate removal command for a hook file: PowerShell users do
/// not have `rm`, and spaced paths need quoting.
fn hook_removal_hint(path: &Path) -> String {
    #[cfg(windows)]
    {
        let quoted = path.display().to_string().replace('\'', "''");
        format!("Remove-Item -LiteralPath '{quoted}' -Force")
    }
    #[cfg(not(windows))]
    {
        format!("rm {}", path.display())
    }
}

pub(crate) fn ensure_knots_gitignore(repo_root: &Path) -> Result<(), AppError> {
    let path = repo_root.join(".gitignore");
    let contents = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    let has_ignore = contains_knots_ignore(&contents);
    if has_ignore {
        return Ok(());
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    if !contents.is_empty() && !contents.ends_with('\n') {
        writeln!(file)?;
    }
    if !has_ignore {
        writeln!(file, "{}", KNOTS_IGNORE_RULE)?;
    }
    Ok(())
}

fn remove_gitignore_entries(repo_root: &Path) -> Result<(), AppError> {
    let path = repo_root.join(".gitignore");
    if !path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(&path)?;
    let filtered: Vec<&str> = contents
        .lines()
        .map(str::trim)
        .filter(|line| {
            let line = *line;
            !(line == KNOTS_IGNORE_RULE || line.is_empty())
        })
        .collect();

    if filtered.len() == contents.lines().count() {
        return Ok(());
    }

    let new_contents = format!("{}\n", filtered.join("\n"));
    std::fs::write(path, new_contents)?;
    Ok(())
}

fn remove_db_file(db_path: &str) -> Result<(), AppError> {
    let path = Path::new(db_path);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn store_root_for_db(db_path: &str) -> PathBuf {
    let path = Path::new(db_path);
    path.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

fn contains_knots_ignore(contents: &str) -> bool {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .any(|line| {
            matches!(
                line,
                "/.knots" | "/.knots/" | "/.knots/*" | ".knots" | ".knots/" | ".knots/*"
            )
        })
}

#[cfg(test)]
#[path = "init_tests_inline.rs"]
mod tests;

#[cfg(test)]
#[path = "init_tests_ext.rs"]
mod tests_ext;

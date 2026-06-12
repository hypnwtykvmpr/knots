use std::io;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct SelfUpdateOptions {
    pub version: Option<String>,
    pub repo: Option<String>,
    pub install_dir: Option<PathBuf>,
    pub script_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct SelfUninstallOptions {
    pub bin_path: Option<PathBuf>,
    pub remove_previous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallResult {
    pub binary_path: PathBuf,
    pub removed_previous: bool,
    pub removed_aliases: Vec<PathBuf>,
}

pub fn run_update(options: &SelfUpdateOptions) -> io::Result<()> {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(
            "script=$(mktemp) && trap 'rm -f \"$script\"' EXIT && \
             curl -fsSL \"$1\" >\"$script\" && sh \"$script\"",
        )
        .arg("knots-self-update")
        .arg(&options.script_url);
    apply_update_env(&mut command, options);

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "update installer failed with status {status}"
        )))
    }
}

pub fn run_uninstall(options: &SelfUninstallOptions) -> io::Result<UninstallResult> {
    let launch_path = resolve_binary_path(options.bin_path.clone())?;
    if std::fs::symlink_metadata(&launch_path).is_err() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("binary not found: {}", launch_path.display()),
        ));
    }
    let binary_path = canonical_binary_path(&launch_path)?;

    remove_file_if_present(&binary_path)?;
    let mut removed_aliases = Vec::new();
    for alias_path in alias_paths(&binary_path) {
        if alias_path == binary_path {
            continue;
        }
        if remove_file_if_present(&alias_path)? {
            removed_aliases.push(alias_path);
        }
    }

    let mut removed_previous = false;
    if options.remove_previous {
        for previous_path in previous_paths(&binary_path) {
            if remove_file_if_present(&previous_path)? {
                removed_previous = true;
            }
        }
    }

    Ok(UninstallResult {
        binary_path,
        removed_previous,
        removed_aliases,
    })
}

fn apply_update_env(command: &mut Command, options: &SelfUpdateOptions) {
    if let Some(version) = options.version.as_deref() {
        command.env("KNOTS_VERSION", version);
    }
    if let Some(repo) = options.repo.as_deref() {
        command.env("KNOTS_GITHUB_REPO", repo);
    }
    if let Some(install_dir) = options.install_dir.as_deref() {
        command.env("KNOTS_INSTALL_DIR", install_dir);
    }
}

fn update_install_dir(explicit: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let binary_path = canonical_binary_path(&resolve_binary_path(None)?)?;
    Ok(parent_dir(&binary_path).to_path_buf())
}

fn resolve_binary_path(explicit: Option<PathBuf>) -> io::Result<PathBuf> {
    match explicit {
        Some(path) => Ok(path),
        None => std::env::current_exe(),
    }
}

fn canonical_binary_path(path: &Path) -> io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(err) => Err(err),
    }
}

fn remove_file_if_present(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("expected file but found directory: {}", path.display()),
                ));
            }
            std::fs::remove_file(path)?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn alias_paths(binary_path: &Path) -> Vec<PathBuf> {
    let parent = parent_dir(binary_path);
    vec![parent.join("kno"), parent.join("knots")]
}

fn previous_paths(binary_path: &Path) -> Vec<PathBuf> {
    let parent = parent_dir(binary_path);
    vec![parent.join("kno.previous"), parent.join("knots.previous")]
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

pub fn maybe_run_self_command(
    command: &crate::cli::Commands,
    current_dir: &Path,
) -> Result<Option<String>, crate::app::AppError> {
    use crate::cli::Commands;

    match command {
        Commands::Upgrade(update_args) => {
            let install_dir = update_install_dir(update_args.install_dir.clone())?;
            run_update(&SelfUpdateOptions {
                version: update_args.version.clone(),
                repo: update_args.repo.clone(),
                install_dir: Some(install_dir.clone()),
                script_url: update_args.script_url.clone(),
            })?;
            Ok(Some(format_upgrade_summary(
                update_args.version.as_deref(),
                update_args.repo.as_deref(),
                Some(&install_dir),
                upgrade_hint_needed(current_dir),
            )))
        }
        Commands::Uninstall(uninstall_args) => {
            let result = run_uninstall(&SelfUninstallOptions {
                bin_path: uninstall_args.bin_path.clone(),
                remove_previous: uninstall_args.remove_previous,
            })?;
            let mut lines = vec![format!("removed {}", result.binary_path.display())];
            if result.removed_previous {
                lines.push("removed previous backups (kno.previous/knots.previous)".to_string());
            }
            Ok(Some(lines.join("\n")))
        }
        _ => Ok(None),
    }
}

fn upgrade_hint_needed(current_dir: &Path) -> bool {
    let Some(repo_root) = crate::project::find_git_root(current_dir) else {
        return true;
    };
    crate::git_hooks::check_hooks(&repo_root).status != crate::doctor::DoctorStatus::Pass
}

fn format_upgrade_summary(
    version: Option<&str>,
    repo: Option<&str>,
    install_dir: Option<&Path>,
    include_hint: bool,
) -> String {
    let mut fields = vec![("status", "updated kno binary".to_string())];
    if let Some(version) = version {
        fields.push(("version", version.to_string()));
    }
    if let Some(repo) = repo {
        fields.push(("repo", repo.to_string()));
    }
    if let Some(install_dir) = install_dir {
        fields.push(("install_dir", install_dir.display().to_string()));
    }
    if include_hint {
        fields.push((
            "hint",
            "run `kno doctor` to check for post-upgrade issues".to_string(),
        ));
    }
    format_titled_fields("Upgrade", &fields)
}

fn format_titled_fields(title: &str, fields: &[(&str, String)]) -> String {
    let label_width = fields
        .iter()
        .map(|(label, _)| label.len() + 1)
        .max()
        .unwrap_or(0);
    let mut lines = Vec::with_capacity(fields.len() + 1);
    lines.push(paint("1;36", title));
    for (label, value) in fields {
        let label = format!("{label}:");
        lines.push(format!(
            "{}  {}",
            paint("36", &format!("{label:>label_width$}")),
            value
        ));
    }
    lines.join("\n")
}

fn paint(code: &str, text: &str) -> String {
    if std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[cfg(test)]
#[path = "self_manage_tests.rs"]
mod tests;

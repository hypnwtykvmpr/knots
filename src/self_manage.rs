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
    pub deferred: bool,
}

pub fn run_update(options: &SelfUpdateOptions) -> io::Result<()> {
    run_update_impl(options)
}

#[cfg(not(target_os = "windows"))]
fn run_update_impl(options: &SelfUpdateOptions) -> io::Result<()> {
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

#[cfg(target_os = "windows")]
fn run_update_impl(options: &SelfUpdateOptions) -> io::Result<()> {
    let mut command = windows_update_command(options);
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "update installer failed with status {status}"
        )))
    }
}

#[cfg(target_os = "windows")]
fn windows_update_command(options: &SelfUpdateOptions) -> Command {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass"]);
    if let Some(script_path) = local_file_url_path(&options.script_url) {
        command.arg("-File").arg(script_path);
    } else {
        command
            .arg("-Command")
            .arg(
                "$ErrorActionPreference = 'Stop'; \
                 $script = Join-Path ([IO.Path]::GetTempPath()) \
                   ('knots-install-' + [IO.Path]::GetRandomFileName() + '.ps1'); \
                 try { \
                   Invoke-WebRequest -UseBasicParsing -Uri $args[0] -OutFile $script; \
                   & $script; \
                   $exit = $LASTEXITCODE; \
                   if ($null -ne $exit) { exit $exit } \
                 } finally { \
                   Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue \
                 }",
            )
            .arg(&options.script_url);
    }
    command.env("KNOTS_PARENT_PID", std::process::id().to_string());
    apply_update_env(&mut command, options);
    command
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

    #[cfg(target_os = "windows")]
    if is_current_executable(&binary_path)? {
        let deferred = plan_windows_deferred_uninstall(&binary_path, options.remove_previous)?;
        schedule_windows_deferred_removal(
            &deferred.result.binary_path,
            &deferred.result.removed_aliases,
            &deferred.previous_paths,
        )?;
        return Ok(deferred.result);
    }

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
        deferred: false,
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
        Ok(canonical) => Ok(preferred_binary_path(canonical)),
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

#[cfg(target_os = "windows")]
fn existing_file_paths<I>(paths: I) -> io::Result<Vec<PathBuf>>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut existing = Vec::new();
    for path in paths {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("expected file but found directory: {}", path.display()),
                    ));
                }
                existing.push(path);
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(existing)
}

#[cfg(target_os = "windows")]
struct WindowsDeferredUninstall {
    result: UninstallResult,
    previous_paths: Vec<PathBuf>,
}

#[cfg(target_os = "windows")]
fn plan_windows_deferred_uninstall(
    binary_path: &Path,
    remove_previous: bool,
) -> io::Result<WindowsDeferredUninstall> {
    let removed_aliases = existing_file_paths(
        alias_paths(binary_path)
            .into_iter()
            .filter(|path| path != binary_path),
    )?;
    let previous_paths = if remove_previous {
        existing_file_paths(previous_paths(binary_path))?
    } else {
        Vec::new()
    };
    let result = UninstallResult {
        binary_path: binary_path.to_path_buf(),
        removed_previous: !previous_paths.is_empty(),
        removed_aliases,
        deferred: true,
    };
    Ok(WindowsDeferredUninstall {
        result,
        previous_paths,
    })
}

#[cfg(target_os = "windows")]
fn is_current_executable(path: &Path) -> io::Result<bool> {
    let current = canonical_binary_path(&resolve_binary_path(None)?)?;
    Ok(current == path)
}

#[cfg(target_os = "windows")]
fn schedule_windows_deferred_removal(
    binary_path: &Path,
    aliases: &[PathBuf],
    previous_paths: &[PathBuf],
) -> io::Result<()> {
    let mut command =
        windows_deferred_removal_command(std::process::id(), binary_path, aliases, previous_paths);
    command.spawn().map(|_| ())
}

#[cfg(target_os = "windows")]
fn windows_deferred_removal_command(
    parent_pid: u32,
    binary_path: &Path,
    aliases: &[PathBuf],
    previous_paths: &[PathBuf],
) -> Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = "\
$ErrorActionPreference = 'SilentlyContinue'; \
$parent = [int]$args[0]; \
Wait-Process -Id $parent -ErrorAction SilentlyContinue; \
Start-Sleep -Milliseconds 300; \
for ($i = 1; $i -lt $args.Count; $i++) { \
  Remove-Item -LiteralPath $args[$i] -Force -ErrorAction SilentlyContinue \
}";
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .arg(parent_pid.to_string())
        .arg(binary_path);
    for path in aliases.iter().chain(previous_paths.iter()) {
        command.arg(path);
    }
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn alias_paths(binary_path: &Path) -> Vec<PathBuf> {
    let parent = parent_dir(binary_path);
    let mut paths = vec![
        parent.join(executable_file_name("kno")),
        parent.join(executable_file_name("knots")),
    ];
    #[cfg(target_os = "windows")]
    {
        paths.push(parent.join("kno"));
        paths.push(parent.join("knots"));
    }
    paths
}

fn previous_paths(binary_path: &Path) -> Vec<PathBuf> {
    let parent = parent_dir(binary_path);
    let mut paths = vec![
        parent.join(previous_file_name("kno")),
        parent.join(previous_file_name("knots")),
    ];
    #[cfg(target_os = "windows")]
    {
        paths.push(parent.join("kno.previous"));
        paths.push(parent.join("knots.previous"));
    }
    paths
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

#[cfg(target_os = "windows")]
fn executable_file_name(stem: &str) -> String {
    format!("{stem}.exe")
}

#[cfg(not(target_os = "windows"))]
fn executable_file_name(stem: &str) -> String {
    stem.to_string()
}

#[cfg(target_os = "windows")]
fn previous_file_name(stem: &str) -> String {
    format!("{stem}.previous.exe")
}

#[cfg(not(target_os = "windows"))]
fn previous_file_name(stem: &str) -> String {
    format!("{stem}.previous")
}

#[cfg(target_os = "windows")]
fn preferred_binary_path(path: PathBuf) -> PathBuf {
    let is_kno_alias = path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case(&executable_file_name("kno"))
                || name.eq_ignore_ascii_case("kno")
        });
    if is_kno_alias {
        let parent = parent_dir(&path);
        let sibling = parent.join(executable_file_name("knots"));
        if std::fs::symlink_metadata(&sibling).is_ok() {
            return sibling;
        }
        let legacy_sibling = parent.join("knots");
        if std::fs::symlink_metadata(&legacy_sibling).is_ok() {
            return legacy_sibling;
        }
    }
    path
}

#[cfg(not(target_os = "windows"))]
fn preferred_binary_path(path: PathBuf) -> PathBuf {
    path
}

#[cfg(target_os = "windows")]
fn local_file_url_path(url: &str) -> Option<PathBuf> {
    let raw = url.strip_prefix("file://")?;
    if raw.len() >= 3 && raw.as_bytes()[0] == b'/' && raw.as_bytes()[2] == b':' {
        return Some(PathBuf::from(&raw[1..]));
    }
    Some(PathBuf::from(raw))
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
            let action = if result.deferred {
                "scheduled removal of"
            } else {
                "removed"
            };
            let mut lines = vec![format!("{action} {}", result.binary_path.display())];
            if result.removed_previous && result.deferred {
                lines.push(
                    "scheduled removal of previous backups (kno.previous/knots.previous)"
                        .to_string(),
                );
            } else if result.removed_previous {
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

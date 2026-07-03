use std::ffi::OsStr;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn command_for_program(program: impl AsRef<OsStr>) -> Command {
    let program = program.as_ref();
    #[cfg(windows)]
    {
        let resolved = resolve_windows_program(program);
        if is_powershell_script(&resolved) {
            let mut command = Command::new("powershell.exe");
            command
                .arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-File")
                .arg(resolved);
            return command;
        }
        Command::new(resolved)
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

#[cfg(windows)]
fn resolve_windows_program(program: &OsStr) -> OsString {
    let path = Path::new(program);
    if is_explicit_path(path) {
        return program.to_os_string();
    }
    find_windows_path_program(program)
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| program.to_os_string())
}

#[cfg(windows)]
fn is_explicit_path(path: &Path) -> bool {
    path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || path.components().count() > 1
}

#[cfg(windows)]
fn find_windows_path_program(program: &OsStr) -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")?;
    let extensions = windows_path_extensions();
    for dir in std::env::split_paths(&path_dirs) {
        let base = dir.join(program);
        if base.is_file() {
            return Some(base);
        }
        for extension in &extensions {
            let candidate = PathBuf::from(format!("{}{}", base.display(), extension));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_path_extensions() -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD;.PS1".to_string());
    let mut extensions = raw
        .split(';')
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with('.') {
                value.to_string()
            } else {
                format!(".{value}")
            }
        })
        .collect::<Vec<_>>();
    if !extensions
        .iter()
        .any(|value| value.eq_ignore_ascii_case(".ps1"))
    {
        extensions.push(".PS1".to_string());
    }
    extensions
}

#[cfg(windows)]
fn is_powershell_script(program: &OsStr) -> bool {
    Path::new(program)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
}

/// Absolute path to Windows PowerShell, resolved via %SystemRoot% so spawns
/// stay reliable when PATH is unusual (or clobbered by parallel tests).
/// Falls back to plain PATH lookup when the well-known location is missing.
/// Unused by the kno-mcp binary, which shares this module.
#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn windows_powershell_exe() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| {
            Path::new(&root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

#[cfg(test)]
#[path = "native_command_tests.rs"]
mod tests;

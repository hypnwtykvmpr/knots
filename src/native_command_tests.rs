#[cfg(windows)]
use std::sync::{Mutex, OnceLock};

#[cfg(windows)]
use super::*;

#[cfg(windows)]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(windows)]
fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

#[cfg(windows)]
#[test]
fn command_for_program_wraps_path_resolved_powershell_shim() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let dir = std::env::temp_dir().join(format!("knots-native-command-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("temp dir should exist");
    let shim = dir.join("loom.ps1");
    std::fs::write(&shim, "exit 0").expect("shim should write");
    let old_path = std::env::var_os("PATH");
    let old_pathext = std::env::var_os("PATHEXT");

    std::env::set_var("PATH", &dir);
    std::env::set_var("PATHEXT", ".EXE;.CMD");
    let command = command_for_program("loom");
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(command.get_program(), OsStr::new("powershell.exe"));
    assert!(args.iter().any(|arg| arg == "-File"));
    assert!(args
        .iter()
        .any(|arg| arg.to_ascii_lowercase().ends_with("loom.ps1")));

    restore_env("PATH", old_path);
    restore_env("PATHEXT", old_pathext);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn command_for_program_leaves_explicit_non_ps1_programs_native() {
    let program = PathBuf::from("tool.cmd");
    let command = command_for_program(&program);
    assert_eq!(command.get_program(), program.as_os_str());
}

#[cfg(windows)]
#[test]
fn command_for_program_accepts_exact_path_entry_without_extension() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let dir = std::env::temp_dir().join(format!("knots-native-command-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("temp dir should exist");
    let tool = dir.join("tool");
    std::fs::write(&tool, "fixture").expect("tool fixture should write");
    let old_path = std::env::var_os("PATH");

    std::env::set_var("PATH", &dir);
    let command = command_for_program("tool");
    assert_eq!(command.get_program(), tool.as_os_str());

    restore_env("PATH", old_path);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn command_for_program_falls_back_when_path_lookup_misses() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let dir = std::env::temp_dir().join(format!("knots-native-command-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("temp dir should exist");
    let old_path = std::env::var_os("PATH");

    std::env::set_var("PATH", &dir);
    let command = command_for_program("missing-tool");
    assert_eq!(command.get_program(), OsStr::new("missing-tool"));

    restore_env("PATH", old_path);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn windows_path_extensions_normalizes_bare_extensions() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let old_pathext = std::env::var_os("PATHEXT");

    std::env::set_var("PATHEXT", "EXE;PS1");
    let extensions = windows_path_extensions();
    assert!(extensions.iter().any(|extension| extension == ".EXE"));
    assert!(extensions.iter().any(|extension| extension == ".PS1"));

    restore_env("PATHEXT", old_pathext);
}

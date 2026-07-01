use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

fn unique_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("knots-upgrade-cli-{}-{name}", Uuid::now_v7()));
    std::fs::create_dir_all(&home).expect("temp home should be creatable");
    home
}

fn knots_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_knots"))
}

fn configure_coverage_env(command: &mut Command) {
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

fn upgrade_state_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Application Support")
            .join("knots")
            .join("upgrade-check.json")
    }

    #[cfg(target_os = "windows")]
    {
        home.join("AppData")
            .join("Roaming")
            .join("knots")
            .join("upgrade-check.json")
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join(".local")
            .join("share")
            .join("knots")
            .join("upgrade-check.json")
    }
}

fn run_help(home: &Path, curl_bin: Option<&Path>) -> std::process::Output {
    let mut cmd = Command::new(knots_binary());
    cmd.arg("--help")
        .env("HOME", home)
        .env_remove("KNOTS_SKIP_DOCTOR_UPGRADE")
        .env_remove("XDG_DATA_HOME")
        .env_remove("USERPROFILE")
        .env_remove("APPDATA")
        .env_remove("LOCALAPPDATA");
    if let Some(curl_bin) = curl_bin {
        cmd.env("KNOTS_CURL_BIN", curl_bin);
    } else {
        cmd.env_remove("KNOTS_CURL_BIN");
    }
    configure_coverage_env(&mut cmd);
    cmd.output().expect("knots help should run")
}

fn install_stub_curl(bin_dir: &Path, latest_tag: &str) -> PathBuf {
    std::fs::create_dir_all(bin_dir).expect("bin dir should be creatable");
    let script_path = bin_dir.join(curl_file_name());
    #[cfg(windows)]
    let script = format!("'{{\"tag_name\":\"{latest_tag}\"}}'\n");
    #[cfg(not(windows))]
    let script = format!(
        "#!/bin/sh\nprintf 'HTTP/2 302\\r\\nlocation: \
         https://github.com/acartine/knots/releases/tag/{latest_tag}\\r\\n\\r\\n'\n"
    );
    std::fs::write(&script_path, script).expect("stub curl should be writable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(&script_path)
            .expect("stub curl metadata should exist")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).expect("stub curl should be executable");
    }
    script_path
}

fn curl_file_name() -> &'static str {
    if cfg!(windows) {
        "curl.ps1"
    } else {
        "curl"
    }
}

#[test]
fn fresh_cached_upgrade_check_suppresses_banner() {
    let home = unique_home("fresh");
    let state_path = upgrade_state_path(&home);
    std::fs::create_dir_all(state_path.parent().expect("state dir should exist"))
        .expect("state dir should be creatable");
    std::fs::write(
        &state_path,
        r#"{"last_checked_unix_secs":9223372036854775807}"#,
    )
    .expect("state file should be writable");

    let output = run_help(&home, None);
    assert!(output.status.success(), "help command should succeed");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Upgrade available"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Commands:"));

    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn stale_upgrade_check_prints_banner_when_stub_reports_newer_version() {
    let home = unique_home("stale");
    let bin_dir = home.join("bin");
    let curl_bin = install_stub_curl(&bin_dir, "v9.9.9");

    let output = run_help(&home, Some(&curl_bin));
    assert!(output.status.success(), "help command should succeed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "Upgrade available: v{} -> v9.9.9",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        stderr.contains(&expected),
        "stderr should contain banner: {stderr}"
    );
    assert!(stderr.contains("kno upgrade"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Commands:"));

    let _ = std::fs::remove_dir_all(home);
}

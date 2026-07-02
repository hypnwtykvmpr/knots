#[cfg(unix)]
use super::canonical_binary_path;
#[cfg(windows)]
use super::{
    existing_file_paths, local_file_url_path, plan_windows_deferred_uninstall,
    windows_deferred_removal_command, windows_update_command,
};
use super::{
    format_titled_fields, format_upgrade_summary, paint, parent_dir, remove_file_if_present,
    resolve_binary_path, run_uninstall, run_update, update_install_dir, upgrade_hint_needed,
    SelfUninstallOptions, SelfUpdateOptions,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
fn symlink_file(src: &Path, dst: &Path) {
    std::os::unix::fs::symlink(src, dst).expect("symlink should be created");
}

#[cfg(windows)]
fn symlink_file(src: &Path, dst: &Path) {
    std::fs::hard_link(src, dst).expect("hard link should be created");
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("knots-self-manage-{nanos}"));
    std::fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn uninstall_removes_binary_and_previous_when_requested() {
    let dir = unique_temp_dir();
    let binary = dir.join("knots");
    let alias = dir.join("kno");
    let previous = dir.join("kno.previous");
    let legacy_previous = dir.join("knots.previous");
    std::fs::write(&binary, b"bin").expect("binary fixture should be written");
    symlink_file(&binary, &alias);
    std::fs::write(&previous, b"bin").expect("previous fixture should be written");
    std::fs::write(&legacy_previous, b"bin").expect("legacy previous fixture should be written");

    let result = run_uninstall(&SelfUninstallOptions {
        bin_path: Some(alias.clone()),
        remove_previous: true,
    })
    .expect("uninstall should succeed");

    assert_eq!(
        result
            .binary_path
            .file_name()
            .and_then(|value| value.to_str()),
        Some("knots")
    );
    assert!(result.removed_previous);
    assert!(!result.deferred);
    assert_eq!(result.removed_aliases.len(), 1);
    assert_eq!(
        result.removed_aliases[0]
            .file_name()
            .and_then(|value| value.to_str()),
        Some("kno")
    );
    assert!(!result.binary_path.exists());
    assert!(!alias.exists());
    assert!(!previous.exists());
    assert!(!legacy_previous.exists());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn uninstall_keeps_previous_without_flag() {
    let dir = unique_temp_dir();
    let binary = dir.join("knots");
    let alias = dir.join("kno");
    let previous = dir.join("kno.previous");
    let legacy_previous = dir.join("knots.previous");
    std::fs::write(&binary, b"bin").expect("binary fixture should be written");
    symlink_file(&binary, &alias);
    std::fs::write(&previous, b"bin").expect("previous fixture should be written");
    std::fs::write(&legacy_previous, b"bin").expect("legacy previous fixture should be written");

    let result = run_uninstall(&SelfUninstallOptions {
        bin_path: Some(binary),
        remove_previous: false,
    })
    .expect("uninstall should succeed");

    assert!(!result.binary_path.exists());
    assert!(!result.removed_previous);
    assert!(!result.deferred);
    assert!(!alias.exists());
    assert!(previous.exists());
    assert!(legacy_previous.exists());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn update_and_path_helpers_cover_error_paths() {
    let dir = unique_temp_dir();
    let installer = dir.join(installer_script_name("installer"));
    std::fs::write(&installer, "exit 1\n").expect("installer script fixture should be written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&installer)
            .expect("installer metadata should be readable")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&installer, perms)
            .expect("installer permissions should be writable");
    }

    let result = run_update(&SelfUpdateOptions {
        version: Some("v0.0.0-test".to_string()),
        repo: Some("acartine/knots".to_string()),
        install_dir: Some(dir.clone()),
        script_url: format!("file://{}", installer.display()),
    });
    assert!(result.is_err());

    let missing_installer = dir.join(installer_script_name("missing-installer"));
    let missing_result = run_update(&SelfUpdateOptions {
        version: None,
        repo: None,
        install_dir: Some(dir.clone()),
        script_url: format!("file://{}", missing_installer.display()),
    });
    assert!(missing_result.is_err());

    let current = resolve_binary_path(None).expect("current executable path should resolve");
    // Under coverage tools the test binary may be a temporary path that no
    // longer exists after instrumentation, so only assert the path resolved.
    assert!(!current.as_os_str().is_empty());

    let missing = dir.join("missing-knots-binary");
    let uninstall = run_uninstall(&SelfUninstallOptions {
        bin_path: Some(missing),
        remove_previous: false,
    });
    assert!(uninstall.is_err());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn update_install_dir_uses_explicit_or_running_binary_parent() {
    let dir = unique_temp_dir();
    assert_eq!(
        update_install_dir(Some(dir.clone())).expect("explicit install dir should resolve"),
        dir
    );

    let implicit = update_install_dir(None).expect("implicit install dir should resolve");
    assert!(!implicit.as_os_str().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parent_dir_falls_back_to_dot_for_bare_paths() {
    assert_eq!(parent_dir(Path::new("knots")), Path::new("."));
    assert_eq!(parent_dir(Path::new("/tmp/knots")), Path::new("/tmp"));
}

#[test]
fn canonicalize_and_remove_file_helpers_cover_directory_and_missing_paths() {
    let dir = unique_temp_dir();
    let fixture_dir = dir.join("directory-fixture");
    std::fs::create_dir_all(&fixture_dir).expect("fixture directory should be creatable");

    let removed_missing = remove_file_if_present(&dir.join("missing-file"))
        .expect("missing files should be treated as absent");
    assert!(!removed_missing);

    let err = remove_file_if_present(&fixture_dir).expect_err("directory should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let loop_path = dir.join("loop");
        symlink(&loop_path, &loop_path).expect("symlink loop fixture should be creatable");
        let loop_err = canonical_binary_path(&loop_path).expect_err("symlink loop should fail");
        assert_ne!(loop_err.kind(), std::io::ErrorKind::NotFound);

        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).expect("locked dir should be creatable");
        let mut perms = std::fs::metadata(&locked)
            .expect("locked dir metadata should be readable")
            .permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&locked, perms).expect("locked dir permissions should update");
        let denied_path = locked.join("missing");
        let denied =
            remove_file_if_present(&denied_path).expect_err("permission denied path should fail");
        assert_ne!(denied.kind(), std::io::ErrorKind::NotFound);

        let mut reset = std::fs::metadata(&locked)
            .expect("locked dir metadata should be readable")
            .permissions();
        reset.set_mode(0o755);
        std::fs::set_permissions(&locked, reset).expect("locked dir permissions should reset");
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn windows_deferred_uninstall_helpers_build_expected_command() {
    let dir = unique_temp_dir();
    let binary = dir.join("knots.exe");
    let alias = dir.join("kno.exe");
    let missing = dir.join("missing.exe");
    let previous = dir.join("knots.previous.exe");
    let directory = dir.join("directory");
    std::fs::write(&binary, b"bin").expect("binary should write");
    std::fs::write(&alias, b"alias").expect("alias should write");
    std::fs::write(&previous, b"previous").expect("previous should write");
    std::fs::create_dir_all(&directory).expect("directory should exist");

    let existing = existing_file_paths(vec![alias.clone(), missing])
        .expect("existing file collection should skip missing paths");
    assert_eq!(existing, vec![alias.clone()]);
    let err = existing_file_paths(vec![directory]).expect_err("directories should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    let command = windows_deferred_removal_command(
        123,
        &binary,
        std::slice::from_ref(&alias),
        std::slice::from_ref(&previous),
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        command.get_program(),
        std::ffi::OsStr::new("powershell.exe")
    );
    assert!(args.iter().any(|arg| arg == "-Command"));
    assert!(args.iter().any(|arg| arg == "123"));
    assert!(args.iter().any(|arg| arg.ends_with("knots.exe")));
    assert!(args.iter().any(|arg| arg.ends_with("kno.exe")));
    assert!(args.iter().any(|arg| arg.ends_with("knots.previous.exe")));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn windows_update_command_builds_remote_download_invocation() {
    let command = windows_update_command(&SelfUpdateOptions {
        version: Some("v9.9.9-test".to_string()),
        repo: Some("example/knots".to_string()),
        install_dir: Some(PathBuf::from("C:\\Tools\\Knots")),
        script_url: "https://example.invalid/install.ps1".to_string(),
    });
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        command.get_program(),
        std::ffi::OsStr::new("powershell.exe")
    );
    assert!(args.iter().any(|arg| arg == "-Command"));
    assert!(args.iter().any(|arg| arg.contains("Invoke-WebRequest")));
    assert!(args
        .iter()
        .any(|arg| arg == "https://example.invalid/install.ps1"));
    let version_env = command
        .get_envs()
        .find(|(key, _)| *key == std::ffi::OsStr::new("KNOTS_VERSION"))
        .and_then(|(_, value)| value);
    assert_eq!(version_env, Some(std::ffi::OsStr::new("v9.9.9-test")));
}

#[cfg(windows)]
#[test]
fn windows_deferred_uninstall_plan_collects_existing_files() {
    let dir = unique_temp_dir();
    let binary = dir.join("knots.exe");
    let alias = dir.join("kno.exe");
    let previous = dir.join("knots.previous.exe");
    std::fs::write(&binary, b"bin").expect("binary should write");
    std::fs::write(&alias, b"alias").expect("alias should write");
    std::fs::write(&previous, b"previous").expect("previous should write");

    let plan = plan_windows_deferred_uninstall(&binary, true)
        .expect("deferred uninstall plan should resolve");

    assert_eq!(plan.result.binary_path, binary);
    assert_eq!(plan.result.removed_aliases, vec![alias]);
    assert!(plan.result.removed_previous);
    assert!(plan.result.deferred);
    assert_eq!(plan.previous_paths, vec![previous]);

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn windows_local_file_url_path_accepts_drive_prefixed_urls() {
    assert_eq!(
        local_file_url_path("file:///C:/Tools/knots/install.ps1"),
        Some(PathBuf::from("C:/Tools/knots/install.ps1"))
    );
}

#[test]
fn upgrade_summary_right_aligns_labels_and_left_aligns_values() {
    std::env::set_var("NO_COLOR", "1");
    let install_dir = Path::new("/tmp/kno-test-install");
    let summary = format_upgrade_summary(
        Some("v1.2.3"),
        Some("acartine/knots"),
        Some(install_dir),
        true,
    );
    std::env::remove_var("NO_COLOR");
    let lines = summary.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "Upgrade");
    assert_eq!(lines[1], "     status:  updated kno binary");
    assert_eq!(lines[2], "    version:  v1.2.3");
    assert_eq!(lines[3], "       repo:  acartine/knots");
    assert_eq!(lines[4], "install_dir:  /tmp/kno-test-install");
    assert!(lines[5].contains("kno doctor"));
}

#[test]
fn upgrade_summary_omits_hint_when_not_needed() {
    let install_dir = Path::new("/tmp/kno-test-install");
    let summary = format_upgrade_summary(
        Some("v1.2.3"),
        Some("acartine/knots"),
        Some(install_dir),
        false,
    );
    assert!(!summary.contains("kno doctor"));
}

#[test]
fn upgrade_hint_needed_stays_enabled_outside_git_repo() {
    let dir = unique_temp_dir();
    assert!(upgrade_hint_needed(&dir));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn titled_fields_render_plain_text_when_color_is_disabled() {
    std::env::set_var("NO_COLOR", "1");
    let rendered = format_titled_fields("Upgrade", &[("status", "updated kno binary".to_string())]);
    std::env::remove_var("NO_COLOR");
    assert_eq!(rendered.lines().next(), Some("Upgrade"));
    assert!(!rendered.contains("\x1b["));
    assert!(rendered.contains("status:  updated kno binary"));
}

#[test]
fn paint_respects_no_color() {
    std::env::set_var("NO_COLOR", "1");
    let rendered = paint("1;36", "Upgrade");
    std::env::remove_var("NO_COLOR");
    assert_eq!(rendered, "Upgrade");
}

fn installer_script_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.ps1")
    } else {
        format!("{stem}.sh")
    }
}

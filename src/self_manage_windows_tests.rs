use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::SelfUpdateOptions;
use super::{
    deferred_removal_script, encoded_powershell_command, local_file_url_path, ps_single_quote,
    remote_update_script, windows_deferred_removal_command, windows_update_command,
};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("knots-self-windows-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn file_uri(path: &Path) -> String {
    format!("file:///{}", path.to_string_lossy().replace('\\', "/"))
}

fn exited_process_pid() -> u32 {
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit", "0"])
        .spawn()
        .expect("cmd should spawn");
    let pid = child.id();
    child.wait().expect("cmd should exit");
    pid
}

#[test]
fn ps_single_quote_doubles_embedded_quotes() {
    assert_eq!(ps_single_quote("plain"), "'plain'");
    assert_eq!(ps_single_quote("it's"), "'it''s'");
    assert_eq!(ps_single_quote("$args `tick\""), "'$args `tick\"'");
}

#[test]
fn base64_utf16le_encoding_matches_known_vectors() {
    // "hi" in UTF-16LE is 68 00 69 00.
    assert_eq!(super::encode_utf16le_base64("hi"), "aABpAA==");
    assert_eq!(super::base64_encode(b"f"), "Zg==");
    assert_eq!(super::base64_encode(b"fo"), "Zm8=");
    assert_eq!(super::base64_encode(b"foo"), "Zm9v");
    assert_eq!(super::base64_encode(b""), "");
}

#[test]
fn scripts_embed_values_as_quoted_literals_not_args() {
    let dir = PathBuf::from("C:\\space dir");
    let binary = dir.join("knots.exe");
    let alias = dir.join("kno's.exe");
    let script = deferred_removal_script(123, &binary, std::slice::from_ref(&alias), &[]);
    assert!(script.contains("Wait-Process -Id 123"));
    assert!(script.contains("-LiteralPath 'C:\\space dir\\knots.exe'"));
    assert!(script.contains("-LiteralPath 'C:\\space dir\\kno''s.exe'"));
    assert!(!script.contains("$args"));

    let update = remote_update_script("https://example.invalid/it's.ps1");
    assert!(update.contains("$url = 'https://example.invalid/it''s.ps1'"));
    assert!(update.contains("Invoke-WebRequest"));
    assert!(!update.contains("$args"));
}

#[test]
fn update_command_uses_encoded_command_for_remote_urls_and_file_for_local() {
    let remote = windows_update_command(&SelfUpdateOptions {
        version: None,
        repo: None,
        install_dir: None,
        script_url: "https://example.invalid/install.ps1".to_string(),
    });
    let remote_args = remote
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(remote_args.iter().any(|arg| arg == "-EncodedCommand"));
    assert!(!remote_args.iter().any(|arg| arg == "-Command"));

    let local = windows_update_command(&SelfUpdateOptions {
        version: None,
        repo: None,
        install_dir: None,
        script_url: "file:///C:/Tools/install.ps1".to_string(),
    });
    let local_args = local
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(local_args.iter().any(|arg| arg == "-File"));
}

#[test]
fn local_file_url_path_accepts_drive_prefixed_urls() {
    assert_eq!(
        local_file_url_path("file:///C:/Tools/knots/install.ps1"),
        Some(PathBuf::from("C:/Tools/knots/install.ps1"))
    );
    assert_eq!(local_file_url_path("https://example.invalid/x"), None);
}

#[test]
fn encoded_command_roundtrips_literals_through_real_powershell() {
    let dir = unique_temp_dir("roundtrip");
    let out = dir.join("out with spaces.txt");
    let tricky = "it's $args `backtick \"double\" & ; | % (100%)";
    let script = format!(
        "Set-Content -LiteralPath {path} -Value {value}",
        path = ps_single_quote(&out.to_string_lossy()),
        value = ps_single_quote(tricky)
    );
    let status = encoded_powershell_command(&script)
        .status()
        .expect("powershell should spawn");
    assert!(status.success(), "roundtrip script should succeed");
    let written = std::fs::read_to_string(&out).expect("output file should exist");
    assert_eq!(written.trim_end_matches(['\r', '\n']), tricky);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn deferred_removal_command_deletes_targets_after_parent_exits() {
    let dir = unique_temp_dir("deferred removal");
    let binary = dir.join("knots.exe");
    let alias = dir.join("kno.exe");
    let previous = dir.join("knots.previous.exe");
    for path in [&binary, &alias, &previous] {
        std::fs::write(path, b"bin").expect("fixture should write");
    }

    let status = windows_deferred_removal_command(
        exited_process_pid(),
        &binary,
        std::slice::from_ref(&alias),
        std::slice::from_ref(&previous),
    )
    .status()
    .expect("removal powershell should run");

    assert!(status.success(), "removal script should succeed");
    assert!(!binary.exists(), "binary should be deleted");
    assert!(!alias.exists(), "alias should be deleted");
    assert!(!previous.exists(), "previous backup should be deleted");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn remote_update_script_downloads_and_runs_installer_from_file_uri() {
    let dir = unique_temp_dir("remote-update");
    let marker = dir.join("marker.txt");
    let installer = dir.join("installer.ps1");
    let body = format!(
        "Set-Content -LiteralPath {marker} -Value 'installed'\nexit 0\n",
        marker = ps_single_quote(&marker.to_string_lossy())
    );
    std::fs::write(&installer, body).expect("installer fixture should write");

    let status = encoded_powershell_command(&remote_update_script(&file_uri(&installer)))
        .status()
        .expect("update powershell should run");

    assert!(status.success(), "update bootstrap should succeed");
    assert!(marker.exists(), "installer should have run");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn remote_update_script_propagates_installer_exit_code() {
    let dir = unique_temp_dir("remote-exit");
    let installer = dir.join("installer.ps1");
    std::fs::write(&installer, "exit 7\n").expect("installer fixture should write");

    let status = encoded_powershell_command(&remote_update_script(&file_uri(&installer)))
        .status()
        .expect("update powershell should run");

    assert_eq!(status.code(), Some(7), "installer exit code should surface");
    let _ = std::fs::remove_dir_all(dir);
}

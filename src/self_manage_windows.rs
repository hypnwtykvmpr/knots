//! Windows-native PowerShell invocations for self-update and deferred
//! self-uninstall.
//!
//! Runtime values (installer URL, parent PID, paths to delete) are embedded
//! in the script text as single-quoted PowerShell literals and the script is
//! passed via `-EncodedCommand`. `powershell.exe -Command` does NOT bind
//! trailing argv tokens to `$args` (only `-File` does), so passing values as
//! extra arguments silently breaks; encoding the full script avoids both that
//! and any quoting/injection hazards from user-controlled values.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::SelfUpdateOptions;

pub(super) fn windows_update_command(options: &SelfUpdateOptions) -> Command {
    let mut command = match local_file_url_path(&options.script_url) {
        Some(script_path) => {
            let mut command = base_powershell_command();
            command.arg("-File").arg(script_path);
            command
        }
        None => encoded_powershell_command(&remote_update_script(&options.script_url)),
    };
    command.env("KNOTS_PARENT_PID", std::process::id().to_string());
    super::apply_update_env(&mut command, options);
    command
}

pub(super) fn windows_deferred_removal_command(
    parent_pid: u32,
    binary_path: &Path,
    aliases: &[PathBuf],
    previous_paths: &[PathBuf],
) -> Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = deferred_removal_script(parent_pid, binary_path, aliases, previous_paths);
    let mut command = encoded_powershell_command(&script);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Bootstrap script for remote installers: download to a temp file, run it,
/// propagate its exit code, and always clean up the download.
pub(super) fn remote_update_script(url: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; \
         $url = {url}; \
         $script = Join-Path ([IO.Path]::GetTempPath()) \
           ('knots-install-' + [IO.Path]::GetRandomFileName() + '.ps1'); \
         try {{ \
           Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $script; \
           & $script; \
           $exit = $LASTEXITCODE; \
           if ($null -ne $exit) {{ exit $exit }} \
         }} finally {{ \
           Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue \
         }}",
        url = ps_single_quote(url)
    )
}

/// Watcher script that waits for the parent process to exit, then deletes the
/// binary, aliases, and previous backups. All paths are embedded as quoted
/// literals so the script needs no arguments at all.
pub(super) fn deferred_removal_script(
    parent_pid: u32,
    binary_path: &Path,
    aliases: &[PathBuf],
    previous_paths: &[PathBuf],
) -> String {
    let mut script = format!(
        "$ErrorActionPreference = 'SilentlyContinue'; \
         Wait-Process -Id {parent_pid} -ErrorAction SilentlyContinue; \
         Start-Sleep -Milliseconds 300;"
    );
    let targets = std::iter::once(binary_path).chain(
        aliases
            .iter()
            .chain(previous_paths.iter())
            .map(PathBuf::as_path),
    );
    for path in targets {
        let literal = ps_single_quote(&path.to_string_lossy());
        script.push_str(&format!(
            " Remove-Item -LiteralPath {literal} -Force -ErrorAction SilentlyContinue;"
        ));
    }
    script
}

fn base_powershell_command() -> Command {
    let mut command = Command::new(crate::native_command::windows_powershell_exe());
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass"]);
    command
}

pub(super) fn encoded_powershell_command(script: &str) -> Command {
    let mut command = base_powershell_command();
    command
        .arg("-EncodedCommand")
        .arg(encode_utf16le_base64(script));
    command
}

/// Escape a value as a PowerShell single-quoted string literal. Inside single
/// quotes PowerShell performs no interpolation; the only escape is doubling
/// embedded single quotes.
pub(super) fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// `-EncodedCommand` expects base64 of the UTF-16LE script text.
fn encode_utf16le_base64(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(ALPHABET[(triple >> 18) as usize & 63]));
        out.push(char::from(ALPHABET[(triple >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(ALPHABET[(triple >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(ALPHABET[triple as usize & 63])
        } else {
            '='
        });
    }
    out
}

pub(super) fn local_file_url_path(url: &str) -> Option<PathBuf> {
    let raw = url.strip_prefix("file://")?;
    if raw.len() >= 3 && raw.as_bytes()[0] == b'/' && raw.as_bytes()[2] == b':' {
        return Some(PathBuf::from(&raw[1..]));
    }
    Some(PathBuf::from(raw))
}

#[cfg(test)]
#[path = "self_manage_windows_tests.rs"]
mod tests;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap_complete::{generate, Shell};

pub fn generate_completions(shell: Shell, buf: &mut dyn Write) {
    let mut cmd = crate::cli::styled_command();
    if shell == Shell::Zsh {
        let mut raw = Vec::new();
        generate(shell, &mut cmd, "kno", &mut raw);
        let script = String::from_utf8_lossy(&raw);
        let grouped = group_zsh_toplevel_commands(&script);
        buf.write_all(grouped.as_bytes())
            .expect("write completions");
    } else {
        generate(shell, &mut cmd, "kno", buf);
    }
}

pub fn detect_current_shell() -> Option<Shell> {
    if let Ok(shell_var) = std::env::var("SHELL") {
        if let Some(shell) = shell_from_path(&shell_var) {
            return Some(shell);
        }
    }

    #[cfg(target_os = "windows")]
    {
        Some(Shell::PowerShell)
    }

    #[cfg(not(target_os = "windows"))]
    None
}

fn shell_from_path(path: &str) -> Option<Shell> {
    let basename = path.rsplit(['/', '\\']).next()?;
    let basename = basename.to_ascii_lowercase();
    let basename = basename.strip_suffix(".exe").unwrap_or(&basename);
    match basename {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "elvish" => Some(Shell::Elvish),
        "powershell" | "pwsh" => Some(Shell::PowerShell),
        _ => None,
    }
}

fn completions_install_path_for_home(shell: Shell, home: &std::path::Path) -> Option<PathBuf> {
    match shell {
        Shell::Bash => {
            let dir = home.join(".local/share/bash-completion/completions");
            Some(dir.join("kno"))
        }
        Shell::Zsh => {
            let dir = home.join(".config/knots/completions");
            Some(dir.join("kno.zsh"))
        }
        Shell::Fish => {
            let dir = home.join(".config/fish/completions");
            Some(dir.join("kno.fish"))
        }
        Shell::PowerShell => {
            let dir = powershell_dir(home).join("Completions");
            Some(dir.join("kno.ps1"))
        }
        _ => None,
    }
}

pub fn install_completions(shell: Shell) -> io::Result<PathBuf> {
    let home = crate::project::home_dir(None).map_err(io::Error::other)?;
    install_completions_to(shell, &home)
}

fn install_completions_to(shell: Shell, home: &std::path::Path) -> io::Result<PathBuf> {
    let path = completions_install_path_for_home(shell, home).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("no install path for {shell:?}"),
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut buf = Vec::new();
    generate_completions(shell, &mut buf);
    std::fs::write(&path, buf)?;

    if shell == Shell::Zsh {
        patch_zshrc(home, &path)?;
    }
    if shell == Shell::PowerShell {
        patch_powershell_profile(home, &path)?;
    }

    Ok(path)
}

fn patch_zshrc(home: &std::path::Path, completions_path: &std::path::Path) -> io::Result<()> {
    let zshrc = home.join(".zshrc");
    let source_line = format!("source \"{}\"", completions_path.display());

    if zshrc.exists() {
        let content = std::fs::read_to_string(&zshrc)?;
        if content.contains(&source_line) {
            return Ok(());
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc)?;
    writeln!(file)?;
    writeln!(file, "# kno shell completions")?;
    writeln!(file, "{source_line}")?;
    Ok(())
}

fn patch_powershell_profile(home: &Path, completions_path: &Path) -> io::Result<()> {
    let source_line = format!(". '{}'", escape_powershell_single_quoted(completions_path));

    for profile in powershell_profile_paths(home) {
        patch_powershell_profile_file(&profile, &source_line)?;
    }
    Ok(())
}

fn patch_powershell_profile_file(profile: &Path, source_line: &str) -> io::Result<()> {
    let (mut content, encoding) = if profile.exists() {
        let (content, encoding) = read_profile_text(profile)?;
        if content.contains(source_line) {
            return Ok(());
        }
        (content, encoding)
    } else {
        (String::new(), ProfileEncoding::Utf8)
    };

    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent)?;
    }
    append_profile_source(&mut content, source_line);
    write_profile_text(profile, &content, encoding)
}

fn append_profile_source(content: &mut String, source_line: &str) {
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str("# kno shell completions\n");
    content.push_str(source_line);
    content.push('\n');
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn read_profile_text(path: &Path) -> io::Result<(String, ProfileEncoding)> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Ok((decode_utf16(&bytes[2..], true)?, ProfileEncoding::Utf16Le));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Ok((decode_utf16(&bytes[2..], false)?, ProfileEncoding::Utf16Be));
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let text = String::from_utf8(bytes[3..].to_vec()).map_err(invalid_data)?;
        return Ok((text, ProfileEncoding::Utf8Bom));
    }
    String::from_utf8(bytes)
        .map(|text| (text, ProfileEncoding::Utf8))
        .map_err(invalid_data)
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> io::Result<String> {
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "UTF-16 profile has an odd byte count",
        ));
    }
    let words = chunks
        .map(|chunk| {
            let pair = [chunk[0], chunk[1]];
            if little_endian {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&words).map_err(invalid_data)
}

fn write_profile_text(path: &Path, content: &str, encoding: ProfileEncoding) -> io::Result<()> {
    let bytes = match encoding {
        ProfileEncoding::Utf8 => content.as_bytes().to_vec(),
        ProfileEncoding::Utf8Bom => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(content.as_bytes());
            bytes
        }
        ProfileEncoding::Utf16Le => encode_utf16(content, true),
        ProfileEncoding::Utf16Be => encode_utf16(content, false),
    };
    std::fs::write(path, bytes)
}

fn encode_utf16(content: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for word in content.encode_utf16() {
        let pair = if little_endian {
            word.to_le_bytes()
        } else {
            word.to_be_bytes()
        };
        bytes.extend_from_slice(&pair);
    }
    bytes
}

fn invalid_data(err: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

#[cfg(target_os = "windows")]
fn powershell_dir(home: &std::path::Path) -> PathBuf {
    windows_documents_dir(home).join("PowerShell")
}

/// Documents may be redirected away from `home\Documents` (OneDrive or
/// enterprise folder redirection), so ask the shell for the known folder.
/// The redirected path is honored only when it lives under the home being
/// resolved: synthetic homes (tests, env overrides) must never leak profile
/// writes into the real user's Documents.
#[cfg(target_os = "windows")]
fn windows_documents_dir(home: &std::path::Path) -> PathBuf {
    documents_dir_for(home, known_folder_documents().as_deref())
}

#[cfg(target_os = "windows")]
fn documents_dir_for(home: &std::path::Path, known: Option<&std::path::Path>) -> PathBuf {
    match known {
        Some(known) if known.starts_with(home) => known.to_path_buf(),
        _ => home.join("Documents"),
    }
}

/// Only successful resolutions are cached: a transient spawn failure (e.g.
/// process pressure or a hostile PATH) must not permanently degrade the
/// process to the unredirected layout.
#[cfg(target_os = "windows")]
fn known_folder_documents() -> Option<PathBuf> {
    static DOCUMENTS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    if let Some(cached) = DOCUMENTS.get() {
        return Some(cached.clone());
    }
    let resolved = query_known_folder_documents()?;
    Some(DOCUMENTS.get_or_init(|| resolved).clone())
}

#[cfg(target_os = "windows")]
fn query_known_folder_documents() -> Option<PathBuf> {
    let output = std::process::Command::new(crate::native_command::windows_powershell_exe())
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetFolderPath('MyDocuments')",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

#[cfg(not(target_os = "windows"))]
fn powershell_dir(home: &std::path::Path) -> PathBuf {
    home.join(".config").join("powershell")
}

fn powershell_profile_paths(home: &std::path::Path) -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut profiles = vec![powershell_dir(home).join("Microsoft.PowerShell_profile.ps1")];
    #[cfg(target_os = "windows")]
    profiles.push(
        windows_documents_dir(home)
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
    );
    profiles
}

fn escape_powershell_single_quoted(path: &std::path::Path) -> String {
    path.display().to_string().replace('\'', "''")
}

fn group_zsh_toplevel_commands(script: &str) -> String {
    let s = group_zsh_command_fn(script, "_kno_commands");
    group_zsh_command_fn(&s, "_kno__help_commands")
}

fn group_zsh_command_fn(script: &str, fn_name: &str) -> String {
    let guard = format!("(( $+functions[{fn_name}] )) ||");
    let Some(start) = script.find(&guard) else {
        return script.to_string();
    };
    let rest = &script[start..];
    let Some(end_rel) = rest.find("\n}\n") else {
        return script.to_string();
    };
    let block_end = start + end_rel + 2; // include the `}`

    let block = &script[start..block_end];
    let mut entries = Vec::new();

    for line in block.lines() {
        let trimmed = line.trim();
        let entry = trimmed.trim_end_matches(" \\");
        if entry.starts_with('\'') && entry.contains(':') {
            entries.push(entry.to_string());
        }
    }
    entries.sort_by_key(|entry| {
        entry
            .split_once(':')
            .map(|(name, _)| name)
            .unwrap_or(entry.as_str())
            .to_string()
    });

    let mut out = String::new();
    // Omit the (( $+functions[...] )) || guard so that sourcing the
    // file always installs the sorted definition over cached versions.
    out.push_str(&format!("{fn_name}() {{\n"));
    out.push_str("    local -a commands\n");
    out.push_str("    commands=(\n");
    for entry in &entries {
        out.push_str(&format!("{entry} \\\n"));
    }
    out.push_str("    )\n");
    out.push_str("    _describe -t commands 'kno commands' commands \"$@\"\n");
    out.push('}');

    format!("{}{}{}", &script[..start], out, &script[block_end..])
}

fn parse_shell(raw: &str) -> Option<Shell> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "elvish" => Some(Shell::Elvish),
        "powershell" | "pwsh" => Some(Shell::PowerShell),
        _ => None,
    }
}

pub fn run_completions_command(
    shell_arg: Option<&str>,
    install: bool,
) -> Result<(), crate::app::AppError> {
    run_completions_command_with_home(shell_arg, install, None)
}

pub(crate) fn run_completions_command_with_home(
    shell_arg: Option<&str>,
    install: bool,
    home_override: Option<&std::path::Path>,
) -> Result<(), crate::app::AppError> {
    let mut stdout = io::stdout().lock();
    run_completions_command_inner(shell_arg, install, home_override, &mut stdout)
}

pub(crate) fn run_completions_command_inner(
    shell_arg: Option<&str>,
    install: bool,
    home_override: Option<&std::path::Path>,
    out: &mut dyn Write,
) -> Result<(), crate::app::AppError> {
    let shell = if let Some(name) = shell_arg {
        parse_shell(name).ok_or_else(|| {
            crate::app::AppError::InvalidArgument(format!("unknown shell '{name}'"))
        })?
    } else {
        detect_current_shell().ok_or_else(|| {
            crate::app::AppError::InvalidArgument(
                "unable to detect shell from $SHELL; pass a shell name".to_string(),
            )
        })?
    };

    if install {
        let path = match home_override {
            Some(home) => install_completions_to(shell, home)?,
            None => install_completions(shell)?,
        };
        writeln!(out, "completions installed to {}", path.display())
            .map_err(crate::app::AppError::Io)?;
    } else {
        generate_completions(shell, out);
    }
    Ok(())
}

#[cfg(test)]
#[path = "completions_tests.rs"]
mod tests;

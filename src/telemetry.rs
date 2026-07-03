//! Opt-in local telemetry sink for dogfooding on a Windows workstation.
//!
//! Privacy contract (do not weaken without an explicit decision):
//! - OFF by default. Nothing is written unless `KNOTS_TELEMETRY_LOG` is set.
//! - Writes only to a local runtime path outside the repo (or an explicit
//!   path the operator chooses). Never committed; never network.
//! - Redacts argument VALUES by default: positional values, values attached
//!   to a flag (`--desc=secret`, `-dsecret`), and everything after the `--`
//!   end-of-options separator are stripped; only bare flags (`--json`, `-C`)
//!   and phase timings survive. Full args are logged solely when
//!   `KNOTS_TELEMETRY_ARGS=1` is also set — a second opt-in.
//! - Best-effort: any failure is swallowed so telemetry never affects the
//!   command's exit status or output.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const ENABLE_VAR: &str = "KNOTS_TELEMETRY_LOG";
const ARGS_VAR: &str = "KNOTS_TELEMETRY_ARGS";

pub struct TelemetryConfig {
    path: PathBuf,
    include_args: bool,
}

/// Resolve the sink from the environment, or `None` when telemetry is off.
pub fn from_env() -> Option<TelemetryConfig> {
    let raw = std::env::var_os(ENABLE_VAR)?;
    let raw = raw.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed.is_empty() || is_falsy(trimmed) {
        // Falsy values (0/false/off/no/disable) must DISABLE, never be
        // mistaken for a relative file path that writes into the cwd.
        return None;
    }
    let path = if is_truthy(trimmed) {
        default_log_path()?
    } else {
        PathBuf::from(trimmed)
    };
    let include_args = std::env::var_os(ARGS_VAR)
        .map(|value| is_truthy(value.to_string_lossy().trim()))
        .unwrap_or(false);
    Some(TelemetryConfig { path, include_args })
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

fn is_falsy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no" | "disable" | "disabled"
    )
}

/// Default runtime location, outside any repo working tree.
fn default_log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            // Fallback so an explicit opt-in is not silently dropped when
            // LOCALAPPDATA is unset (stripped service/CI environments).
            std::env::var_os("USERPROFILE")
                .map(|home| PathBuf::from(home).join("AppData").join("Local"))
        });
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));
    Some(base?.join("knots").join("telemetry").join("events.jsonl"))
}

pub struct SessionRecord<'a> {
    pub cmd: &'a str,
    pub args: &'a [String],
    pub total_ms: u128,
    pub phases: &'a [(String, u128, Option<String>)],
}

/// Append one JSONL record. Best-effort — errors are intentionally ignored.
pub fn append(config: &TelemetryConfig, record: &SessionRecord<'_>) {
    let _ = try_append(config, record);
}

fn try_append(config: &TelemetryConfig, record: &SessionRecord<'_>) -> std::io::Result<()> {
    if let Some(parent) = config.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut line = serde_json::to_string(&build_value(config, record))
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.path)?;
    // One write_all of the line-plus-newline: under O_APPEND a single write
    // is atomic for our small records, so concurrent kno processes cannot
    // interleave the body and newline into a torn JSONL line.
    file.write_all(line.as_bytes())
}

fn build_value(config: &TelemetryConfig, record: &SessionRecord<'_>) -> serde_json::Value {
    use serde_json::json;
    let phases = record
        .phases
        .iter()
        .map(|(name, ms, detail)| json!({ "name": name, "ms": ms, "detail": detail }))
        .collect::<Vec<_>>();
    json!({
        "schema": "kno.telemetry/1",
        "cmd": record.cmd,
        "total_ms": record.total_ms,
        "arg_count": record.args.len(),
        "args": redact_args(record.args, config.include_args),
        "phases": phases,
    })
}

const REDACTED: &str = "<redacted>";

/// Redact positional VALUES and any value ATTACHED to a flag, unless the
/// operator explicitly opted into full-arg logging. Only a bare flag with no
/// attached data is safe to keep: `--json`, `-C`. Attached forms
/// (`--desc=secret`, `-dsecret`) carry user content and must have their value
/// portion stripped — the flag heuristic `starts_with('-')` alone is not
/// enough, because clap accepts those as a single argv token.
///
/// Redaction is stateful: after the `--` end-of-options separator, clap treats
/// every remaining token as a positional value even when it looks like a flag,
/// so everything past `--` is redacted regardless of prefix.
fn redact_args(args: &[String], include_args: bool) -> Vec<String> {
    if include_args {
        return args.to_vec();
    }
    let mut positional_only = false;
    args.iter()
        .map(|arg| {
            if positional_only {
                return REDACTED.to_string();
            }
            if arg == "--" {
                positional_only = true;
                return arg.clone(); // preserve the literal separator
            }
            redact_one(arg)
        })
        .collect()
}

fn redact_one(arg: &str) -> String {
    if let Some(long) = arg.strip_prefix("--") {
        // `--flag=value` -> `--flag=<redacted>`; bare `--flag` kept as-is.
        return match long.split_once('=') {
            Some((flag, _)) => format!("--{flag}={REDACTED}"),
            None => arg.to_string(),
        };
    }
    if let Some(short) = arg.strip_prefix('-') {
        if short.is_empty() {
            return arg.to_string(); // a lone "-" (stdin convention)
        }
        // A single short flag (`-C`) is a bare flag; anything longer is
        // either an attached value (`-dsecret`) or bundled flags — redact
        // the trailing data either way, keeping only the first flag char.
        let mut chars = short.chars();
        let first = chars.next().expect("non-empty");
        return if chars.next().is_none() {
            arg.to_string()
        } else {
            format!("-{first}{REDACTED}")
        };
    }
    // Positional value.
    REDACTED.to_string()
}

/// Small adapter so callers can pass a `Duration` phase list.
pub fn phase_tuple(
    name: String,
    elapsed: Duration,
    detail: Option<String>,
) -> (String, u128, Option<String>) {
    (name, elapsed.as_millis(), detail)
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;

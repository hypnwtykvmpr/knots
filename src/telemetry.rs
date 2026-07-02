//! Opt-in local telemetry sink for dogfooding on a Windows workstation.
//!
//! Privacy contract (do not weaken without an explicit decision):
//! - OFF by default. Nothing is written unless `KNOTS_TELEMETRY_LOG` is set.
//! - Writes only to a local runtime path outside the repo (or an explicit
//!   path the operator chooses). Never committed; never network.
//! - Redacts argument VALUES by default. Only flags (tokens starting with
//!   `-`) and phase timings are recorded. Full args are logged solely when
//!   `KNOTS_TELEMETRY_ARGS=1` is also set — an explicit second opt-in.
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
    if trimmed.is_empty() {
        return None;
    }
    let path = match trimmed {
        "1" | "true" | "TRUE" | "on" | "ON" => default_log_path()?,
        explicit => PathBuf::from(explicit),
    };
    let include_args = std::env::var_os(ARGS_VAR)
        .map(|value| {
            let value = value.to_string_lossy();
            matches!(value.trim(), "1" | "true" | "TRUE" | "on" | "ON")
        })
        .unwrap_or(false);
    Some(TelemetryConfig { path, include_args })
}

/// Default runtime location, outside any repo working tree.
fn default_log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
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
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&build_value(config, record))
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.path)?;
    writeln!(file, "{line}")
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

/// Keep flags verbatim; replace positional VALUES with a typed placeholder
/// unless the operator explicitly opted into full-arg logging.
fn redact_args(args: &[String], include_args: bool) -> Vec<String> {
    if include_args {
        return args.to_vec();
    }
    args.iter()
        .map(|arg| {
            if arg.starts_with('-') {
                arg.clone()
            } else {
                "<redacted>".to_string()
            }
        })
        .collect()
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

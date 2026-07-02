use std::time::Duration;

use super::{append, from_env, redact_args, SessionRecord};
use crate::test_env::EnvVarGuard;

const ENABLE_VAR: &str = "KNOTS_TELEMETRY_LOG";
const ARGS_VAR: &str = "KNOTS_TELEMETRY_ARGS";

fn unique_log_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("knots-telemetry-{}", uuid::Uuid::now_v7()))
}

#[test]
fn from_env_is_off_by_default() {
    let env = EnvVarGuard::capture(&[ENABLE_VAR, ARGS_VAR]);
    env.remove(ENABLE_VAR);
    env.remove(ARGS_VAR);
    assert!(from_env().is_none(), "telemetry must be off when unset");

    env.set(ENABLE_VAR, "");
    assert!(
        from_env().is_none(),
        "empty value must not enable telemetry"
    );

    // Falsy values must DISABLE, not be treated as a relative log path.
    for falsy in ["0", "false", "off", "no", "FALSE", "Off", "disable"] {
        env.set(ENABLE_VAR, falsy);
        assert!(
            from_env().is_none(),
            "falsy value {falsy:?} must keep telemetry off"
        );
    }
}

#[test]
fn from_env_uses_explicit_path_and_arg_optin() {
    let env = EnvVarGuard::capture(&[ENABLE_VAR, ARGS_VAR]);
    let path = unique_log_path();
    env.set(ENABLE_VAR, &path);
    env.remove(ARGS_VAR);

    let config = from_env().expect("explicit path should enable telemetry");
    assert_eq!(config.path, path);
    assert!(!config.include_args, "args are redacted without opt-in");

    env.set(ARGS_VAR, "1");
    let config = from_env().expect("still enabled");
    assert!(config.include_args, "KNOTS_TELEMETRY_ARGS=1 opts into args");
}

#[test]
fn redact_keeps_flags_and_hides_positional_values() {
    let args = vec![
        "new".to_string(),
        "Secret knot title".to_string(),
        "--json".to_string(),
        "-d".to_string(),
        "private description".to_string(),
    ];

    let redacted = redact_args(&args, false);
    assert_eq!(
        redacted,
        vec![
            "<redacted>".to_string(),
            "<redacted>".to_string(),
            "--json".to_string(),
            "-d".to_string(),
            "<redacted>".to_string(),
        ]
    );
    assert!(!redacted.contains(&"Secret knot title".to_string()));
    assert!(!redacted.contains(&"private description".to_string()));

    let full = redact_args(&args, true);
    assert_eq!(full, args, "opt-in keeps everything verbatim");
}

#[test]
fn redact_strips_values_attached_to_flags() {
    // The bug the verification pass caught: attached values start with '-'
    // and must NOT be logged verbatim in the default-redaction path.
    let args = vec![
        "--desc=confidential PII".to_string(),
        "--acceptance=secret criteria".to_string(),
        "-dattached secret".to_string(),
        "--json".to_string(),
        "-C".to_string(),
        "-".to_string(),
    ];
    let redacted = redact_args(&args, false);
    assert_eq!(
        redacted,
        vec![
            "--desc=<redacted>".to_string(),
            "--acceptance=<redacted>".to_string(),
            "-d<redacted>".to_string(),
            "--json".to_string(),
            "-C".to_string(),
            "-".to_string(),
        ]
    );
    let joined = redacted.join(" ");
    assert!(!joined.contains("confidential"), "PII must not survive");
    assert!(!joined.contains("secret"), "secrets must not survive");
    assert!(
        !joined.contains("attached"),
        "attached value must not survive"
    );
}

#[test]
fn append_writes_redacted_jsonl_record() {
    let env = EnvVarGuard::capture(&[ENABLE_VAR, ARGS_VAR]);
    let path = unique_log_path().join("events.jsonl");
    env.set(ENABLE_VAR, &path);
    env.remove(ARGS_VAR);
    let config = from_env().expect("telemetry should be enabled");

    let phases = vec![("repo_lock".to_string(), 3u128, Some("acquired".to_string()))];
    let args = vec![
        "new".to_string(),
        "Private title".to_string(),
        "--json".to_string(),
    ];
    append(
        &config,
        &SessionRecord {
            cmd: "new",
            args: &args,
            total_ms: 42,
            phases: &phases,
        },
    );
    // Second record to prove append (not truncate).
    append(
        &config,
        &SessionRecord {
            cmd: "ls",
            args: &[],
            total_ms: 5,
            phases: &[],
        },
    );

    let contents = std::fs::read_to_string(&path).expect("telemetry file should exist");
    let lines = contents.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "records should append");

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("valid json line");
    assert_eq!(first["schema"], "kno.telemetry/1");
    assert_eq!(first["cmd"], "new");
    assert_eq!(first["total_ms"], 42);
    assert_eq!(first["arg_count"], 3);
    assert_eq!(first["phases"][0]["name"], "repo_lock");
    assert_eq!(first["phases"][0]["ms"], 3);
    // Privacy: the positional title must be redacted, flags preserved.
    assert!(
        !contents.contains("Private title"),
        "private arg values must never be written when args are redacted"
    );
    let args_json = first["args"].as_array().expect("args array");
    assert!(args_json.iter().any(|value| value == "--json"));
    assert!(args_json.iter().any(|value| value == "<redacted>"));

    let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn from_env_truthy_resolves_default_path_under_base_dir() {
    let env = EnvVarGuard::capture(&[
        ENABLE_VAR,
        ARGS_VAR,
        "LOCALAPPDATA",
        "USERPROFILE",
        "XDG_STATE_HOME",
        "HOME",
    ]);
    env.remove(ARGS_VAR);
    let base = unique_log_path();
    #[cfg(windows)]
    env.set("LOCALAPPDATA", &base);
    #[cfg(not(windows))]
    {
        env.set("XDG_STATE_HOME", &base);
        env.remove("HOME");
    }
    env.set(ENABLE_VAR, "1");

    let config = from_env().expect("truthy value enables the default path");
    assert!(
        config.path.starts_with(&base),
        "default log path should live under the base runtime dir"
    );
    assert!(config.path.ends_with("events.jsonl"));
}

#[cfg(windows)]
#[test]
fn default_path_falls_back_to_userprofile_when_localappdata_unset() {
    let env = EnvVarGuard::capture(&[ENABLE_VAR, ARGS_VAR, "LOCALAPPDATA", "USERPROFILE"]);
    env.remove(ARGS_VAR);
    env.remove("LOCALAPPDATA");
    let base = unique_log_path();
    env.set("USERPROFILE", &base);
    env.set(ENABLE_VAR, "1");

    let config = from_env().expect("USERPROFILE fallback should resolve");
    assert!(config.path.starts_with(base.join("AppData").join("Local")));
}

#[test]
fn phase_tuple_converts_duration_to_millis() {
    let (name, ms, detail) =
        super::phase_tuple("query".to_string(), Duration::from_millis(7), None);
    assert_eq!(name, "query");
    assert_eq!(ms, 7);
    assert_eq!(detail, None);
}

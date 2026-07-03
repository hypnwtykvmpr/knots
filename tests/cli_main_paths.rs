mod common;
use common::*;

use serde_json::Value;
use std::process::Command;

#[test]
fn toplevel_help_uses_custom_help_path() {
    let root = unique_workspace("knots-main-help");
    setup_repo(&root);

    let mut command = Command::new(knots_binary());
    command
        .current_dir(&root)
        .env("KNOTS_SKIP_DOCTOR_UPGRADE", "1");
    configure_coverage_env(&mut command);
    let output = command.output().expect("knots command should run");
    assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Common Commands:"), "stdout: {stdout}");
    assert!(stdout.contains("Other Commands:"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fsck_non_json_failure_prints_issue_rows() {
    let root = unique_workspace("knots-main-fsck-issues");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    let created = run_knots(
        &root,
        &db,
        &[
            "new",
            "Broken fsck input",
            "--profile",
            "autopilot",
            "--state",
            "idea",
        ],
    );
    assert_success(&created);

    let bad_file = root.join(".knots/index/bad-event.json");
    std::fs::create_dir_all(
        bad_file
            .parent()
            .expect("bad fsck file should always have a parent"),
    )
    .expect("index directory should be creatable");
    std::fs::write(&bad_file, "{ this is not valid json").expect("invalid fsck file should write");

    let fsck = run_knots(&root, &db, &["fsck"]);
    assert_failure(&fsck);
    let stdout = String::from_utf8_lossy(&fsck.stdout);
    let stderr = String::from_utf8_lossy(&fsck.stderr);
    assert!(stdout.contains("issues="), "stdout: {stdout}");
    assert!(stdout.contains("invalid JSON payload"), "stdout: {stdout}");
    assert!(stderr.contains("fsck found"), "stderr: {stderr}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ready_claim_peek_skill_terminal_and_rehydrate_missing_paths() {
    let root = unique_workspace("knots-main-branches");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    let ready_knot = run_knots(
        &root,
        &db,
        &[
            "new",
            "Peek candidate",
            "--profile",
            "autopilot",
            "--state",
            "ready_for_implementation",
        ],
    );
    assert_success(&ready_knot);
    let ready_id = parse_created_id(&ready_knot);

    let ready = run_knots(&root, &db, &["ready"]);
    assert_success(&ready);
    assert!(
        String::from_utf8_lossy(&ready.stdout).contains("Peek candidate"),
        "ready should include knot title"
    );

    let peek = run_knots(&root, &db, &["claim", &ready_id, "--peek"]);
    assert_success(&peek);

    let shown = run_knots(&root, &db, &["show", &ready_id, "--json"]);
    assert_success(&shown);
    let shown_json: Value = serde_json::from_slice(&shown.stdout).expect("show should return json");
    assert_eq!(shown_json["state"], "ready_for_implementation");

    let shipped = run_knots(
        &root,
        &db,
        &[
            "new",
            "Terminal skill",
            "--profile",
            "autopilot",
            "--state",
            "shipped",
        ],
    );
    assert_success(&shipped);
    let shipped_id = parse_created_id(&shipped);

    let skill_terminal = run_knots(&root, &db, &["skill", &shipped_id]);
    assert_failure(&skill_terminal);
    assert!(
        String::from_utf8_lossy(&skill_terminal.stderr).contains("no next state"),
        "terminal skill should report no next state"
    );

    let missing_rehydrate = run_knots(&root, &db, &["rehydrate", "missing-id"]);
    assert_failure(&missing_rehydrate);
    assert!(
        String::from_utf8_lossy(&missing_rehydrate.stderr).contains("not found"),
        "rehydrate missing should return not found"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn hooks_status_command_dispatches_through_main() {
    let root = unique_workspace("knots-main-hooks-dispatch");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    let output = run_knots(&root, &db, &["hooks", "status"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("missing"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn loom_compat_test_dispatches_through_main() {
    let root = unique_workspace("knots-main-loom-dispatch");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");
    let bin_dir = install_stub_loom(&root);

    let output = run_knots_with_path(
        &root,
        &db,
        &["loom", "compat-test", "--mode", "matrix"],
        Some(&bin_dir),
    );
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("loom compat-test custom_flow matrix"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("success -> ready_for_review"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("blocked -> blocked"), "stdout: {stdout}");

    let json = run_knots_with_path(
        &root,
        &db,
        &["loom", "compat-test", "--json"],
        Some(&bin_dir),
    );
    assert_success(&json);
    let parsed: Value = serde_json::from_slice(&json.stdout).expect("loom json should parse");
    assert_eq!(parsed["workflow_id"], "custom_flow");
    assert_eq!(parsed["mode"], "smoke");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn repo_root_flag_resolves_default_db_relative_to_repo() {
    let root = unique_workspace("knots-main-repo-root-db");
    let outside = unique_workspace("knots-main-repo-root-db-outside");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    let created = run_knots(
        &root,
        &db,
        &[
            "new",
            "Repo root default db",
            "--profile",
            "autopilot",
            "--state",
            "ready_for_implementation",
        ],
    );
    assert_success(&created);
    let id = parse_created_id(&created);

    let shown = run_knots_with_current_dir(&outside, &root, None, &["show", &id, "--json"]);
    assert_success(&shown);
    let parsed: Value = serde_json::from_slice(&shown.stdout).expect("show json should parse");
    assert_eq!(parsed["title"], "Repo root default db");
    let shown_id = parsed["id"].as_str().expect("show should return string id");
    assert!(
        shown_id.ends_with(&format!("-{id}")),
        "show id {shown_id} should end with created suffix {id}"
    );

    let _ = std::fs::remove_dir_all(outside);
    let _ = std::fs::remove_dir_all(root);
}

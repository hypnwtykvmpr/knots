use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

fn unique_workspace(prefix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&path).expect("workspace should be creatable");
    path
}

fn run_git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_repo_with_remote(root: &Path) {
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "knots@example.com"]);
    run_git(root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# knots\n").expect("readme should be writable");
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(root, &["branch", "-M", "main"]);

    let remote = root.join("remote.git");
    run_git(
        root,
        &["init", "--bare", remote.to_str().expect("utf8 path")],
    );
    run_git(
        root,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("utf8 path"),
        ],
    );
    run_git(root, &["push", "-u", "origin", "main"]);
}

fn knots_binary() -> PathBuf {
    let configured = PathBuf::from(env!("CARGO_BIN_EXE_knots"));
    if configured.is_absolute() && configured.exists() {
        return configured;
    }
    if configured.exists() {
        return std::fs::canonicalize(&configured).unwrap_or(configured);
    }
    let manifest_relative = Path::new(env!("CARGO_MANIFEST_DIR")).join(&configured);
    if manifest_relative.exists() {
        return std::fs::canonicalize(&manifest_relative).unwrap_or(manifest_relative);
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(debug_dir) = current_exe.parent().and_then(|deps| deps.parent()) {
            for name in ["knots", "knots.exe"] {
                let candidate = debug_dir.join(name);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    configured
}

fn run_knots(repo_root: &Path, db_path: &Path, args: &[&str]) -> Output {
    Command::new(knots_binary())
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--db")
        .arg(db_path)
        .env("KNOTS_SKIP_DOCTOR_UPGRADE", "1")
        .args(args)
        .output()
        .expect("knots command should run")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success but failed.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn parse_created_id(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .expect("created output should include knot id")
        .to_string()
}

fn create_knot(root: &Path, db: &Path, title: &str) -> String {
    let alias = parse_created_id(&run_knots(
        root,
        db,
        &[
            "new",
            title,
            "--profile",
            "default",
            "--state",
            "implementation",
        ],
    ));
    resolve_full_id(root, db, &alias)
}

fn resolve_full_id(root: &Path, db: &Path, alias: &str) -> String {
    let show = run_knots(root, db, &["show", alias, "--json"]);
    assert_success(&show);
    let json: Value = serde_json::from_slice(&show.stdout).expect("show json should parse");
    json["id"]
        .as_str()
        .expect("show json should carry id")
        .to_string()
}

fn bootstrap_workflows(repo_root: &Path, db_path: &Path) {
    for (knot_type, workflow_id) in [
        ("work", "work_sdlc"),
        ("gate", "gate_sdlc"),
        ("lease", "lease_sdlc"),
        ("explore", "explore_sdlc"),
        ("execution_plan", "execution_plan_sdlc"),
    ] {
        let output = run_knots(
            repo_root,
            db_path,
            &["workflow", "use", workflow_id, "--type", knot_type],
        );
        assert_success(&output);
    }
}

fn demote_to_cold(db: &Path, id: &str, title: &str, updated_at: &str) {
    let conn = Connection::open(db).expect("db should open");
    // Read the existing hot row so we can reconstruct a warm + cold entry.
    conn.execute("DELETE FROM knot_hot WHERE id = ?1", rusqlite::params![id])
        .expect("delete hot should succeed");
    conn.execute(
        "INSERT OR REPLACE INTO knot_warm (id, title) VALUES (?1, ?2)",
        rusqlite::params![id, title],
    )
    .expect("insert warm should succeed");
    conn.execute(
        "INSERT OR REPLACE INTO cold_catalog (id, title, state, updated_at) \
         VALUES (?1, ?2, 'implementation', ?3)",
        rusqlite::params![id, title, updated_at],
    )
    .expect("insert cold should succeed");
}

fn count_cold_catalog(db: &Path) -> i64 {
    Connection::open(db)
        .expect("db should open")
        .query_row("SELECT COUNT(*) FROM cold_catalog", [], |row| row.get(0))
        .expect("count should succeed")
}

fn count_knot_hot(db: &Path) -> i64 {
    Connection::open(db)
        .expect("db should open")
        .query_row("SELECT COUNT(*) FROM knot_hot", [], |row| row.get(0))
        .expect("count should succeed")
}

fn find_check<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .expect("checks should be an array")
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("doctor report should contain check {name}"))
}

#[test]
fn doctor_warns_when_cold_catalog_present_below_hot_target() {
    let root = unique_workspace("knots-cli-cold-tier-warn");
    setup_repo_with_remote(&root);
    let db = root.join(".knots/cache/state.sqlite");
    bootstrap_workflows(&root, &db);

    // Seed three knots, then archive two into cold.
    let hot_id = create_knot(&root, &db, "Hot one");
    let cold_a = create_knot(&root, &db, "Cold A");
    let cold_b = create_knot(&root, &db, "Cold B");
    demote_to_cold(&db, &cold_a, "Cold A", "2026-04-09T00:00:00Z");
    demote_to_cold(&db, &cold_b, "Cold B", "2026-04-10T00:00:00Z");

    let doctor = run_knots(&root, &db, &["doctor", "--json"]);
    assert_success(&doctor);
    let report: Value = serde_json::from_slice(&doctor.stdout).expect("doctor json should parse");
    let check = find_check(&report, "cold_tier_imbalance");
    assert_eq!(check["status"], "warn");
    let data = check
        .get("data")
        .expect("cold_tier_imbalance should carry data");
    let hot_count = data["hot_count"].as_i64().expect("hot_count should be int");
    let cold_count = data["cold_count"]
        .as_i64()
        .expect("cold_count should be int");
    assert!(
        hot_count >= 1,
        "hot should include at least the seeded knot"
    );
    assert_eq!(cold_count, 2, "both archived knots should be in cold");
    let detail = check["detail"].as_str().expect("detail should be a string");
    assert!(
        detail.contains(&format!("{hot_count} hot / {cold_count} cold")),
        "detail should describe counts: {detail}"
    );

    // Sanity: the "hot" knot is still reachable via kno show.
    let show = run_knots(&root, &db, &["show", &hot_id, "--json"]);
    assert_success(&show);
}

#[test]
fn doctor_fix_rehydrates_cold_catalog_back_to_hot() {
    let root = unique_workspace("knots-cli-cold-tier-fix");
    setup_repo_with_remote(&root);
    let db = root.join(".knots/cache/state.sqlite");
    bootstrap_workflows(&root, &db);

    let hot_id = create_knot(&root, &db, "Hot one");
    let cold_a = create_knot(&root, &db, "Cold A");
    let cold_b = create_knot(&root, &db, "Cold B");
    demote_to_cold(&db, &cold_a, "Cold A", "2026-04-08T00:00:00Z");
    demote_to_cold(&db, &cold_b, "Cold B", "2026-04-09T00:00:00Z");

    assert_eq!(count_knot_hot(&db), 1);
    assert_eq!(count_cold_catalog(&db), 2);

    let fix = run_knots(&root, &db, &["doctor", "--fix"]);
    assert_success(&fix);

    assert_eq!(
        count_knot_hot(&db),
        3,
        "all cold knots should be rehydrated"
    );
    assert_eq!(count_cold_catalog(&db), 0, "cold catalog should be drained");

    let after = run_knots(&root, &db, &["doctor", "--json"]);
    assert_success(&after);
    let report: Value = serde_json::from_slice(&after.stdout).expect("doctor json should parse");
    let check = find_check(&report, "cold_tier_imbalance");
    assert_eq!(check["status"], "pass");

    // Rehydrated knots should be visible via kno show.
    for id in [&hot_id, &cold_a, &cold_b] {
        let show = run_knots(&root, &db, &["show", id, "--json"]);
        assert_success(&show);
    }
}

fn insert_shadow_cold(db: &Path, id: &str, title: &str, updated_at: &str) {
    // Leaves the hot row intact while inserting a cold_catalog row with the
    // same id — the data-consistency leftover that kept the imbalance
    // warning lit indefinitely before the fix.
    let conn = Connection::open(db).expect("db should open");
    conn.execute(
        "INSERT OR REPLACE INTO knot_warm (id, title) VALUES (?1, ?2)",
        rusqlite::params![id, title],
    )
    .expect("insert warm should succeed");
    conn.execute(
        "INSERT OR REPLACE INTO cold_catalog (id, title, state, updated_at) \
         VALUES (?1, ?2, 'implementation', ?3)",
        rusqlite::params![id, title, updated_at],
    )
    .expect("insert cold should succeed");
}

#[test]
fn doctor_fix_prunes_cold_row_shadowed_by_hot() {
    let root = unique_workspace("knots-cli-cold-tier-shadow");
    setup_repo_with_remote(&root);
    let db = root.join(".knots/cache/state.sqlite");
    bootstrap_workflows(&root, &db);

    let hot_id = create_knot(&root, &db, "Hot one");
    // The hot row still exists — simulate the data-consistency glitch where
    // a prior bug left a cold_catalog entry for the same id.
    insert_shadow_cold(&db, &hot_id, "Hot one", "2026-04-09T00:00:00Z");

    assert_eq!(count_knot_hot(&db), 1);
    assert_eq!(count_cold_catalog(&db), 1);

    let before = run_knots(&root, &db, &["doctor", "--json"]);
    assert_success(&before);
    let before_report: Value =
        serde_json::from_slice(&before.stdout).expect("doctor json should parse");
    let before_check = find_check(&before_report, "cold_tier_imbalance");
    assert_eq!(before_check["status"], "warn");
    let before_detail = before_check["detail"]
        .as_str()
        .expect("detail should be a string");
    assert!(
        before_detail.contains("shadowed"),
        "pre-fix detail should mention shadowed rows: {before_detail}"
    );

    let fix = run_knots(&root, &db, &["doctor", "--fix"]);
    assert_success(&fix);

    assert_eq!(
        count_knot_hot(&db),
        1,
        "pruning cold shadows must not touch hot rows"
    );
    assert_eq!(
        count_cold_catalog(&db),
        0,
        "shadowed cold row should be pruned"
    );

    let after = run_knots(&root, &db, &["doctor", "--json"]);
    assert_success(&after);
    let report: Value = serde_json::from_slice(&after.stdout).expect("doctor json should parse");
    let check = find_check(&report, "cold_tier_imbalance");
    assert_eq!(
        check["status"], "pass",
        "cold_tier_imbalance should clear after --fix"
    );
}

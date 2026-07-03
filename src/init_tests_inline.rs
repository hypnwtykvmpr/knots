//! Inline init tests relocated to keep init.rs under the size limit.

use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

use crate::db;

use super::{init_all, init_local_store, uninit_all, uninit_local_store, KNOTS_IGNORE_RULE};

fn unique_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("knots-init-test-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("temp directory should be creatable");
    dir
}

fn remove_dir_if_exists(root: &PathBuf) {
    if root.exists() {
        let _ = std::fs::remove_dir_all(root);
    }
}

fn run_git(cwd: &PathBuf, args: &[&str]) {
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

fn setup_repo_with_remote() -> (PathBuf, PathBuf) {
    let root = unique_dir();
    let remote = root.join("remote.git");
    let local = root.join("local");

    std::fs::create_dir_all(&local).expect("local dir should be creatable");
    run_git(
        &root,
        &["init", "--bare", remote.to_str().expect("utf8 path")],
    );
    run_git(&local, &["init"]);
    run_git(&local, &["config", "user.email", "knots@example.com"]);
    run_git(&local, &["config", "user.name", "Knots Test"]);
    std::fs::write(local.join("README.md"), "# knots\n").expect("readme should be writable");
    run_git(&local, &["add", "README.md"]);
    run_git(&local, &["commit", "-m", "init"]);
    run_git(&local, &["branch", "-M", "main"]);
    run_git(
        &local,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("utf8 path"),
        ],
    );
    run_git(&local, &["push", "-u", "origin", "main"]);
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .expect("git symbolic-ref should run");
    assert!(
        output.status.success(),
        "git symbolic-ref failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    (root, local)
}

#[test]
fn init_local_store_writes_expected_artifacts() {
    let root = unique_dir();
    let db_path = root.join(".knots/cache/state.sqlite");

    init_local_store(&root, db_path.to_str().expect("utf8 path")).expect("local init should work");

    assert!(db_path.exists());

    let gitignore =
        std::fs::read_to_string(root.join(".gitignore")).expect("gitignore should be readable");
    assert!(gitignore.lines().any(|line| line == KNOTS_IGNORE_RULE));
    remove_dir_if_exists(&root);
}

#[test]
fn init_local_store_is_idempotent_with_gitignore() {
    let root = unique_dir();
    let db_path = root.join(".knots/cache/state.sqlite");

    init_local_store(&root, db_path.to_str().expect("utf8 path")).expect("first init should work");
    init_local_store(&root, db_path.to_str().expect("utf8 path"))
        .expect("second init should remain idempotent");

    let gitignore =
        std::fs::read_to_string(root.join(".gitignore")).expect("gitignore should be readable");
    let ignore_count = gitignore
        .lines()
        .filter(|line| *line == KNOTS_IGNORE_RULE)
        .count();
    assert_eq!(ignore_count, 1);
    remove_dir_if_exists(&root);
}

#[test]
fn init_all_bootstraps_local_store_and_remote_branch() {
    let (root, local) = setup_repo_with_remote();
    let db_path = local.join(".knots/cache/state.sqlite");

    init_all(&local, db_path.to_str().expect("utf8 path")).expect("init should succeed");

    let output = Command::new("git")
        .arg("-C")
        .arg(&local)
        .args(["ls-remote", "--heads", "origin", "knots"])
        .output()
        .expect("git ls-remote should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("refs/heads/knots"));

    let gitignore =
        std::fs::read_to_string(local.join(".gitignore")).expect("gitignore should be readable");
    assert!(gitignore.lines().any(|line| line == KNOTS_IGNORE_RULE));
    remove_dir_if_exists(&root);
}

#[test]
fn init_all_pulls_knots_when_remote_branch_already_exists() {
    let (root, local) = setup_repo_with_remote();
    let local_db_path = local.join(".knots/cache/state.sqlite");
    init_all(&local, local_db_path.to_str().expect("utf8 path"))
        .expect("first init should succeed");

    let app = crate::app::App::open(local_db_path.to_str().expect("utf8 path"), local.clone())
        .expect("app should open");
    let created = app
        .create_knot(
            "Bootstrap knot",
            Some("pulled from remote"),
            Some("ready_for_planning"),
            Some("autopilot"),
        )
        .expect("knot should be creatable");
    app.push().expect("push should succeed");

    let clone = root.join("clone");
    run_git(
        &root,
        &[
            "clone",
            root.join("remote.git").to_str().expect("utf8 path"),
            clone.to_str().expect("utf8 path"),
        ],
    );
    run_git(&clone, &["config", "user.email", "knots@example.com"]);
    run_git(&clone, &["config", "user.name", "Knots Test"]);

    let clone_db_path = clone.join(".knots/cache/state.sqlite");
    init_all(&clone, clone_db_path.to_str().expect("utf8 path"))
        .expect("clone init should succeed");

    let clone_conn = db::open_connection(clone_db_path.to_str().expect("utf8 path"))
        .expect("clone db should open");
    let knot = db::get_knot_hot(&clone_conn, &created.id)
        .expect("knot query should succeed")
        .expect("knot should be pulled into clone");
    assert_eq!(knot.title, "Bootstrap knot");
    assert_eq!(knot.state, "ready_for_planning");

    remove_dir_if_exists(&root);
}

#[test]
fn uninit_local_store_cleans_local_artifacts_and_gitignore() {
    let root = unique_dir();
    let db_path = root.join(".knots/cache/state.sqlite");
    let gitignore_path = root.join(".gitignore");

    init_local_store(&root, db_path.to_str().expect("utf8 path"))
        .expect("local init should succeed");
    assert!(root.join(".knots").exists());
    assert!(db_path.exists());

    uninit_local_store(&root, db_path.to_str().expect("utf8 path"))
        .expect("local uninit should succeed");

    assert!(!root.join(".knots").exists());
    assert!(!db_path.exists());
    if gitignore_path.exists() {
        let gitignore =
            std::fs::read_to_string(&gitignore_path).expect("gitignore should be readable");
        assert!(!gitignore.lines().any(|line| line == KNOTS_IGNORE_RULE));
    }
    remove_dir_if_exists(&root);
}

#[test]
fn uninit_all_removes_remote_and_local_store() {
    let (root, local) = setup_repo_with_remote();
    let db_path = local.join(".knots/cache/state.sqlite");

    init_all(&local, db_path.to_str().expect("utf8 path")).expect("init should succeed");
    uninit_all(&local, db_path.to_str().expect("utf8 path")).expect("uninit should succeed");

    assert!(!local.join(".knots").exists());
    let output = Command::new("git")
        .arg("-C")
        .arg(&local)
        .args(["ls-remote", "--heads", "origin", "knots"])
        .output()
        .expect("git ls-remote should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("refs/heads/knots"));
    remove_dir_if_exists(&root);
}

use std::path::PathBuf;
use std::process::Command;

use super::App;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-app-list-lease-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn run_git(root: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
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

fn setup_repo(root: &std::path::Path) {
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "knots@example.com"]);
    run_git(root, &["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# knots\n").expect("readme should be writable");
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(root, &["branch", "-M", "main"]);
}

fn open_app(root: &std::path::Path) -> App {
    let db_path = root.join(".knots/cache/state.sqlite");
    App::open(db_path.to_str().expect("utf8 db path"), root.to_path_buf()).expect("app should open")
}

#[test]
fn list_knots_populates_lease_agent_for_active_knot_and_omits_for_queued() {
    let root = unique_workspace();
    setup_repo(&root);
    let app = open_app(&root);

    let active = app
        .create_knot(
            "Active lease-bound work",
            None,
            Some("work_item"),
            Some("default"),
        )
        .expect("active knot should be created");
    let queued = app
        .create_knot(
            "Queued unleased work",
            None,
            Some("work_item"),
            Some("default"),
        )
        .expect("queued knot should be created");

    let lease = crate::lease::create_lease(
        &app,
        "list-lease",
        crate::domain::lease::LeaseType::Agent,
        Some(crate::domain::lease::AgentInfo {
            agent_type: "cli".to_string(),
            provider: "Anthropic".to_string(),
            agent_name: "claude".to_string(),
            model: "opus".to_string(),
            model_version: "4.7".to_string(),
        }),
        600,
    )
    .expect("lease should be created");
    crate::lease::bind_lease(&app, &active.id, &lease.id).expect("bind should succeed");

    let listing = app.list_knots().expect("list should succeed");
    let active_view = listing
        .iter()
        .find(|k| k.id == active.id)
        .expect("active knot should appear in listing");
    let queued_view = listing
        .iter()
        .find(|k| k.id == queued.id)
        .expect("queued knot should appear in listing");

    let agent = active_view
        .lease_agent
        .as_ref()
        .expect("active knot should expose lease agent in listing");
    assert_eq!(active_view.lease_id.as_deref(), Some(lease.id.as_str()));
    assert_eq!(agent.agent_type, "cli");
    assert_eq!(agent.provider, "Anthropic");
    assert_eq!(agent.agent_name, "claude");
    assert_eq!(agent.model, "opus");
    assert_eq!(agent.model_version, "4.7");

    assert!(
        queued_view.lease_id.is_none(),
        "queued knot should have no bound lease id"
    );
    assert!(
        queued_view.lease_agent.is_none(),
        "queued knot must not pretend a lease is present"
    );

    let _ = std::fs::remove_dir_all(root);
}

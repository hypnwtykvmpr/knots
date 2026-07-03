use std::path::Path;
use std::process::Command;

use super::*;

fn unique_workspace() -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("knots-run-command-more-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn setup_git_repo(root: &Path) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init"]);
    run(&["config", "user.email", "knots@example.com"]);
    run(&["config", "user.name", "Knots Test"]);
    std::fs::write(root.join("README.md"), "# knots\n").expect("readme should be writable");
    run(&["add", "README.md"]);
    run(&["commit", "-m", "init"]);
}

fn setup_git_repo_with_remote(root: &Path) {
    setup_git_repo(root);
    let remote = root.join("remote.git");
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "init",
            "--bare",
            remote.to_str().expect("remote path should be utf8"),
        ])
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git init --bare failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for args in [
        vec!["remote", "add", "origin", remote.to_str().unwrap()],
        vec!["push", "-u", "origin", "HEAD"],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn open_app(root: &Path) -> app::App {
    setup_git_repo(root);
    let db_path = root.join(".knots/cache/state.sqlite");
    app::App::open(
        db_path.to_str().expect("db path should be utf8"),
        root.to_path_buf(),
    )
    .expect("app should open")
}

fn open_app_with_remote(root: &Path) -> app::App {
    setup_git_repo_with_remote(root);
    let db_path = root.join(".knots/cache/state.sqlite");
    app::App::open(
        db_path.to_str().expect("db path should be utf8"),
        root.to_path_buf(),
    )
    .expect("app should open")
}

fn list_args() -> crate::cli::ListArgs {
    crate::cli::ListArgs {
        all: true,
        json: false,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: None,
        stream: false,
        limit: None,
        offset: None,
    }
}

#[test]
fn read_commands_cover_show_list_edge_prompt_and_lease_list_paths() {
    let root = unique_workspace();
    let app = open_app(&root);
    let first = app
        .create_knot("first", None, Some("work_item"), None)
        .expect("first knot should be created");
    let second = app
        .create_knot("second", None, Some("work_item"), None)
        .expect("second knot should be created");
    app.add_edge(&first.id, "relates_to", &second.id)
        .expect("edge should be added");

    run_ls(
        &app,
        crate::cli::ListArgs {
            json: true,
            ..list_args()
        },
    )
    .expect("json list should succeed");
    run_ls(
        &app,
        crate::cli::ListArgs {
            limit: Some(1),
            offset: Some(0),
            ..list_args()
        },
    )
    .expect("paginated text list should succeed");
    run_show(
        &app,
        crate::cli::ShowArgs {
            id: first.id.clone(),
            json: false,
            verbose: true,
        },
    )
    .expect("text show should succeed");
    run_show(
        &app,
        crate::cli::ShowArgs {
            id: first.id.clone(),
            json: true,
            verbose: true,
        },
    )
    .expect("verbose json show should succeed");
    run_edge_list(
        &app,
        crate::cli::EdgeListArgs {
            id: first.id.clone(),
            direction: "both".to_string(),
            json: false,
        },
    )
    .expect("edge list should print edge rows");
    run_edge_list(
        &app,
        crate::cli::EdgeListArgs {
            id: "missing".to_string(),
            direction: "both".to_string(),
            json: true,
        },
    )
    .expect("json edge list should succeed");
    run_prompt(
        &app,
        crate::cli::PromptArgs {
            id: "implementation".to_string(),
        },
    )
    .expect("prompt by state should succeed");

    crate::lease::create_lease(
        &app,
        "listable",
        crate::domain::lease::LeaseType::Agent,
        None,
        600,
    )
    .expect("lease should be created");
    run_lease_read(
        &app,
        crate::cli::LeaseArgs {
            command: crate::cli::LeaseSubcommands::List(crate::cli::LeaseListArgs {
                all: false,
                json: false,
            }),
        },
    )
    .expect("active lease list should succeed");
    run_lease_read(
        &app,
        crate::cli::LeaseArgs {
            command: crate::cli::LeaseSubcommands::List(crate::cli::LeaseListArgs {
                all: true,
                json: true,
            }),
        },
    )
    .expect("all lease json list should succeed");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn read_command_show_missing_reports_not_found() {
    let root = unique_workspace();
    let app = open_app(&root);
    let missing_show = run_show(
        &app,
        crate::cli::ShowArgs {
            id: "missing".to_string(),
            json: false,
            verbose: false,
        },
    )
    .expect_err("missing show should fail");
    assert!(matches!(missing_show, app::AppError::NotFound(_)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn maintenance_and_cold_commands_cover_success_and_error_output_paths() {
    let root = unique_workspace();
    let app = open_app(&root);

    let compact_err = run_compact(
        &app,
        crate::cli::CompactArgs {
            write_snapshots: false,
            json: false,
        },
    )
    .expect_err("compact without --write-snapshots should fail");
    assert!(compact_err
        .to_string()
        .contains("requires --write-snapshots"));
    run_compact(
        &app,
        crate::cli::CompactArgs {
            write_snapshots: true,
            json: true,
        },
    )
    .expect("compact json should succeed");
    run_fsck(&app, crate::cli::FsckArgs { json: false }).expect("fsck should succeed");
    let bad_index = root.join(".knots/index/bad.json");
    std::fs::create_dir_all(bad_index.parent().expect("bad index should have a parent"))
        .expect("index dir should exist");
    std::fs::write(&bad_index, "{ invalid json").expect("bad index should write");
    let fsck_err =
        run_fsck(&app, crate::cli::FsckArgs { json: false }).expect_err("fsck should fail");
    assert!(fsck_err.to_string().contains("fsck found"));
    run_perf(
        &app,
        crate::cli::PerfArgs {
            json: false,
            iterations: 1,
            strict: false,
        },
    )
    .expect("perf text should succeed");

    crate::db::upsert_cold_catalog(
        app.conn_for_test(),
        "K-cold",
        "Archived item",
        "shipped",
        "2026-04-01T00:00:00Z",
    )
    .expect("cold catalog row should insert");
    run_cold(
        &app,
        crate::cli::ColdArgs {
            command: crate::cli::ColdSubcommands::Search(crate::cli::ColdSearchArgs {
                term: "none".to_string(),
                json: false,
            }),
        },
    )
    .expect("empty cold search should succeed");
    run_cold(
        &app,
        crate::cli::ColdArgs {
            command: crate::cli::ColdSubcommands::Search(crate::cli::ColdSearchArgs {
                term: "Archived".to_string(),
                json: true,
            }),
        },
    )
    .expect("json cold search should succeed");
    run_cold(
        &app,
        crate::cli::ColdArgs {
            command: crate::cli::ColdSubcommands::Search(crate::cli::ColdSearchArgs {
                term: "Archived".to_string(),
                json: false,
            }),
        },
    )
    .expect("text cold search should print rows");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn run_sync_prints_deferred_and_json_deferred_without_touching_git_remote() {
    let root = unique_workspace();
    let app = open_app(&root);
    crate::lease::create_lease(
        &app,
        "sync deferred",
        crate::domain::lease::LeaseType::Agent,
        None,
        600,
    )
    .expect("lease should be created");

    run_sync(&app, crate::cli::SyncArgs { json: false })
        .expect("text sync should defer while lease exists");
    run_sync(&app, crate::cli::SyncArgs { json: true })
        .expect("json sync should defer while lease exists");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn read_commands_cover_remote_text_empty_and_error_paths() {
    let root = unique_workspace();
    let app = open_app_with_remote(&root);
    app.init_remote(None)
        .expect("remote knots branch should init");
    let knot = app
        .create_knot(
            "remote text paths",
            None,
            Some("ready_for_implementation"),
            None,
        )
        .expect("knot should be created");

    run_push(&app, crate::cli::SyncArgs { json: false }).expect("push text should succeed");
    run_pull(&app, crate::cli::SyncArgs { json: false }).expect("pull text should succeed");
    run_sync(&app, crate::cli::SyncArgs { json: false }).expect("sync text should succeed");
    run_cold(
        &app,
        crate::cli::ColdArgs {
            command: crate::cli::ColdSubcommands::Sync(crate::cli::SyncArgs { json: false }),
        },
    )
    .expect("cold sync text should succeed");

    let missing_rehydrate = run_rehydrate(
        &app,
        crate::cli::RehydrateArgs {
            id: "missing-knot".to_string(),
            json: false,
        },
    )
    .expect_err("missing rehydrate should report not found");
    assert!(matches!(missing_rehydrate, app::AppError::NotFound(_)));

    run_edge_list(
        &app,
        crate::cli::EdgeListArgs {
            id: knot.id.clone(),
            direction: "both".to_string(),
            json: false,
        },
    )
    .expect("empty edge list text should succeed");
    run_prompt(
        &app,
        crate::cli::PromptArgs {
            id: knot.id.clone(),
        },
    )
    .expect("prompt by knot id should succeed");
    let prompt_err = run_prompt(
        &app,
        crate::cli::PromptArgs {
            id: "not-a-real-state".to_string(),
        },
    )
    .expect_err("unknown prompt should fail");
    assert!(prompt_err.to_string().contains("not a knot id"));

    let lease = crate::lease::create_lease(
        &app,
        "showable",
        crate::domain::lease::LeaseType::Agent,
        None,
        600,
    )
    .expect("lease should be created");
    run_lease_read(
        &app,
        crate::cli::LeaseArgs {
            command: crate::cli::LeaseSubcommands::Show(crate::cli::LeaseShowArgs {
                id: lease.id.clone(),
                json: false,
            }),
        },
    )
    .expect("lease show text should succeed");
    run_lease_read(
        &app,
        crate::cli::LeaseArgs {
            command: crate::cli::LeaseSubcommands::Show(crate::cli::LeaseShowArgs {
                id: lease.id.clone(),
                json: true,
            }),
        },
    )
    .expect("lease show json should succeed");
    crate::lease::terminate_lease(&app, &lease.id).expect("lease should terminate");
    run_lease_read(
        &app,
        crate::cli::LeaseArgs {
            command: crate::cli::LeaseSubcommands::List(crate::cli::LeaseListArgs {
                all: false,
                json: false,
            }),
        },
    )
    .expect("empty lease list text should succeed");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn private_list_helper_applies_legacy_limit_after_full_filtering() {
    let root = unique_workspace();
    let app = open_app(&root);
    app.create_knot("first", None, Some("work_item"), None)
        .expect("first knot should be created");
    app.create_knot("second", None, Some("work_item"), None)
        .expect("second knot should be created");

    run_ls_full(
        &app,
        crate::cli::ListArgs {
            limit: Some(1),
            ..list_args()
        },
    )
    .expect("direct full list helper should retain legacy limit behavior");

    let _ = std::fs::remove_dir_all(root);
}

use clap::Parser;

fn unique_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("temp dir should be creatable");
    dir
}

fn run_git(root: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
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

fn init_git_repo(prefix: &str) -> std::path::PathBuf {
    let root = unique_dir(prefix);
    run_git(&root, &["init"]);
    root
}

#[test]
fn first_positional_subcommand_skips_top_level_options_and_detects_skill_alias() {
    let args = [
        "kno",
        "--trace",
        "--db=custom.sqlite",
        "-Crepo",
        "--project",
        "local",
        "skill",
        "implementation",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    assert_eq!(super::first_positional_subcommand(&args), Some("skill"));
    assert!(super::invoked_deprecated_skill_alias(&args));

    let only_flags = ["kno", "--trace", "-d", "db.sqlite"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    assert_eq!(super::first_positional_subcommand(&only_flags), None);
}

#[test]
fn resolve_db_path_handles_default_absolute_and_distribution_relative_paths() {
    let root = unique_dir("knots-main-resolve-db");
    let git_context = crate::project::ProjectContext {
        project_id: None,
        repo_root: root.join("repo"),
        store_paths: crate::project::StorePaths {
            root: root.join("repo/.knots"),
        },
        distribution: crate::project::DistributionMode::Git,
    };
    let local_context = crate::project::ProjectContext {
        project_id: Some("local".to_string()),
        repo_root: root.join("local-repo"),
        store_paths: crate::project::StorePaths {
            root: root.join("store"),
        },
        distribution: crate::project::DistributionMode::LocalOnly,
    };

    assert_eq!(
        super::resolve_db_path(&git_context, None),
        git_context.store_paths.db_path().display().to_string()
    );
    let absolute = root.join("absolute.sqlite");
    assert_eq!(
        super::resolve_db_path(&git_context, Some(absolute.to_str().unwrap())),
        absolute.display().to_string()
    );
    assert_eq!(
        super::resolve_db_path(&git_context, Some("relative.sqlite")),
        git_context
            .repo_root
            .join("relative.sqlite")
            .display()
            .to_string()
    );
    assert_eq!(
        super::resolve_db_path(&local_context, Some("relative.sqlite")),
        local_context
            .store_paths
            .root
            .join("relative.sqlite")
            .display()
            .to_string()
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn command_name_tracks_top_level_subcommands() {
    let cases = [
        (vec!["kno", "new", "title"], "new"),
        (vec!["kno", "state", "K-1", "planning"], "state"),
        (vec!["kno", "update", "K-1", "--title", "new"], "update"),
        (vec!["kno", "upgrade"], "upgrade"),
        (vec!["kno", "uninstall"], "uninstall"),
        (vec!["kno", "ls"], "ls"),
        (vec!["kno", "show", "K-1"], "show"),
        (vec!["kno", "profile", "list"], "profile"),
        (vec!["kno", "workflow", "list"], "workflow"),
        (vec!["kno", "project", "list"], "project"),
        (vec!["kno", "loom", "compat-test"], "loom"),
        (vec!["kno", "pull"], "pull"),
        (vec!["kno", "push"], "push"),
        (vec!["kno", "sync"], "sync"),
        (
            vec!["kno", "sync-ref", "migrate", "--source", "local"],
            "sync-ref",
        ),
        (vec!["kno", "init"], "init"),
        (vec!["kno", "uninit"], "uninit"),
        (vec!["kno", "init-remote"], "init-remote"),
        (vec!["kno", "fsck"], "fsck"),
        (vec!["kno", "doctor"], "doctor"),
        (vec!["kno", "perf"], "perf"),
        (vec!["kno", "compact"], "compact"),
        (vec!["kno", "cold", "search", "term"], "cold"),
        (vec!["kno", "rehydrate", "K-1"], "rehydrate"),
        (vec!["kno", "edge", "list", "K-1"], "edge"),
        (
            vec!["kno", "gate", "evaluate", "K-1", "--decision", "yes"],
            "gate",
        ),
        (
            vec![
                "kno",
                "plan",
                "wave",
                "add",
                "K-1",
                "--name",
                "Wave",
                "--objective",
                "Goal",
            ],
            "plan",
        ),
        (vec!["kno", "next", "K-1"], "next"),
        (vec!["kno", "rollback", "K-1"], "rollback"),
        (vec!["kno", "prompt", "implementation"], "prompt"),
        (vec!["kno", "skills", "install", "codex"], "skills"),
        (vec!["kno", "q", "quick"], "q"),
        (vec!["kno", "completions", "bash"], "completions"),
        (vec!["kno", "poll"], "poll"),
        (vec!["kno", "claim", "K-1"], "claim"),
        (vec!["kno", "ready"], "ready"),
        (vec!["kno", "step", "annotate", "K-1"], "step"),
        (vec!["kno", "lease", "list"], "lease"),
        (vec!["kno", "hooks", "status"], "hooks"),
    ];

    for (argv, expected) in cases {
        let cli = crate::cli::Cli::parse_from(argv);
        assert_eq!(super::command_name(&cli.command), expected);
    }
}

#[test]
fn progress_reporter_respects_flag() {
    assert!(super::progress_reporter(false).is_none());
    assert!(super::progress_reporter(true).is_some());
}

#[test]
fn target_from_arg_maps_all_skill_tools() {
    let cases = [
        (
            crate::cli_skills::SkillTargetArg::Codex,
            crate::managed_skills::SkillTool::Codex,
        ),
        (
            crate::cli_skills::SkillTargetArg::Claude,
            crate::managed_skills::SkillTool::Claude,
        ),
        (
            crate::cli_skills::SkillTargetArg::OpenCode,
            crate::managed_skills::SkillTool::OpenCode,
        ),
    ];

    for (arg, expected) in cases {
        assert_eq!(super::target_from_arg(arg), expected);
    }
}

#[test]
fn run_hooks_command_handles_install_status_and_uninstall() {
    let root = init_git_repo("knots-main-hooks-command");

    super::run_hooks_command(&root, &crate::cli::HooksSubcommands::Status)
        .expect("status should work before install");
    super::run_hooks_command(&root, &crate::cli::HooksSubcommands::Install)
        .expect("install should work");
    super::run_hooks_command(&root, &crate::cli::HooksSubcommands::Status)
        .expect("status should work after install");
    super::run_hooks_command(&root, &crate::cli::HooksSubcommands::Uninstall)
        .expect("uninstall should work");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn format_error_adds_hint_for_missing_knot() {
    let formatted = super::format_error(&crate::app::AppError::NotFound("missing".to_string()));
    assert!(formatted.contains("error:"));
    assert!(formatted.contains("try: kno -C <repo_root>"));
}

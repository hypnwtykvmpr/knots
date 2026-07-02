use std::io::Cursor;
use std::path::PathBuf;

use super::*;
#[cfg(target_os = "windows")]
use crate::test_env::EnvVarGuard;

fn temp_home() -> PathBuf {
    let path = std::env::temp_dir().join(format!("knots-project-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&path).expect("temp home should be creatable");
    path
}

#[test]
fn create_list_and_resolve_named_projects() {
    let home = temp_home();
    let project = create_named_project(Some(&home), "demo", None).expect("create project");
    assert_eq!(project.id, "demo");
    let listed = list_named_projects(Some(&home)).expect("list projects");
    assert_eq!(listed.len(), 1);
    set_active_project(Some(&home), "demo").expect("set active project");
    let context = resolve_context(None, None, &home, Some(&home)).expect("resolve context");
    assert_eq!(context.project_id.as_deref(), Some("demo"));
    assert_eq!(context.distribution, DistributionMode::LocalOnly);
    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_config_and_data_dirs_prefer_native_appdata_env_vars() {
    let env = EnvVarGuard::capture(&[
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
    ]);
    let home = temp_home();
    let appdata = home.join("Roaming");
    let local_appdata = home.join("Local");
    let shell_home = home.join("GitBashHome");
    let xdg_config = home.join("xdg-config");
    let xdg_data = home.join("xdg-data");

    env.set("HOME", &shell_home);
    env.set("APPDATA", &appdata);
    env.set("LOCALAPPDATA", &local_appdata);
    env.set("XDG_CONFIG_HOME", &xdg_config);
    env.set("XDG_DATA_HOME", &xdg_data);

    assert_eq!(config_dir(None).expect("config dir"), appdata.join("knots"));
    assert_eq!(
        data_dir(None).expect("data dir"),
        local_appdata.join("knots")
    );

    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_config_and_data_dirs_fall_back_after_appdata() {
    let env = EnvVarGuard::capture(&[
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
    ]);
    let home = temp_home();
    let appdata_only = home.join("RoamingOnly");
    let shell_home = home.join("ShellHome");
    let xdg_config = home.join("xdg-config");
    let xdg_data = home.join("xdg-data");
    let userprofile = home.join("Profile");

    env.remove("HOME");
    env.remove("APPDATA");
    env.remove("LOCALAPPDATA");
    env.remove("HOMEDRIVE");
    env.remove("HOMEPATH");
    env.set("XDG_CONFIG_HOME", &xdg_config);
    env.set("XDG_DATA_HOME", &xdg_data);
    env.set("USERPROFILE", &userprofile);

    assert_eq!(
        config_dir(None).expect("config dir"),
        userprofile.join("AppData").join("Roaming").join("knots")
    );
    assert_eq!(
        data_dir(None).expect("data dir"),
        userprofile.join("AppData").join("Local").join("knots")
    );

    env.remove("USERPROFILE");
    env.set("APPDATA", &appdata_only);
    assert_eq!(
        data_dir(None).expect("data appdata fallback"),
        appdata_only.join("knots")
    );
    env.remove("APPDATA");
    assert_eq!(
        config_dir(None).expect("config xdg fallback"),
        xdg_config.join("knots")
    );
    assert_eq!(
        data_dir(None).expect("data xdg fallback"),
        xdg_data.join("knots")
    );
    env.remove("XDG_CONFIG_HOME");
    env.remove("XDG_DATA_HOME");
    env.set("HOME", &shell_home);

    assert_eq!(
        config_dir(None).expect("config home fallback"),
        shell_home.join(".config").join("knots")
    );
    assert_eq!(
        data_dir(None).expect("data home fallback"),
        shell_home.join(".local").join("share").join("knots")
    );
    env.remove("HOME");
    env.set("USERPROFILE", &userprofile);

    assert_eq!(
        config_dir(None).expect("config home fallback"),
        userprofile.join("AppData").join("Roaming").join("knots")
    );
    assert_eq!(
        data_dir(None).expect("data home fallback"),
        userprofile.join("AppData").join("Local").join("knots")
    );

    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_falls_back_to_drive_and_homepath() {
    let env = EnvVarGuard::capture(&["HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"]);

    env.remove("HOME");
    env.remove("USERPROFILE");
    env.set("HOMEDRIVE", "Z:");
    env.set("HOMEPATH", "\\Users\\Knots");

    assert_eq!(
        home_dir(None).expect("home dir should resolve"),
        PathBuf::from("Z:\\Users\\Knots")
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_prefers_userprofile_over_shell_home() {
    let env = EnvVarGuard::capture(&["HOME", "USERPROFILE"]);
    let home = temp_home();
    let shell_home = home.join("GitBashHome");
    let userprofile = home.join("Profile");

    env.set("HOME", &shell_home);
    env.set("USERPROFILE", &userprofile);

    assert_eq!(home_dir(None).expect("home dir"), userprofile);

    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_rejects_drive_relative_homepath() {
    let env = EnvVarGuard::capture(&["HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"]);

    env.remove("HOME");
    env.remove("USERPROFILE");
    env.set("HOMEDRIVE", "Z:");
    env.set("HOMEPATH", "Users\\Knots");

    assert_eq!(
        home_dir(None).expect_err("drive-relative home should fail"),
        "unable to resolve home directory"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_uses_explicit_override_and_reports_missing_env() {
    let env = EnvVarGuard::capture(&["HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"]);
    let home = temp_home();

    assert_eq!(home_dir(Some(&home)).expect("explicit home"), home);

    env.remove("HOME");
    env.remove("USERPROFILE");
    env.remove("HOMEDRIVE");
    env.remove("HOMEPATH");

    assert_eq!(
        home_dir(None).expect_err("missing env should fail"),
        "unable to resolve home directory"
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn explicit_repo_root_beats_active_project() {
    let home = temp_home();
    create_named_project(Some(&home), "demo", None).expect("create project");
    set_active_project(Some(&home), "demo").expect("set active project");
    let repo_root = home.join("repo");
    fs::create_dir_all(repo_root.join(".git")).expect("git dir should exist");
    fs::write(repo_root.join(".git/HEAD"), "ref: refs/heads/main\n")
        .expect("git HEAD should exist");
    let context = resolve_context(None, Some(&repo_root), &home, Some(&home)).expect("resolve git");
    assert_eq!(context.project_id, None);
    assert_eq!(context.distribution, DistributionMode::Git);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn delete_project_removes_store_and_clears_active_project() {
    let home = temp_home();
    let project = create_named_project(Some(&home), "demo", None).expect("create project");
    set_active_project(Some(&home), "demo").expect("set active project");

    let store = project
        .store_paths(Some(&home))
        .expect("store paths should resolve");
    fs::write(store.root.join("marker.txt"), "x").expect("marker should be writable");
    delete_named_project(Some(&home), "demo").expect("delete project");

    assert!(!store.root.exists());
    assert!(!projects_dir(Some(&home))
        .expect("projects dir should resolve")
        .join("demo.toml")
        .exists());
    let config = read_global_config(Some(&home)).expect("config should load");
    assert_eq!(config.active_project, None);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn prompt_selection_requires_tty_streams() {
    let home = temp_home();
    let mut input = Cursor::new(b"1\n".to_vec());
    let mut output = Vec::new();

    let err = prompt_for_project_selection_from_io(
        &mut input,
        &mut output,
        Some(&home),
        None,
        false,
        true,
        &[],
    )
    .expect_err("non-tty stdin should reject interactive selection");

    assert_eq!(err, "interactive project selection requires a TTY");
    let _ = fs::remove_dir_all(home);
}

#[test]
fn prompt_selection_can_select_existing_project_or_create_new_one() {
    let home = temp_home();
    let first = NamedProjectRecord {
        id: "alpha".to_string(),
        repo_root: None,
    };
    let second = NamedProjectRecord {
        id: "bravo".to_string(),
        repo_root: None,
    };

    let mut existing_input = Cursor::new(b"2\n".to_vec());
    let mut existing_output = Vec::new();
    let selected = prompt_for_project_selection_with_io(
        &mut existing_input,
        &mut existing_output,
        Some(&home),
        None,
        &[first.clone(), second.clone()],
    )
    .expect("existing project should be selected");
    assert_eq!(selected.id, "bravo");

    let repo_root = home.join("repo");
    fs::create_dir_all(&repo_root).expect("repo root should be creatable");
    let mut create_input = Cursor::new(b"n\nnew_project\n".to_vec());
    let mut create_output = Vec::new();
    let created = prompt_for_project_selection_with_io(
        &mut create_input,
        &mut create_output,
        Some(&home),
        Some(&repo_root),
        &[first, second],
    )
    .expect("new project should be created");
    assert_eq!(created.id, "new_project");
    assert_eq!(
        load_named_project(Some(&home), "new_project")
            .expect("new project should load")
            .repo_root,
        Some(canonical_or_original(&repo_root))
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn prompt_selection_reports_invalid_and_out_of_range_choices() {
    let home = temp_home();
    let projects = vec![NamedProjectRecord {
        id: "alpha".to_string(),
        repo_root: None,
    }];

    let mut bad_input = Cursor::new(b"wat\n".to_vec());
    let mut bad_output = Vec::new();
    let bad = prompt_for_project_selection_with_io(
        &mut bad_input,
        &mut bad_output,
        Some(&home),
        None,
        &projects,
    )
    .expect_err("non-numeric choice should reject");
    assert_eq!(bad, "invalid selection");

    let mut range_input = Cursor::new(b"2\n".to_vec());
    let mut range_output = Vec::new();
    let range = prompt_for_project_selection_with_io(
        &mut range_input,
        &mut range_output,
        Some(&home),
        None,
        &projects,
    )
    .expect_err("out-of-range choice should reject");
    assert_eq!(range, "selection out of range");

    let _ = fs::remove_dir_all(home);
}

#[test]
fn project_files_fill_empty_ids_and_report_invalid_toml() {
    let home = temp_home();
    let dir = projects_dir(Some(&home)).expect("projects dir should resolve");
    fs::create_dir_all(&dir).expect("projects dir should be creatable");
    fs::write(dir.join("fallback.toml"), "id = \"\"\n").expect("project file should write");

    let loaded = load_named_project(Some(&home), "fallback").expect("project should load");
    assert_eq!(loaded.id, "fallback");

    fs::write(dir.join("broken.toml"), "id = [").expect("broken project should write");
    let err = list_named_projects(Some(&home)).expect_err("bad project TOML should fail");
    assert!(err.contains("invalid project file"));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn config_and_project_validation_errors_are_reported() {
    let home = temp_home();
    let config = config_path(Some(&home)).expect("config path should resolve");
    fs::create_dir_all(config.parent().expect("config parent should exist"))
        .expect("config parent should be creatable");
    fs::write(&config, "active_project = [").expect("invalid config should write");

    let err = read_global_config(Some(&home)).expect_err("invalid config should fail");
    assert!(err.contains("invalid config"));

    assert!(validate_project_id("").is_err());
    assert!(validate_project_id("UPPER").is_err());
    let reserved = ["con", "prn", "aux", "nul", "com1", "com9", "lpt1", "lpt9"];
    #[cfg(target_os = "windows")]
    for reserved in reserved {
        assert!(
            validate_project_id(reserved).is_err(),
            "{reserved} should be rejected"
        );
    }
    #[cfg(not(target_os = "windows"))]
    for reserved in reserved {
        assert!(
            validate_project_id(reserved).is_ok(),
            "{reserved} should remain valid off Windows"
        );
    }
    assert!(load_named_project(Some(&home), "missing")
        .expect_err("missing project should fail")
        .contains("unknown project"));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn create_named_project_rejects_nonexistent_repo_root() {
    let home = temp_home();
    let missing = home.join("missing-repo");
    let err = create_named_project(Some(&home), "demo", Some(&missing))
        .expect_err("missing repo root should be rejected");
    assert!(err.contains("must exist and be readable"), "{err}");
    let _ = fs::remove_dir_all(home);
}

#[test]
fn find_git_root_uses_file_markers_and_skips_knots_store() {
    let home = temp_home();
    let repo = home.join("repo");
    let nested = repo.join(".knots/cache/nested");
    fs::create_dir_all(&nested).expect("nested dir should exist");
    fs::write(repo.join(".git"), "gitdir: ../real.git\n").expect("git file marker should write");
    fs::write(nested.join(".git"), "gitdir: ../ignored.git\n")
        .expect("nested git marker should write");

    let found = find_git_root(&nested).expect("repo root should resolve");
    assert_eq!(found, canonical_or_original(&repo));

    let _ = fs::remove_dir_all(home);
}

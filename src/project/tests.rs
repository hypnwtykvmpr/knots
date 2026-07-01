use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use super::*;

fn temp_home() -> PathBuf {
    let path = std::env::temp_dir().join(format!("knots-project-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&path).expect("temp home should be creatable");
    path
}

#[cfg(target_os = "windows")]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(target_os = "windows")]
fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
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
fn windows_config_and_data_dirs_prefer_roaming_appdata_env_vars() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let home = temp_home();
    let appdata = home.join("Roaming");
    let xdg_config = home.join("xdg-config");
    let xdg_data = home.join("xdg-data");
    let old_appdata = std::env::var_os("APPDATA");
    let old_xdg_config = std::env::var_os("XDG_CONFIG_HOME");
    let old_xdg_data = std::env::var_os("XDG_DATA_HOME");

    std::env::set_var("APPDATA", &appdata);
    std::env::set_var("XDG_CONFIG_HOME", &xdg_config);
    std::env::set_var("XDG_DATA_HOME", &xdg_data);

    assert_eq!(config_dir(None).expect("config dir"), appdata.join("knots"));
    assert_eq!(data_dir(None).expect("data dir"), appdata.join("knots"));

    restore_env("APPDATA", old_appdata);
    restore_env("XDG_CONFIG_HOME", old_xdg_config);
    restore_env("XDG_DATA_HOME", old_xdg_data);
    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_config_and_data_dirs_fall_back_after_appdata() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let home = temp_home();
    let xdg_config = home.join("xdg-config");
    let xdg_data = home.join("xdg-data");
    let userprofile = home.join("Profile");
    let old_home = std::env::var_os("HOME");
    let old_appdata = std::env::var_os("APPDATA");
    let old_xdg_config = std::env::var_os("XDG_CONFIG_HOME");
    let old_xdg_data = std::env::var_os("XDG_DATA_HOME");
    let old_userprofile = std::env::var_os("USERPROFILE");

    std::env::remove_var("HOME");
    std::env::remove_var("APPDATA");
    std::env::set_var("XDG_CONFIG_HOME", &xdg_config);
    std::env::set_var("XDG_DATA_HOME", &xdg_data);
    std::env::set_var("USERPROFILE", &userprofile);

    assert_eq!(
        config_dir(None).expect("config dir"),
        xdg_config.join("knots")
    );
    assert_eq!(data_dir(None).expect("data dir"), xdg_data.join("knots"));

    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");

    assert_eq!(
        config_dir(None).expect("config home fallback"),
        userprofile.join(".config").join("knots")
    );
    assert_eq!(
        data_dir(None).expect("data home fallback"),
        userprofile.join(".local").join("share").join("knots")
    );

    restore_env("HOME", old_home);
    restore_env("APPDATA", old_appdata);
    restore_env("XDG_CONFIG_HOME", old_xdg_config);
    restore_env("XDG_DATA_HOME", old_xdg_data);
    restore_env("USERPROFILE", old_userprofile);
    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_falls_back_to_drive_and_homepath() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let old_home = std::env::var_os("HOME");
    let old_userprofile = std::env::var_os("USERPROFILE");
    let old_home_drive = std::env::var_os("HOMEDRIVE");
    let old_home_path = std::env::var_os("HOMEPATH");

    std::env::remove_var("HOME");
    std::env::remove_var("USERPROFILE");
    std::env::set_var("HOMEDRIVE", "Z:");
    std::env::set_var("HOMEPATH", "\\Users\\Knots");

    assert_eq!(
        home_dir(None).expect("home dir should resolve"),
        PathBuf::from("Z:\\Users\\Knots")
    );

    restore_env("HOME", old_home);
    restore_env("USERPROFILE", old_userprofile);
    restore_env("HOMEDRIVE", old_home_drive);
    restore_env("HOMEPATH", old_home_path);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_home_dir_uses_explicit_override_and_reports_missing_env() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let home = temp_home();
    let old_home = std::env::var_os("HOME");
    let old_userprofile = std::env::var_os("USERPROFILE");
    let old_home_drive = std::env::var_os("HOMEDRIVE");
    let old_home_path = std::env::var_os("HOMEPATH");

    assert_eq!(home_dir(Some(&home)).expect("explicit home"), home);

    std::env::remove_var("HOME");
    std::env::remove_var("USERPROFILE");
    std::env::remove_var("HOMEDRIVE");
    std::env::remove_var("HOMEPATH");

    assert_eq!(
        home_dir(None).expect_err("missing env should fail"),
        "unable to resolve home directory"
    );

    restore_env("HOME", old_home);
    restore_env("USERPROFILE", old_userprofile);
    restore_env("HOMEDRIVE", old_home_drive);
    restore_env("HOMEPATH", old_home_path);
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
    assert!(load_named_project(Some(&home), "missing")
        .expect_err("missing project should fail")
        .contains("unknown project"));

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

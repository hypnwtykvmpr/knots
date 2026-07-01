use super::*;

#[test]
fn shell_from_path_parses_known_shells() {
    assert_eq!(shell_from_path("/bin/zsh"), Some(Shell::Zsh));
    assert_eq!(shell_from_path("/usr/bin/bash"), Some(Shell::Bash));
    assert_eq!(shell_from_path("/usr/bin/fish"), Some(Shell::Fish));
    assert_eq!(shell_from_path("/usr/bin/elvish"), Some(Shell::Elvish));
    assert_eq!(shell_from_path("/usr/bin/pwsh"), Some(Shell::PowerShell));
    assert_eq!(
        shell_from_path("C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
        Some(Shell::PowerShell)
    );
    assert_eq!(shell_from_path("/usr/bin/csh"), None);
}

#[test]
fn completions_install_path_for_known_shells() {
    let home = PathBuf::from("/tmp/test-home");
    let bash = completions_install_path_for_home(Shell::Bash, &home);
    assert!(bash.unwrap().to_str().unwrap().contains("bash-completion"));
    let zsh = completions_install_path_for_home(Shell::Zsh, &home);
    assert!(zsh.unwrap().to_str().unwrap().contains("kno.zsh"));
    let fish = completions_install_path_for_home(Shell::Fish, &home);
    assert!(fish.unwrap().to_str().unwrap().contains("kno.fish"));
    let powershell = completions_install_path_for_home(Shell::PowerShell, &home);
    assert!(powershell.unwrap().to_str().unwrap().contains("PowerShell"));
}

#[test]
fn generate_completions_produces_non_empty_output() {
    let mut buf = Vec::new();
    generate_completions(Shell::Bash, &mut buf);
    assert!(!buf.is_empty(), "bash completions should be non-empty");
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("kno"),
        "bash completions should reference kno"
    );
}

#[test]
fn parse_shell_is_case_insensitive() {
    assert_eq!(parse_shell("BASH"), Some(Shell::Bash));
    assert_eq!(parse_shell("Zsh"), Some(Shell::Zsh));
    assert_eq!(parse_shell("Fish"), Some(Shell::Fish));
    assert_eq!(parse_shell("elvish"), Some(Shell::Elvish));
    assert_eq!(parse_shell("powershell"), Some(Shell::PowerShell));
    assert_eq!(parse_shell("pwsh"), Some(Shell::PowerShell));
    assert_eq!(parse_shell("nonsense"), None);
}

#[test]
fn install_completions_with_zshrc_patching() {
    let dir = std::env::temp_dir().join(format!("knots-comp-all-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("dir should be creatable");

    let path = install_completions_to(Shell::Bash, &dir).expect("bash install should succeed");
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).expect("should read file");
    assert!(content.contains("kno"));

    let zsh_path = install_completions_to(Shell::Zsh, &dir).expect("zsh install should succeed");
    assert!(zsh_path.exists());
    let zshrc = dir.join(".zshrc");
    assert!(zshrc.exists(), ".zshrc should be created");
    let rc_content = std::fs::read_to_string(&zshrc).expect("should read .zshrc");
    assert!(
        rc_content.contains("source"),
        ".zshrc should source completions"
    );
    assert!(
        rc_content.contains("kno.zsh"),
        ".zshrc should reference kno.zsh"
    );

    install_completions_to(Shell::Zsh, &dir).expect("second zsh install should succeed");
    let rc_after = std::fs::read_to_string(&zshrc).expect("should read .zshrc again");
    assert_eq!(
        rc_content.matches("source").count(),
        rc_after.matches("source").count(),
        "source line should not be duplicated"
    );

    let fish_path = install_completions_to(Shell::Fish, &dir).expect("fish install should succeed");
    assert!(fish_path.exists());

    let ps_path =
        install_completions_to(Shell::PowerShell, &dir).expect("PowerShell install should succeed");
    assert!(ps_path.exists());
    let profile = dir
        .join("Documents")
        .join("PowerShell")
        .join("Microsoft.PowerShell_profile.ps1");
    assert!(profile.exists(), "PowerShell profile should be created");
    let profile_content = std::fs::read_to_string(&profile).expect("should read profile");
    assert!(profile_content.contains("kno.ps1"));

    if cfg!(windows) {
        let legacy_profile = dir
            .join("Documents")
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1");
        assert!(
            legacy_profile.exists(),
            "Windows PowerShell profile should be created"
        );
        let legacy_content =
            std::fs::read_to_string(&legacy_profile).expect("should read legacy profile");
        assert!(legacy_content.contains("kno.ps1"));
    }

    assert!(install_completions_to(Shell::Elvish, &dir).is_err());
    assert!(run_completions_command_with_home(Some("bash"), true, Some(&dir)).is_ok());
    assert!(run_completions_command_with_home(Some("zsh"), true, Some(&dir)).is_ok());
    assert!(run_completions_command_with_home(Some("fish"), true, Some(&dir)).is_ok());
    assert!(run_completions_command_with_home(Some("powershell"), true, Some(&dir)).is_ok());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_completions_command_print_and_error_modes() {
    let mut buf = Vec::new();
    let result = run_completions_command_inner(Some("bash"), false, None, &mut buf);
    assert!(result.is_ok());
    assert!(!buf.is_empty(), "bash completions should be written");
    let mut buf2 = Vec::new();
    let result2 = run_completions_command_inner(None, false, None, &mut buf2);
    assert!(result2.is_ok());
    let mut buf3 = Vec::new();
    let result3 = run_completions_command_inner(Some("nonsense"), false, None, &mut buf3);
    assert!(result3.is_err());
}

#[test]
fn completions_install_path_returns_none_for_unsupported_shell() {
    let home = PathBuf::from("/tmp/test-home");
    assert!(completions_install_path_for_home(Shell::Elvish, &home).is_none());
}

#[test]
fn zsh_completions_are_flat_and_unstyled() {
    let mut buf = Vec::new();
    generate_completions(Shell::Zsh, &mut buf);
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("_describe -t commands 'kno commands' commands \"$@\""),
        "zsh completions should use one flat commands list"
    );
    assert!(
        text.contains("commands=("),
        "zsh completions should define a single commands array"
    );
    assert!(
        !text.contains("Common Commands"),
        "zsh completions should not contain section headers"
    );
    assert!(
        !text.contains("Other Commands"),
        "zsh completions should not contain section headers"
    );
    assert!(
        !text.contains("list-colors"),
        "zsh completions should not inject list-colors styles"
    );
    assert!(
        !text.contains("common_entries"),
        "zsh completions should not define grouped arrays"
    );
    assert!(
        !text.contains("other_entries"),
        "zsh completions should not define grouped arrays"
    );
    assert!(
        !text.contains("compadd -V"),
        "zsh completions should not use grouped compadd sections"
    );
}

#[test]
fn zsh_commands_are_sorted_alphabetically() {
    let mut buf = Vec::new();
    generate_completions(Shell::Zsh, &mut buf);
    let text = String::from_utf8_lossy(&buf);

    let marker = "\n_kno_commands() {";
    let start = text.find(marker).expect("should find _kno_commands");
    let rest = &text[start..];
    let end = rest.find("\n}\n").expect("should find function end");
    let block = &rest[..end + 2];

    let cmds_start = block
        .find("commands=(")
        .expect("commands array should exist");
    let describe_start = block
        .find("_describe -t commands")
        .expect("commands should be passed to _describe");
    let cmds_section = &block[cmds_start..describe_start];
    let names: Vec<&str> = cmds_section
        .lines()
        .filter_map(|line| {
            let entry = line.trim().trim_end_matches(" \\");
            if !entry.starts_with('\'') {
                return None;
            }
            let colon = entry.find(':')?;
            Some(&entry[1..colon])
        })
        .collect();

    assert!(!names.is_empty(), "commands list should not be empty");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "commands should be alphabetical");
    assert!(names.contains(&"init"), "init should exist");
    assert!(names.contains(&"ls"), "ls should exist");
    assert!(names.contains(&"sync"), "sync should exist");
}

#[test]
fn group_zsh_noop_when_function_not_found() {
    let input = "some random script content";
    assert_eq!(group_zsh_command_fn(input, "_kno_commands"), input);
}

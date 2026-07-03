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
    assert_eq!(
        shell_from_path("C:\\Program Files\\Git\\bin\\bash.EXE"),
        Some(Shell::Bash)
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
    #[cfg(target_os = "windows")]
    assert!(powershell.unwrap().to_str().unwrap().contains("PowerShell"));
    #[cfg(not(target_os = "windows"))]
    assert!(powershell.unwrap().to_str().unwrap().contains("powershell"));
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
    let profile = powershell_dir(&dir).join("Microsoft.PowerShell_profile.ps1");
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
fn install_completions_uses_detected_home_environment() {
    let dir = std::env::temp_dir().join(format!("knots-comp-env-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("dir should be creatable");
    let env = crate::test_env::EnvVarGuard::capture(&[
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "HOMEDRIVE",
        "HOMEPATH",
    ]);
    env.set("HOME", &dir);
    env.set("USERPROFILE", &dir);
    env.set("APPDATA", dir.join("AppData").join("Roaming"));
    env.set("LOCALAPPDATA", dir.join("AppData").join("Local"));
    env.remove("HOMEDRIVE");
    env.remove("HOMEPATH");

    let path = install_completions(Shell::Zsh).expect("zsh install should use temp home");
    assert!(path.exists());
    let zshrc = std::fs::read_to_string(dir.join(".zshrc")).expect(".zshrc should read");
    assert!(zshrc.contains("kno.zsh"));

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

#[test]
fn powershell_profile_patch_preserves_utf16le_profiles() {
    let dir = std::env::temp_dir().join(format!("knots-comp-utf16-{}", uuid::Uuid::now_v7()));
    let profile = powershell_profile_paths(&dir)
        .into_iter()
        .next()
        .expect("PowerShell profile path should exist");
    std::fs::create_dir_all(profile.parent().expect("profile should have parent"))
        .expect("profile parent should be creatable");
    let mut bytes = vec![0xFF, 0xFE];
    for word in "# existing\r\n".encode_utf16() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    std::fs::write(&profile, bytes).expect("UTF-16 profile should write");

    patch_powershell_profile_file(&profile, ". 'C:\\Tools\\kno.ps1'")
        .expect("profile patch should succeed");
    let patched = std::fs::read(&profile).expect("patched profile should read");
    assert!(patched.starts_with(&[0xFF, 0xFE]));
    let text = decode_utf16(&patched[2..], true).expect("patched profile should decode");
    assert!(text.contains("# existing"));
    assert!(text.contains("kno.ps1"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn powershell_profile_text_helpers_cover_encodings_and_errors() {
    let dir = std::env::temp_dir().join(format!("knots-comp-enc-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("dir should be creatable");

    let new_profile = dir.join("new").join("profile.ps1");
    patch_powershell_profile_file(&new_profile, ". 'C:\\Tools\\kno.ps1'")
        .expect("new profile should patch");
    let new_content = std::fs::read_to_string(&new_profile).expect("new profile should read");
    assert!(new_content.contains("kno.ps1"));

    let utf8_bom = dir.join("utf8-bom.ps1");
    write_profile_text(&utf8_bom, "# existing\n", ProfileEncoding::Utf8Bom)
        .expect("utf8 bom profile should write");
    let (text, encoding) = read_profile_text(&utf8_bom).expect("utf8 bom profile should read");
    assert_eq!(encoding, ProfileEncoding::Utf8Bom);
    assert!(text.contains("existing"));

    let utf16be = dir.join("utf16be.ps1");
    write_profile_text(&utf16be, "# existing\n", ProfileEncoding::Utf16Be)
        .expect("utf16be profile should write");
    patch_powershell_profile_file(&utf16be, ". 'C:\\Tools\\kno.ps1'")
        .expect("utf16be profile should patch");
    let patched = std::fs::read(&utf16be).expect("utf16be profile should read");
    assert!(patched.starts_with(&[0xFE, 0xFF]));
    let text = decode_utf16(&patched[2..], false).expect("utf16be should decode");
    assert!(text.contains("kno.ps1"));

    let odd = dir.join("odd.ps1");
    std::fs::write(&odd, [0xFF, 0xFE, 0x41]).expect("odd profile should write");
    let err = read_profile_text(&odd).expect_err("odd utf16 should fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn utf16_decode_rejects_lone_surrogates() {
    let dir = std::env::temp_dir().join(format!("knots-comp-surrogate-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("dir should be creatable");
    let bad = dir.join("surrogate.ps1");
    // UTF-16LE BOM followed by a lone high surrogate (0xD800).
    std::fs::write(&bad, [0xFF, 0xFE, 0x00, 0xD8]).expect("fixture should write");

    let err = read_profile_text(&bad).expect_err("lone surrogate should fail decode");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn group_zsh_noop_when_block_end_is_missing() {
    let script = "(( $+functions[_kno_commands] )) || _kno_commands() { local x";
    assert_eq!(group_zsh_toplevel_commands(script), script);
}

#[cfg(target_os = "windows")]
#[test]
fn documents_dir_prefers_known_folder_only_within_home() {
    let home = Path::new(r"C:\Users\example");
    let redirected = Path::new(r"C:\Users\example\OneDrive\Documents");
    assert_eq!(documents_dir_for(home, Some(redirected)), redirected);

    let foreign = Path::new(r"D:\CorpDocs\Documents");
    assert_eq!(
        documents_dir_for(home, Some(foreign)),
        home.join("Documents")
    );
    assert_eq!(documents_dir_for(home, None), home.join("Documents"));
}

#[cfg(target_os = "windows")]
#[test]
fn known_folder_documents_resolves_on_real_windows() {
    // Parallel tests mutate process-global env (PATH and friends), which can
    // fail an individual resolution attempt; failures are not cached, so
    // retry until the environment settles.
    let documents = (0..20)
        .find_map(|attempt| {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            known_folder_documents()
        })
        .expect("known folder should resolve");
    assert!(documents.is_absolute());
}

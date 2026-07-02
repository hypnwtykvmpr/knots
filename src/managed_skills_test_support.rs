use std::path::Path;

pub(super) struct TestHomeEnv {
    _guard: crate::test_env::EnvVarGuard,
}

pub(super) fn set_test_home_env(home: &Path) -> TestHomeEnv {
    let guard = crate::test_env::EnvVarGuard::capture(&[
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "HOMEDRIVE",
        "HOMEPATH",
    ]);
    for (name, value) in [
        ("HOME", home.to_path_buf()),
        ("USERPROFILE", home.to_path_buf()),
        ("APPDATA", home.join("AppData").join("Roaming")),
        ("LOCALAPPDATA", home.join("AppData").join("Local")),
    ] {
        guard.set(name, value);
    }
    guard.remove("HOMEDRIVE");
    guard.remove("HOMEPATH");
    TestHomeEnv { _guard: guard }
}

pub(super) fn restore_test_home_env(prior: TestHomeEnv) {
    drop(prior);
}

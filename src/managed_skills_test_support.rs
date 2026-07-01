use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub(super) fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(super) struct TestHomeEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

pub(super) fn set_test_home_env(home: &Path) -> TestHomeEnv {
    let names = ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"];
    let prior = TestHomeEnv(
        names
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect(),
    );
    for (name, value) in [
        ("HOME", home.to_path_buf()),
        ("USERPROFILE", home.to_path_buf()),
        ("APPDATA", home.join("AppData").join("Roaming")),
        ("LOCALAPPDATA", home.join("AppData").join("Local")),
    ] {
        std::env::set_var(name, value);
    }
    prior
}

pub(super) fn restore_test_home_env(prior: TestHomeEnv) {
    for (name, value) in prior.0 {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

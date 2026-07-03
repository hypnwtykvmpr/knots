use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub(crate) struct EnvVarGuard {
    _lock: MutexGuard<'static, ()>,
    prior: Vec<(&'static str, Option<OsString>)>,
}

impl EnvVarGuard {
    pub(crate) fn capture(names: &[&'static str]) -> Self {
        let lock = env_lock().lock().unwrap_or_else(|err| err.into_inner());
        let prior = names
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect();
        Self { _lock: lock, prior }
    }

    pub(crate) fn set(&self, name: &str, value: impl AsRef<OsStr>) {
        std::env::set_var(name, value);
    }

    pub(crate) fn remove(&self, name: &str) {
        std::env::remove_var(name);
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (name, value) in self.prior.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

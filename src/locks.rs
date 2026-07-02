use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// How long an unparseable (usually empty) lock file is given before being
/// treated as stale. A freshly created lock is legitimately empty for the
/// instant between `create_new` and the holder's PID write landing.
const CORRUPT_LOCK_GRACE: Duration = Duration::from_secs(2);

/// How long a reclaim guard left behind by a crashed reclaimer survives
/// before contenders break it.
const RECLAIM_GUARD_GRACE: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum LockError {
    Busy(PathBuf),
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Busy(path) => write!(f, "lock busy: {}", path.display()),
            LockError::Io(err) => write!(f, "lock I/O error: {}", err),
        }
    }
}

impl std::error::Error for LockError {}

impl From<std::io::Error> for LockError {
    fn from(value: std::io::Error) -> Self {
        LockError::Io(value)
    }
}

#[derive(Debug)]
pub struct FileLock {
    path: PathBuf,
    _file: File,
}

impl FileLock {
    pub fn acquire(path: &Path, timeout: Duration) -> Result<Self, LockError> {
        let start = Instant::now();
        loop {
            match try_acquire(path)? {
                Some(guard) => return Ok(guard),
                None if start.elapsed() >= timeout => {
                    return Err(LockError::Busy(path.to_path_buf()));
                }
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    pub fn try_acquire(path: &Path) -> Result<Option<Self>, LockError> {
        try_acquire(path)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn try_acquire(path: &Path) -> Result<Option<FileLock>, LockError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    loop {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                // Contenders judge staleness by the PID in this file; a
                // failed write must not leave an anonymous lock behind.
                if let Err(err) = write!(file, "{}", std::process::id()) {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                    return Err(LockError::Io(err));
                }
                return Ok(Some(FileLock {
                    path: path.to_path_buf(),
                    _file: file,
                }));
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if reclaim_stale(path)? {
                    continue;
                }
                return Ok(None);
            }
            Err(err) if cfg!(windows) && err.kind() == ErrorKind::PermissionDenied => {
                // Windows reports ERROR_ACCESS_DENIED while the previous
                // holder's file is delete-pending; treat it as busy so
                // acquire() keeps polling instead of failing hard.
                return Ok(None);
            }
            Err(err) => return Err(LockError::Io(err)),
        }
    }
}

/// Read the PID from the lock file and check if that process is alive.
/// If the process is gone, remove the stale lock and return `true`.
fn reclaim_stale(path: &Path) -> Result<bool, LockError> {
    reclaim_stale_with_grace(path, CORRUPT_LOCK_GRACE, RECLAIM_GUARD_GRACE)
}

fn reclaim_stale_with_grace(
    path: &Path,
    corrupt_grace: Duration,
    guard_grace: Duration,
) -> Result<bool, LockError> {
    // Serialize reclamation through a sidecar guard so two contenders can
    // never interleave the dead-check with each other's remove/re-acquire;
    // without this a contender could delete a lock that was just reclaimed
    // and re-acquired by a live process.
    let Some(_guard) = ReclaimGuard::acquire(path, guard_grace)? else {
        return Ok(false);
    };

    let mut contents = String::new();
    match File::open(path).and_then(|mut f| f.read_to_string(&mut contents)) {
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(true),
        Err(_) => return Ok(false),
    }

    let pid: u32 = match contents.trim().parse() {
        Ok(pid) => pid,
        // Unparseable content is usually the creator's PID write that has
        // not landed yet; only treat it as stale once it has aged past the
        // write window.
        Err(_) => {
            return if file_older_than(path, corrupt_grace) {
                remove_stale_lock(path)
            } else {
                Ok(false)
            };
        }
    };

    match process_status(pid) {
        ProcessStatus::Alive | ProcessStatus::Unknown => Ok(false),
        ProcessStatus::Dead => remove_stale_lock(path),
    }
}

struct ReclaimGuard {
    path: PathBuf,
}

impl ReclaimGuard {
    fn acquire(lock_path: &Path, grace: Duration) -> Result<Option<Self>, LockError> {
        let path = reclaim_guard_path(lock_path);
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Some(Self { path })),
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                    // A guard left by a crashed reclaimer is broken only
                    // once it is clearly abandoned.
                    if !file_older_than(&path, grace) {
                        return Ok(None);
                    }
                    match std::fs::remove_file(&path) {
                        Ok(()) => continue,
                        Err(err) if err.kind() == ErrorKind::NotFound => continue,
                        Err(_) => return Ok(None),
                    }
                }
                Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
                Err(err) if err.kind() == ErrorKind::PermissionDenied => return Ok(None),
                Err(err) => return Err(LockError::Io(err)),
            }
        }
    }
}

impl Drop for ReclaimGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn reclaim_guard_path(lock_path: &Path) -> PathBuf {
    let mut name = lock_path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".reclaim");
    lock_path.with_file_name(name)
}

fn file_older_than(path: &Path, age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed >= age)
}

#[cfg(test)]
fn process_alive(pid: u32) -> bool {
    matches!(
        process_status(pid),
        ProcessStatus::Alive | ProcessStatus::Unknown
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessStatus {
    Alive,
    Dead,
    Unknown,
}

fn process_status(pid: u32) -> ProcessStatus {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return ProcessStatus::Dead;
        };
        if pid <= 0 {
            return ProcessStatus::Dead;
        }
        // signal 0 checks if the process exists without sending a signal.
        let ret = unsafe { libc_kill(pid, 0) };
        return if ret == 0 {
            ProcessStatus::Alive
        } else {
            ProcessStatus::Dead
        };
    }
    #[cfg(windows)]
    {
        if pid == 0 {
            return ProcessStatus::Dead;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const SYNCHRONIZE: u32 = 0x00100000;
        const WAIT_OBJECT_0: u32 = 0x00000000;
        const WAIT_TIMEOUT: u32 = 0x00000102;
        const WAIT_FAILED: u32 = 0xFFFFFFFF;
        const ERROR_ACCESS_DENIED: u32 = 5;
        const ERROR_INVALID_PARAMETER: u32 = 87;

        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            let error = unsafe { GetLastError() };
            return match error {
                ERROR_INVALID_PARAMETER => ProcessStatus::Dead,
                ERROR_ACCESS_DENIED => ProcessStatus::Alive,
                _ => ProcessStatus::Unknown,
            };
        }
        let wait_result = unsafe { WaitForSingleObject(handle, 0) };
        unsafe {
            let _ = CloseHandle(handle);
        }
        match wait_result {
            WAIT_TIMEOUT => ProcessStatus::Alive,
            WAIT_OBJECT_0 => ProcessStatus::Dead,
            WAIT_FAILED => ProcessStatus::Unknown,
            _ => ProcessStatus::Unknown,
        }
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        if pid == 0 {
            ProcessStatus::Dead
        } else {
            ProcessStatus::Unknown
        }
    }
}

fn remove_stale_lock(path: &Path) -> Result<bool, LockError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(true),
        Err(err) if err.kind() == ErrorKind::PermissionDenied => Ok(false),
        Err(err) => Err(LockError::Io(err)),
    }
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid, sig) }
}

#[cfg(windows)]
type WindowsHandle = *mut std::ffi::c_void;

#[cfg(windows)]
extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> WindowsHandle;
    fn WaitForSingleObject(handle: WindowsHandle, milliseconds: u32) -> u32;
    fn CloseHandle(handle: WindowsHandle) -> i32;
    fn GetLastError() -> u32;
}

#[cfg(test)]
#[path = "locks_tests.rs"]
mod tests;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

use crate::locks::{FileLock, LockError};
use crate::project::{DistributionMode, StorePaths};

use super::{QueuedWriteRequest, QueuedWriteResponse, WriteOperation};

const REQUESTS_DIR: &str = "writes";
const RESPONSES_DIR: &str = "responses";
const WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub enum QueueError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    Lock(LockError),
    Timeout(Duration),
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::Io(err) => write!(f, "queue I/O error: {}", err),
            QueueError::Serde(err) => write!(f, "queue serialization error: {}", err),
            QueueError::Lock(err) => write!(f, "queue lock error: {}", err),
            QueueError::Timeout(timeout) => {
                write!(
                    f,
                    "timed out waiting for queued write response after {:?}",
                    timeout
                )
            }
        }
    }
}

impl std::error::Error for QueueError {}

impl From<std::io::Error> for QueueError {
    fn from(value: std::io::Error) -> Self {
        QueueError::Io(value)
    }
}

impl From<serde_json::Error> for QueueError {
    fn from(value: serde_json::Error) -> Self {
        QueueError::Serde(value)
    }
}

impl From<LockError> for QueueError {
    fn from(value: LockError) -> Self {
        QueueError::Lock(value)
    }
}

#[derive(Debug, Clone)]
pub(super) struct QueuePaths {
    pub(super) requests_dir: PathBuf,
    pub(super) responses_dir: PathBuf,
    pub(super) worker_lock_path: PathBuf,
}

impl QueuePaths {
    #[cfg(test)]
    pub(super) fn for_repo(repo_root: &Path) -> Self {
        let store_paths = StorePaths {
            root: repo_root.join(".knots"),
        };
        Self::for_store(&store_paths)
    }

    fn for_store(store_paths: &StorePaths) -> Self {
        let queue_dir = store_paths.queue_dir();
        Self {
            requests_dir: queue_dir.join(REQUESTS_DIR),
            responses_dir: queue_dir.join(RESPONSES_DIR),
            worker_lock_path: store_paths.write_queue_worker_lock_path(),
        }
    }

    pub(super) fn create_dirs(&self) -> Result<(), QueueError> {
        fs::create_dir_all(&self.requests_dir)?;
        fs::create_dir_all(&self.responses_dir)?;
        Ok(())
    }
}

#[cfg(test)]
pub fn enqueue_and_wait<F>(
    repo_root: &Path,
    db_path: &str,
    operation: WriteOperation,
    executor: F,
) -> Result<QueuedWriteResponse, QueueError>
where
    F: FnMut(&QueuedWriteRequest) -> QueuedWriteResponse,
{
    enqueue_and_wait_with_context(
        repo_root,
        &StorePaths {
            root: repo_root.join(".knots"),
        },
        DistributionMode::Git,
        None,
        db_path,
        operation,
        executor,
    )
}

pub fn enqueue_and_wait_with_context<F>(
    repo_root: &Path,
    store_paths: &StorePaths,
    distribution: DistributionMode,
    project_id: Option<String>,
    db_path: &str,
    operation: WriteOperation,
    mut executor: F,
) -> Result<QueuedWriteResponse, QueueError>
where
    F: FnMut(&QueuedWriteRequest) -> QueuedWriteResponse,
{
    let absolute_root = canonical_or_original(repo_root);
    let absolute_store = canonical_or_original(&store_paths.root);
    let absolute_db = to_absolute_db_path(&absolute_store, db_path);
    let paths = QueuePaths::for_store(&StorePaths {
        root: absolute_store.clone(),
    });
    paths.create_dirs()?;

    let request_id = uuid::Uuid::now_v7().to_string();
    let response_path = paths
        .responses_dir
        .join(format!("{}.json", request_id))
        .display()
        .to_string();
    let request = QueuedWriteRequest {
        request_id: request_id.clone(),
        repo_root: absolute_root.display().to_string(),
        store_root: absolute_store.display().to_string(),
        distribution: match distribution {
            DistributionMode::Git => "git".to_string(),
            DistributionMode::LocalOnly => "local_only".to_string(),
        },
        project_id,
        db_path: absolute_db,
        response_path: response_path.clone(),
        operation,
    };
    enqueue_request(&paths, &request)?;

    let start = std::time::Instant::now();
    loop {
        drain_pending_requests(&paths, &mut executor)?;
        if let Some(response) = read_response_file(Path::new(&response_path))? {
            let _ = fs::remove_file(&response_path);
            return Ok(response);
        }
        if start.elapsed() >= WAIT_TIMEOUT {
            return Err(QueueError::Timeout(WAIT_TIMEOUT));
        }
        thread::sleep(WAIT_POLL_INTERVAL);
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn to_absolute_db_path(base_root: &Path, db_path: &str) -> String {
    let db = Path::new(db_path);
    if db.is_absolute() {
        db.display().to_string()
    } else {
        base_root.join(db).display().to_string()
    }
}

pub(super) fn enqueue_request(
    paths: &QueuePaths,
    request: &QueuedWriteRequest,
) -> Result<(), QueueError> {
    let request_path =
        paths
            .requests_dir
            .join(format!("{}-{}.json", now_nanos(), request.request_id));
    let temp_path = request_path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(request)?;
    fs::write(&temp_path, bytes)?;
    fs::rename(temp_path, request_path)?;
    Ok(())
}

pub(super) fn drain_pending_requests<F>(
    paths: &QueuePaths,
    executor: &mut F,
) -> Result<usize, QueueError>
where
    F: FnMut(&QueuedWriteRequest) -> QueuedWriteResponse,
{
    let worker_lock = FileLock::try_acquire(&paths.worker_lock_path)?;
    let Some(_worker_guard) = worker_lock else {
        return Ok(0);
    };

    let mut processed = 0usize;
    loop {
        let request_files = list_request_files(&paths.requests_dir)?;
        if request_files.is_empty() {
            break;
        }
        for request_file in request_files {
            let request = match read_request_file(&request_file) {
                Ok(request) => request,
                Err(err) => {
                    let _ = fs::remove_file(&request_file);
                    return Err(err);
                }
            };
            let response = executor(&request);
            let response_path = PathBuf::from(&request.response_path);
            write_response_file(&response_path, &response)?;
            let _ = fs::remove_file(&request_file);
            processed += 1;
        }
    }

    Ok(processed)
}

pub(super) fn list_request_files(requests_dir: &Path) -> Result<Vec<PathBuf>, QueueError> {
    let mut files = Vec::new();
    if !requests_dir.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(requests_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn read_request_file(path: &Path) -> Result<QueuedWriteRequest, QueueError> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_response_file(path: &Path, response: &QueuedWriteResponse) -> Result<(), QueueError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(response)?;
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub(super) fn read_response_file(path: &Path) -> Result<Option<QueuedWriteResponse>, QueueError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX_EPOCH")
        .as_nanos()
}

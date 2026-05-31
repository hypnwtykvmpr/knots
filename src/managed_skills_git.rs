use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn changed_tracked_paths(repo_root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    if paths.is_empty() {
        return Vec::new();
    }

    let relative_paths = paths
        .iter()
        .map(|path| repo_relative_path(repo_root, path))
        .collect::<Vec<_>>();
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=no",
            "--",
        ])
        .args(&relative_paths)
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let changed = status_paths(&output.stdout).collect::<HashSet<_>>();
    relative_paths
        .iter()
        .zip(paths)
        .filter(|(relative, _)| changed.contains(*relative))
        .map(|(_, path)| path.clone())
        .collect()
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(repo_root).unwrap_or(path).to_path_buf()
}

fn status_paths(output: &[u8]) -> impl Iterator<Item = PathBuf> + '_ {
    output.split(|byte| *byte == 0).filter_map(status_path)
}

fn status_path(entry: &[u8]) -> Option<PathBuf> {
    if entry.len() < 4 || entry[2] != b' ' {
        return None;
    }
    Some(PathBuf::from(
        String::from_utf8_lossy(&entry[3..]).into_owned(),
    ))
}

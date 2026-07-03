//! `nested_caches` doctor check.
//!
//! Detects `.knots` cache directories nested inside the canonical store. Nested
//! caches cause silent state drift, so the check is detection-only and emits
//! manual removal guidance for each offender rather than auto-deleting.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::doctor::{DoctorCheck, DoctorError, DoctorStatus};
use crate::project::StorePaths;

const MAX_WALK_DEPTH: usize = 8;

pub fn check_nested_caches_at(store_paths: &StorePaths) -> Result<DoctorCheck, DoctorError> {
    let root = &store_paths.root;
    if !root.is_dir() {
        return Ok(pass());
    }

    let nested = scan_nested_caches(root)?;
    if nested.is_empty() {
        return Ok(pass());
    }

    let mut detail = format!(
        "found {} nested .knots cache(s); remove manually:\n",
        nested.len()
    );
    for path in &nested {
        detail.push_str("  ");
        detail.push_str(&removal_hint(path));
        detail.push('\n');
    }
    let data = serde_json::json!({
        "nested": nested
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    });
    Ok(DoctorCheck {
        name: "nested_caches".to_string(),
        status: DoctorStatus::Warn,
        detail: detail.trim_end().to_string(),
        data: Some(data),
    })
}

fn pass() -> DoctorCheck {
    DoctorCheck::simple(
        "nested_caches",
        DoctorStatus::Pass,
        "no nested .knots caches detected",
    )
}

fn scan_nested_caches(root: &Path) -> Result<Vec<PathBuf>, DoctorError> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_WALK_DEPTH {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if name == ".git" {
                continue;
            }
            if name == ".knots" && path != *root && is_cache_dir(&path) {
                found.push(path.clone());
                // Do not descend into a nested cache; siblings inside it
                // do not constitute additional independent nestings.
                continue;
            }
            queue.push_back((path, depth + 1));
        }
    }

    found.sort();
    Ok(found)
}

fn is_cache_dir(path: &Path) -> bool {
    let cache = path.join("cache");
    cache.join("state.sqlite").exists() || cache.join("cache.lock").exists()
}

fn removal_hint(path: &Path) -> String {
    #[cfg(windows)]
    {
        format!(
            "Remove-Item -LiteralPath {} -Recurse -Force",
            powershell_single_quoted(path)
        )
    }
    #[cfg(not(windows))]
    {
        format!("rm -rf {}", path.display())
    }
}

#[cfg(windows)]
fn powershell_single_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

#[cfg(test)]
#[path = "doctor_nested_cache_tests.rs"]
mod tests;

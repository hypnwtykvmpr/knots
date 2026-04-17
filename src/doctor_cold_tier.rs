//! `cold_tier_imbalance` doctor check + --fix helper.
//!
//! The check warns when the hot cache is below the hot target but the cold
//! catalog still contains knots that could be brought back. `kno doctor --fix`
//! rehydrates the newest-first cold records up to the available headroom
//! (`COLD_TIER_HOT_TARGET - hot_count`).

use std::path::Path;

use rusqlite::Connection;

use crate::app::App;
use crate::db;
use crate::doctor::{DoctorCheck, DoctorError, DoctorStatus};

/// Target hot-cache size used by the imbalance check / fix.
pub const COLD_TIER_HOT_TARGET: i64 = 100;

pub fn check_cold_tier_imbalance_at(
    store_paths: &crate::project::StorePaths,
) -> Result<DoctorCheck, DoctorError> {
    let db_path = store_paths.db_path();
    if !db_path.exists() {
        return Ok(DoctorCheck::simple(
            "cold_tier_imbalance",
            DoctorStatus::Pass,
            "no cache database found",
        ));
    }
    let conn = crate::db::open_connection(db_path.to_str().unwrap_or("cache/state.sqlite"))
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    check_cold_tier_imbalance(&conn)
}

pub fn check_cold_tier_imbalance(conn: &Connection) -> Result<DoctorCheck, DoctorError> {
    let hot = db::count_knot_hot(conn)
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    let cold = db::count_cold_catalog(conn)
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    let shadowed = db::count_cold_catalog_shadowed_by_hot(conn)
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    // Cold rows whose id is already present in hot cannot be rehydrated into
    // hot — they are stale duplicates, and counting them here produced a
    // permanent warn that `doctor --fix` could never clear.
    let effective_cold = (cold - shadowed).max(0);
    let data =
        Some(serde_json::json!({ "hot_count": hot, "cold_count": cold, "shadowed": shadowed }));
    if hot >= COLD_TIER_HOT_TARGET || effective_cold == 0 {
        if shadowed > 0 {
            return Ok(with_data(
                DoctorStatus::Warn,
                format!(
                    "{hot} hot / {cold} cold ({shadowed} shadowed by hot); \
                     run doctor --fix to prune shadowed rows",
                ),
                data,
            ));
        }
        return Ok(with_data(
            DoctorStatus::Pass,
            format!("{hot} hot / {cold} cold"),
            data,
        ));
    }
    let cap = (COLD_TIER_HOT_TARGET - hot).min(effective_cold);
    let shadowed_suffix = if shadowed > 0 {
        format!(" ({shadowed} shadowed by hot will be pruned)")
    } else {
        String::new()
    };
    Ok(with_data(
        DoctorStatus::Warn,
        format!(
            "{hot} hot / {cold} cold; run doctor --fix to rehydrate up to {cap}{shadowed_suffix}",
        ),
        data,
    ))
}

fn with_data(status: DoctorStatus, detail: String, data: Option<serde_json::Value>) -> DoctorCheck {
    DoctorCheck {
        name: "cold_tier_imbalance".to_string(),
        status,
        detail,
        data,
    }
}

/// Implements the `--fix` action for `cold_tier_imbalance`. Opens the App at
/// `repo_root` and rehydrates newest-first cold records until the hot count
/// reaches the target (or cold is drained). Errors from individual rehydrates
/// are swallowed so that a single failure does not abort the sweep — the next
/// run will retry the remainder.
pub fn fix_cold_tier_imbalance(repo_root: &Path) {
    let db_path = repo_root.join(".knots").join("cache").join("state.sqlite");
    if !db_path.exists() {
        return;
    }
    let Some(db_str) = db_path.to_str() else {
        return;
    };
    let Ok(app) = App::open(db_str, repo_root.to_path_buf()) else {
        return;
    };
    let Ok(conn) = crate::db::open_connection(db_str) else {
        return;
    };
    // Prune cold rows whose id is already in hot first — these are stale
    // duplicates that would otherwise keep the imbalance warning lit on
    // every subsequent run.
    let _ = db::prune_cold_catalog_shadowed_by_hot(&conn);
    let Ok(hot) = db::count_knot_hot(&conn) else {
        return;
    };
    if hot >= COLD_TIER_HOT_TARGET {
        return;
    }
    let cap = (COLD_TIER_HOT_TARGET - hot) as usize;
    let Ok(cold_records) = db::list_cold_catalog_not_in_hot(&conn) else {
        return;
    };
    drop(conn);
    for record in cold_records.iter().take(cap) {
        let _ = app.rehydrate(&record.id);
    }
}

#[cfg(test)]
#[path = "doctor_cold_tier_tests.rs"]
mod tests;

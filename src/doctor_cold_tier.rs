//! `cold_tier_imbalance` doctor check + --fix helper.
//!
//! Measures three real tier invariants and surfaces any violation as `WARN`:
//!
//! 1. **Disjointness.** No id appears in both `knot_hot` and `cold_catalog`.
//! 2. **Cold is terminal-only.** Every `cold_catalog` row's `state` is in
//!    `tiering::TERMINAL_STATES`.
//! 3. **No stale-terminal hot rows.** No `knot_hot` row has a terminal state
//!    AND `updated_at < now - ARCHIVE_AGE_HOURS` (72h).
//!
//! `--fix` restores each invariant: prune shadow rows, rehydrate non-terminal
//! cold rows back to hot (or drop the cold pointer if events are missing),
//! and demote stale-terminal hot rows to cold via the same upsert+delete
//! pair the cold sweep uses. After one `--fix`, the everyday flows
//! (`run_cold_sweep`, `sync::apply`, snapshot bootstrap) all uphold the
//! invariants, so doctor stays `pass` through normal use.
//!
//! See `docs/tier-balance.md` for the user-facing description.

use std::path::Path;

use rusqlite::Connection;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::app::App;
use crate::db;
use crate::doctor::{DoctorCheck, DoctorError, DoctorStatus};
use crate::tiering::{ARCHIVE_AGE_HOURS, TERMINAL_STATES};

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
    let now = OffsetDateTime::now_utc();
    check_cold_tier_imbalance_at_time(conn, now)
}

fn check_cold_tier_imbalance_at_time(
    conn: &Connection,
    now: OffsetDateTime,
) -> Result<DoctorCheck, DoctorError> {
    let cutoff = format_rfc3339(now - Duration::hours(ARCHIVE_AGE_HOURS));
    let hot = db::count_knot_hot(conn).map_err(io_err)?;
    let cold = db::count_cold_catalog(conn).map_err(io_err)?;
    let shadow = db::count_cold_catalog_shadowed_by_hot(conn).map_err(io_err)?;
    let non_terminal_cold =
        db::count_non_terminal_in_cold(conn, TERMINAL_STATES).map_err(io_err)?;
    let stale_terminal_hot =
        db::count_stale_terminal_in_hot(conn, TERMINAL_STATES, &cutoff).map_err(io_err)?;

    let data = Some(serde_json::json!({
        "hot_count": hot,
        "cold_count": cold,
        "shadow": shadow,
        "non_terminal_cold": non_terminal_cold,
        "stale_terminal_hot": stale_terminal_hot,
    }));

    if shadow == 0 && non_terminal_cold == 0 && stale_terminal_hot == 0 {
        return Ok(with_data(
            DoctorStatus::Pass,
            format!("{hot} hot / {cold} cold; tier invariants hold"),
            data,
        ));
    }

    Ok(with_data(
        DoctorStatus::Warn,
        format!(
            "shadow={shadow} non_terminal_cold={non_terminal_cold} \
             stale_terminal_hot={stale_terminal_hot}; run doctor --fix",
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

fn io_err<E: std::fmt::Display>(err: E) -> DoctorError {
    DoctorError::Io(std::io::Error::other(err.to_string()))
}

fn format_rfc3339(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .expect("RFC3339 formatting for UTC timestamp should never fail")
}

/// Implements the `--fix` action for `cold_tier_imbalance`. Restores the three
/// tier invariants:
///
/// 1. Prune cold rows whose id is also in hot (shadow).
/// 2. Rehydrate non-terminal cold rows. If event replay fails, drop the cold
///    pointer — the warm row stays so the catalog still remembers it.
/// 3. Demote stale-terminal hot rows to cold (mirrors `run_cold_sweep`'s
///    move).
///
/// Each step is idempotent; running `--fix` a second time is a no-op.
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

    // Step 1: prune shadow rows.
    let _ = db::prune_cold_catalog_shadowed_by_hot(&conn);

    // Step 2: rehydrate non-terminal cold rows; fall back to delete if events
    // are missing so the warning can clear.
    let non_terminal = db::list_non_terminal_in_cold(&conn, TERMINAL_STATES).unwrap_or_default();
    drop(conn);
    for record in &non_terminal {
        if app.rehydrate(&record.id).is_err() {
            if let Ok(c) = crate::db::open_connection(db_str) {
                let _ = db::delete_cold_catalog(&c, &record.id);
            }
        }
    }

    // Step 3: demote stale-terminal hot rows to cold.
    let Ok(conn) = crate::db::open_connection(db_str) else {
        return;
    };
    let cutoff = format_rfc3339(OffsetDateTime::now_utc() - Duration::hours(ARCHIVE_AGE_HOURS));
    let stale = db::list_stale_terminal_in_hot(&conn, TERMINAL_STATES, &cutoff).unwrap_or_default();
    for (id, title, state, updated_at) in stale {
        let _ = db::upsert_cold_catalog(&conn, &id, &title, &state, &updated_at);
        let _ = crate::db::delete_knot_hot(&conn, &id);
    }
}

#[cfg(test)]
#[path = "doctor_cold_tier_tests.rs"]
mod tests;

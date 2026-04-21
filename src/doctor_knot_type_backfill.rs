//! `knot_type_backfill` doctor check + --fix helper.
//!
//! An earlier build of `build_index_upsert` never read `type` from
//! `idx.knot_head` event data, so any knot first materialized into the
//! local SQLite cache via a pull from origin ended up with `knot_type`
//! NULL or empty. That makes `kno ls --type <type>` filters silently drop
//! the knot even though the event log has the correct value.
//!
//! The check counts rows in `knot_hot` with an empty `knot_type`. The
//! fix scans the worktree's `.knots/index/` for the latest
//! `idx.knot_head` event per affected knot, extracts `data.type`, and
//! writes it back to the cache row.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::doctor::{DoctorCheck, DoctorError, DoctorStatus};
use crate::project::StorePaths;

pub fn check_knot_type_backfill_at(store_paths: &StorePaths) -> Result<DoctorCheck, DoctorError> {
    let db_path = store_paths.db_path();
    if !db_path.exists() {
        return Ok(DoctorCheck::simple(
            "knot_type_backfill",
            DoctorStatus::Pass,
            "no cache database found",
        ));
    }
    let conn = crate::db::open_connection(db_path.to_str().unwrap_or("cache/state.sqlite"))
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    let count = count_empty_knot_type(&conn)
        .map_err(|err| DoctorError::Io(std::io::Error::other(err.to_string())))?;
    if count == 0 {
        return Ok(DoctorCheck::simple(
            "knot_type_backfill",
            DoctorStatus::Pass,
            "all hot cache rows have knot_type populated",
        ));
    }
    Ok(DoctorCheck::simple(
        "knot_type_backfill",
        DoctorStatus::Warn,
        format!(
            "{count} hot cache row(s) have an empty knot_type; \
             run doctor --fix to backfill from the event log"
        ),
    ))
}

fn count_empty_knot_type(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM knot_hot WHERE knot_type IS NULL OR knot_type = ''",
        [],
        |row| row.get(0),
    )
}

/// Implements the `--fix` action for `knot_type_backfill`. Reads the
/// worktree index once, builds a knot_id → latest-type map, then issues
/// one UPDATE per affected knot. Knots without a discoverable type in the
/// worktree are left alone; the next event that names the type will
/// populate them via the fixed sync-apply path.
pub fn fix_knot_type_backfill(repo_root: &Path) {
    let store_paths = StorePaths {
        root: repo_root.join(".knots"),
    };
    let db_path = store_paths.db_path();
    let Some(db_str) = db_path.to_str() else {
        return;
    };
    let Ok(conn) = crate::db::open_connection(db_str) else {
        return;
    };
    let Ok(empty_ids) = list_empty_knot_type_ids(&conn) else {
        return;
    };
    if empty_ids.is_empty() {
        return;
    }
    let worktree_index = store_paths.worktree_path().join(".knots").join("index");
    let type_by_id = scan_latest_types(&worktree_index);
    for knot_id in empty_ids {
        if let Some(knot_type) = type_by_id.get(&knot_id) {
            let _ = conn.execute(
                "UPDATE knot_hot SET knot_type = ?1 WHERE id = ?2",
                params![knot_type, knot_id],
            );
        }
    }
}

fn list_empty_knot_type_ids(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM knot_hot WHERE knot_type IS NULL OR knot_type = ''")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

struct LatestType {
    occurred_at: OffsetDateTime,
    knot_type: String,
}

fn scan_latest_types(index_root: &Path) -> HashMap<String, String> {
    let mut latest: HashMap<String, LatestType> = HashMap::new();
    if !index_root.exists() {
        return HashMap::new();
    }
    let mut stack = vec![index_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-idx.knot_head.json"))
            {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(event) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let Some(data) = event.get("data").and_then(Value::as_object) else {
                continue;
            };
            let Some(knot_id) = data.get("knot_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(knot_type) = data.get("type").and_then(Value::as_str) else {
                continue;
            };
            if knot_type.trim().is_empty() {
                continue;
            }
            let Some(occurred) = event.get("occurred_at").and_then(Value::as_str) else {
                continue;
            };
            let Ok(ts) = OffsetDateTime::parse(occurred, &Rfc3339) else {
                continue;
            };
            latest
                .entry(knot_id.to_string())
                .and_modify(|entry| {
                    if ts > entry.occurred_at {
                        entry.occurred_at = ts;
                        entry.knot_type = knot_type.to_string();
                    }
                })
                .or_insert(LatestType {
                    occurred_at: ts,
                    knot_type: knot_type.to_string(),
                });
        }
    }
    latest.into_iter().map(|(k, v)| (k, v.knot_type)).collect()
}

#[cfg(test)]
#[path = "doctor_knot_type_backfill_tests.rs"]
mod tests;

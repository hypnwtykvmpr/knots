use rusqlite::Connection;
use serde_json::Value;
use time::OffsetDateTime;

use crate::db;
use crate::tiering::{classify_knot_tier, CacheTier};

use super::SyncError;

pub(super) enum FullApplyOutcome {
    EdgeAdded,
    EdgeRemoved,
    Ignored,
}

pub(super) fn resolve_tier(
    conn: &Connection,
    data: &serde_json::Map<String, Value>,
    state: &str,
    updated_at: &str,
) -> Result<CacheTier, SyncError> {
    let hot_window_days = db::get_hot_window_days(conn)?;
    let terminal_flag = data
        .get("terminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now = OffsetDateTime::now_utc();
    if terminal_flag {
        Ok(CacheTier::Cold)
    } else {
        Ok(classify_knot_tier(state, updated_at, hot_window_days, now))
    }
}

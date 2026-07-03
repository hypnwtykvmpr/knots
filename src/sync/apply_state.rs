use rusqlite::Connection;
use serde_json::Value;
use time::OffsetDateTime;

use crate::db;
use crate::tiering::{classify_knot_head_tier, CacheTier};

use super::SyncError;

#[derive(Debug)]
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
    Ok(classify_knot_head_tier(
        state,
        updated_at,
        terminal_flag,
        hot_window_days,
        now,
    ))
}

// Leaf helpers relocated from apply.rs to keep that file under the
// size limit; they operate purely on parsed events.
use super::apply_helpers::invalid_event;
use super::SyncSummary;
use crate::events::{FullEvent, IndexEvent};
use std::path::Path;

pub(super) fn sync_summary(target_head: &str, index_len: usize, full_len: usize) -> SyncSummary {
    SyncSummary::new(target_head.to_string(), index_len as u64, full_len as u64)
}

pub(super) fn index_event_data<'a>(
    event: &'a IndexEvent,
    path: &Path,
) -> Result<&'a serde_json::Map<String, Value>, SyncError> {
    event
        .data
        .as_object()
        .ok_or_else(|| invalid_event(path, "idx.knot_head data must be an object"))
}

pub(super) fn full_event_data<'a>(
    event: &'a FullEvent,
    path: &Path,
) -> Result<&'a serde_json::Map<String, Value>, SyncError> {
    event
        .data
        .as_object()
        .ok_or_else(|| invalid_event(path, "full event data must be an object"))
}
pub(super) fn unknown_workflow_warning(knot_id: &str, workflow_id: &str) -> String {
    format!(
        "warning: can't import knot '{knot_id}', unknown workflow '{workflow_id}'. \
         The knot creator should install the workflow into the repository \
         so other users can view the knot."
    )
}

//! `workflow_id_parity` doctor check + --fix helper.
//!
//! Legacy `idx.knot_head` events predate the `workflow_id` field. Sync
//! infers `workflow_id` at apply time to populate the local cache, but the
//! shared event log stays "dirty" — other consumers must re-infer on
//! bootstrap. The check identifies knots whose latest `idx.knot_head` event
//! in the synced worktree lacks `workflow_id`; the fix appends a minimal
//! repair event carrying the DB's current `workflow_id`, which publishes on
//! the next sync.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::db;
use crate::doctor::{DoctorCheck, DoctorError, DoctorStatus};
use crate::events::{EventRecord, EventWriter, IndexEvent, IndexEventKind};
use crate::installed_workflows;
use crate::project::StorePaths;

struct StaleHead {
    knot_id: String,
    profile_id: Option<String>,
    knot_type_str: Option<String>,
}

struct ScanEntry {
    occurred_at: OffsetDateTime,
    has_workflow_id: bool,
    profile_id: Option<String>,
    knot_type_str: Option<String>,
}

pub fn check_workflow_id_parity_at(store_paths: &StorePaths) -> Result<DoctorCheck, DoctorError> {
    let worktree_knots = store_paths.worktree_path().join(".knots");
    let stale = scan_stale_heads(&worktree_knots);
    if stale.is_empty() {
        return Ok(DoctorCheck::simple(
            "workflow_id_parity",
            DoctorStatus::Pass,
            "all latest idx.knot_head events include workflow_id",
        ));
    }
    let count = stale.len();
    Ok(DoctorCheck::simple(
        "workflow_id_parity",
        DoctorStatus::Warn,
        format!(
            "{count} knot(s) have a latest idx.knot_head event missing \
             workflow_id; run doctor --fix to publish repair events"
        ),
    ))
}

/// Implements the `--fix` action for `workflow_id_parity`. For each knot with
/// a stale latest event, emits a minimal `idx.knot_head` event into
/// `.knots/index/...` using the DB's current state (hot) or cold-catalog row
/// combined with profile/type carried forward from the stale event. Failures
/// are swallowed so one bad row doesn't abort the sweep.
pub fn fix_workflow_id_parity(repo_root: &Path) {
    let store_paths = StorePaths {
        root: repo_root.join(".knots"),
    };
    let stale = scan_stale_heads(&store_paths.worktree_path().join(".knots"));
    if stale.is_empty() {
        return;
    }
    let db_path = store_paths.db_path();
    let Some(db_str) = db_path.to_str() else {
        return;
    };
    let Ok(conn) = db::open_connection(db_str) else {
        return;
    };
    let writer = EventWriter::new(store_paths.root.clone());
    for head in stale {
        let _ = emit_repair_event(&conn, &writer, &head);
    }
}

fn emit_repair_event(
    conn: &Connection,
    writer: &EventWriter,
    head: &StaleHead,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(payload) = build_repair_payload_from_hot(conn, head)? {
        writer.write(&EventRecord::index(IndexEvent::new(
            IndexEventKind::KnotHead,
            payload,
        )))?;
        return Ok(());
    }
    if let Some(payload) = build_repair_payload_from_cold(conn, head)? {
        writer.write(&EventRecord::index(IndexEvent::new(
            IndexEventKind::KnotHead,
            payload,
        )))?;
    }
    Ok(())
}

fn build_repair_payload_from_hot(
    conn: &Connection,
    head: &StaleHead,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let Some(record) = db::get_knot_hot(conn, &head.knot_id)? else {
        return Ok(None);
    };
    if record.workflow_id.trim().is_empty() {
        return Ok(None);
    }
    let knot_type_str = record
        .knot_type
        .as_deref()
        .or(head.knot_type_str.as_deref())
        .unwrap_or("work");
    Ok(Some(json!({
        "knot_id": record.id,
        "title": record.title,
        "state": record.state,
        "updated_at": record.updated_at,
        "profile_id": record.profile_id,
        "workflow_id": record.workflow_id,
        "type": knot_type_str,
        "terminal": false,
    })))
}

fn build_repair_payload_from_cold(
    conn: &Connection,
    head: &StaleHead,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let Some(record) = db::get_cold_catalog(conn, &head.knot_id)? else {
        return Ok(None);
    };
    let knot_type_str = head.knot_type_str.as_deref().unwrap_or("work");
    let knot_type = crate::domain::knot_type::parse_knot_type(Some(knot_type_str));
    let workflow_id = installed_workflows::builtin_workflow_id_for_knot_type(knot_type);
    let profile_id = head.profile_id.as_deref().unwrap_or("autopilot");
    Ok(Some(json!({
        "knot_id": record.id,
        "title": record.title,
        "state": record.state,
        "updated_at": record.updated_at,
        "profile_id": profile_id,
        "workflow_id": workflow_id,
        "type": knot_type_str,
        "terminal": true,
    })))
}

fn scan_stale_heads(worktree_knots_root: &Path) -> Vec<StaleHead> {
    let index_root = worktree_knots_root.join("index");
    if !index_root.exists() {
        return Vec::new();
    }
    let mut latest: HashMap<String, ScanEntry> = HashMap::new();
    let mut stack = vec![index_root];
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
            if is_knot_head_file(&path) {
                process_event_file(&path, &mut latest);
            }
        }
    }
    let mut stale: Vec<StaleHead> = latest
        .into_iter()
        .filter_map(|(knot_id, entry)| {
            if entry.has_workflow_id {
                None
            } else {
                Some(StaleHead {
                    knot_id,
                    profile_id: entry.profile_id,
                    knot_type_str: entry.knot_type_str,
                })
            }
        })
        .collect();
    stale.sort_by(|a, b| a.knot_id.cmp(&b.knot_id));
    stale
}

fn is_knot_head_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "json")
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("-idx.knot_head.json"))
}

fn process_event_file(path: &Path, latest: &mut HashMap<String, ScanEntry>) {
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let Ok(event) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let Some(data) = event.get("data").and_then(Value::as_object) else {
        return;
    };
    let Some(knot_id) = data.get("knot_id").and_then(Value::as_str) else {
        return;
    };
    let Some(occurred) = event.get("occurred_at").and_then(Value::as_str) else {
        return;
    };
    let Ok(ts) = OffsetDateTime::parse(occurred, &Rfc3339) else {
        return;
    };
    let has_workflow_id = data
        .get("workflow_id")
        .and_then(Value::as_str)
        .is_some_and(|v| !v.trim().is_empty());
    let profile_id = data
        .get("profile_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let knot_type_str = data.get("type").and_then(Value::as_str).map(str::to_string);
    let new_entry = ScanEntry {
        occurred_at: ts,
        has_workflow_id,
        profile_id,
        knot_type_str,
    };
    latest
        .entry(knot_id.to_string())
        .and_modify(|entry| {
            if ts > entry.occurred_at {
                *entry = ScanEntry {
                    occurred_at: ts,
                    has_workflow_id,
                    profile_id: new_entry.profile_id.clone(),
                    knot_type_str: new_entry.knot_type_str.clone(),
                };
            }
        })
        .or_insert(new_entry);
}

#[cfg(test)]
#[path = "doctor_workflow_parity_tests.rs"]
mod tests;

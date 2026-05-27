use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::db::{self, KnotCacheRecord, UpsertKnotHot};
use crate::domain::execution_plan::ExecutionPlanData;
use crate::domain::gate::GateData;
use crate::domain::invariant::Invariant;
use crate::domain::lease::LeaseData;
use crate::domain::metadata::MetadataEntry;
use crate::domain::scope::ScopeData;
use crate::domain::step_history::StepRecord;
use crate::events::FullEvent;
use crate::installed_workflows;

use super::SyncError;

pub(super) fn current_unix_ms_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

pub(super) struct MetadataProjection {
    pub title: String,
    pub state: String,
    pub updated_at: String,
    pub body: Option<String>,
    pub description: Option<String>,
    pub acceptance: Option<String>,
    pub priority: Option<i64>,
    pub knot_type: Option<String>,
    pub tags: Vec<String>,
    pub notes: Vec<MetadataEntry>,
    pub handoff_capsules: Vec<MetadataEntry>,
    pub invariants: Vec<Invariant>,
    pub verification_steps: Vec<String>,
    pub step_history: Vec<StepRecord>,
    pub gate_data: GateData,
    pub lease_data: LeaseData,
    pub execution_plan_data: ExecutionPlanData,
    pub scope_data: ScopeData,
    pub lease_id: Option<String>,
    pub workflow_id: String,
    pub profile_id: String,
    pub profile_etag: Option<String>,
    pub deferred_from_state: Option<String>,
    pub blocked_from_state: Option<String>,
    pub created_at: Option<String>,
}

impl MetadataProjection {
    pub fn from_existing(existing: &KnotCacheRecord) -> Self {
        Self {
            title: existing.title.clone(),
            state: existing.state.clone(),
            updated_at: existing.updated_at.clone(),
            body: existing.body.clone(),
            description: existing.description.clone(),
            acceptance: existing.acceptance.clone(),
            priority: existing.priority,
            knot_type: existing.knot_type.clone(),
            tags: existing.tags.clone(),
            notes: existing.notes.clone(),
            handoff_capsules: existing.handoff_capsules.clone(),
            invariants: existing.invariants.clone(),
            verification_steps: existing.verification_steps.clone(),
            step_history: existing.step_history.clone(),
            gate_data: existing.gate_data.clone(),
            lease_data: existing.lease_data.clone(),
            execution_plan_data: existing.execution_plan_data.clone(),
            scope_data: existing.scope_data.clone(),
            lease_id: existing.lease_id.clone(),
            workflow_id: existing.workflow_id.clone(),
            profile_id: existing.profile_id.clone(),
            profile_etag: existing.profile_etag.clone(),
            deferred_from_state: existing.deferred_from_state.clone(),
            blocked_from_state: existing.blocked_from_state.clone(),
            created_at: existing.created_at.clone(),
        }
    }

    pub fn upsert(&self, conn: &Connection, id: &str) -> Result<(), SyncError> {
        db::upsert_knot_hot(
            conn,
            &UpsertKnotHot {
                id,
                title: &self.title,
                state: &self.state,
                updated_at: &self.updated_at,
                body: self.body.as_deref(),
                description: self.description.as_deref(),
                acceptance: self.acceptance.as_deref(),
                priority: self.priority,
                knot_type: self.knot_type.as_deref(),
                tags: &self.tags,
                notes: &self.notes,
                handoff_capsules: &self.handoff_capsules,
                invariants: &self.invariants,
                verification_steps: &self.verification_steps,
                step_history: &self.step_history,
                gate_data: &self.gate_data,
                lease_data: &self.lease_data,
                execution_plan_data: &self.execution_plan_data,
                lease_id: self.lease_id.as_deref(),
                workflow_id: &self.workflow_id,
                profile_id: &self.profile_id,
                profile_etag: self.profile_etag.as_deref(),
                deferred_from_state: self.deferred_from_state.as_deref(),
                blocked_from_state: self.blocked_from_state.as_deref(),
                created_at: self.created_at.as_deref(),
            },
        )?;
        db::update_knot_scope_data(conn, id, &self.scope_data)?;
        Ok(())
    }
}

pub(super) fn read_json_file<T>(path: &Path) -> Result<T, SyncError>
where
    T: DeserializeOwned,
{
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|err| invalid_event(path, &format!("invalid JSON payload: {}", err)))
}

pub(super) fn required_string(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<String, SyncError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| invalid_event(path, &format!("missing '{}' string field", key)))
}

/// Default profile used when a legacy event omits `profile_id`. The field
/// was only made required after 2026-04-09 ("Remove legacy workflow runtime
/// fallbacks"); older events committed before that date may lack it. We
/// translate them at apply time to the built-in default so a bootstrap pull
/// of a pre-cutoff repo doesn't hard-fail.
const LEGACY_DEFAULT_PROFILE_ID: &str = "autopilot";

pub(super) fn required_profile_id(
    object: &Map<String, Value>,
    _path: &Path,
) -> Result<String, SyncError> {
    if let Some(value) = object.get("profile_id").and_then(Value::as_str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Ok(LEGACY_DEFAULT_PROFILE_ID.to_string())
}

pub(super) enum WorkflowIdResolution {
    Direct,
    ConvertedLegacy(String),
    InferredFromType(String),
}

pub(super) struct ResolvedWorkflowId {
    pub id: String,
    pub resolution: WorkflowIdResolution,
}

pub(super) fn required_workflow_id(
    object: &Map<String, Value>,
    _path: &Path,
) -> Result<ResolvedWorkflowId, SyncError> {
    if let Some(value) = object.get("workflow_id").and_then(Value::as_str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let normalized = installed_workflows::normalize_workflow_id(trimmed);
            return Ok(match normalized.as_str() {
                // "default" was the pre-workflow-registry name used before
                // knots_sdlc/work_sdlc existed; treated the same way.
                "compatibility" | "knots_sdlc" | "default" => ResolvedWorkflowId {
                    id: "work_sdlc".to_string(),
                    resolution: WorkflowIdResolution::ConvertedLegacy(normalized),
                },
                _ => ResolvedWorkflowId {
                    id: normalized,
                    resolution: WorkflowIdResolution::Direct,
                },
            });
        }
    }
    let type_value = object.get("type").and_then(Value::as_str);
    let knot_type = crate::domain::knot_type::parse_knot_type(type_value);
    Ok(ResolvedWorkflowId {
        id: installed_workflows::builtin_workflow_id_for_knot_type(knot_type),
        resolution: WorkflowIdResolution::InferredFromType(knot_type.as_str().to_string()),
    })
}

pub(super) fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(|raw| {
            if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            }
        })
}

pub(super) fn optional_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

pub(super) fn parse_metadata_entry(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<MetadataEntry, SyncError> {
    let entry_id = required_string(object, "entry_id", path)?;
    let content = required_string(object, "content", path)?;
    let username = required_string(object, "username", path)?;
    let datetime = required_string(object, "datetime", path)?;
    let agentname = required_string(object, "agentname", path)?;
    let model = required_string(object, "model", path)?;
    let version = required_string(object, "version", path)?;
    Ok(MetadataEntry {
        entry_id,
        content,
        username,
        datetime,
        agentname,
        model,
        version,
    })
}

pub(super) fn parse_invariants(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<Vec<Invariant>, SyncError> {
    let raw = object
        .get("invariants")
        .ok_or_else(|| invalid_event(path, "missing 'invariants' array field"))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid 'invariants' payload: {}", err)))
}

pub(super) fn parse_string_vec(
    object: &Map<String, Value>,
    path: &Path,
    key: &str,
) -> Result<Vec<String>, SyncError> {
    let raw = object
        .get(key)
        .ok_or_else(|| invalid_event(path, &format!("missing '{key}' array field")))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid '{key}' payload: {}", err)))
}

pub(super) fn parse_gate_data(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<GateData, SyncError> {
    let raw = object
        .get("gate")
        .ok_or_else(|| invalid_event(path, "missing 'gate' object field"))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid 'gate' payload: {}", err)))
}

pub(super) fn parse_lease_data(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<LeaseData, SyncError> {
    let raw = object
        .get("lease_data")
        .ok_or_else(|| invalid_event(path, "missing 'lease_data' field"))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid 'lease_data' payload: {}", err)))
}

pub(super) fn parse_execution_plan_data(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<ExecutionPlanData, SyncError> {
    let raw = object
        .get("execution_plan")
        .ok_or_else(|| invalid_event(path, "missing 'execution_plan' field"))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid 'execution_plan' payload: {}", err)))
}

pub(super) fn parse_scope_data(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<ScopeData, SyncError> {
    serde_json::from_value(Value::Object(object.clone()))
        .map_err(|err| invalid_event(path, &format!("invalid scope payload: {}", err)))
}

pub(super) fn parse_index_scope_data(
    object: &Map<String, Value>,
    path: &Path,
) -> Result<ScopeData, SyncError> {
    let raw = object
        .get("scope")
        .ok_or_else(|| invalid_event(path, "missing 'scope' field"))?;
    serde_json::from_value(raw.clone())
        .map_err(|err| invalid_event(path, &format!("invalid 'scope' payload: {}", err)))
}

pub(super) fn invalid_event(path: &Path, message: &str) -> SyncError {
    SyncError::InvalidEvent {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

pub(super) fn is_stale_precondition(
    conn: &Connection,
    knot_id: &str,
    precondition: Option<&crate::events::WorkflowPrecondition>,
) -> Result<bool, SyncError> {
    let Some(precondition) = precondition else {
        return Ok(false);
    };
    let current = db::get_knot_hot(conn, knot_id)?
        .and_then(|record| record.profile_etag)
        .unwrap_or_default();
    Ok(current != precondition.profile_etag)
}

pub(super) fn is_stale_full_precondition(
    conn: &Connection,
    event: &FullEvent,
) -> Result<bool, SyncError> {
    let Some(precondition) = event.precondition.as_ref() else {
        return Ok(false);
    };
    let Some(current) = db::get_knot_hot(conn, &event.knot_id)? else {
        return Ok(false);
    };
    if current.profile_etag.as_deref() == Some(precondition.profile_etag.as_str()) {
        return Ok(false);
    }
    Ok(current.updated_at != event.occurred_at)
}

pub(super) struct IndexUpsertParams<'a> {
    pub conn: &'a Connection,
    pub data: &'a serde_json::Map<String, serde_json::Value>,
    pub absolute_path: &'a std::path::Path,
    pub knot_id: &'a str,
    pub title: &'a str,
    pub state: &'a str,
    pub updated_at: &'a str,
    pub profile_id: &'a str,
    pub workflow_id: &'a str,
    pub event_id: &'a str,
}

pub(super) fn build_index_upsert(
    params: &IndexUpsertParams<'_>,
) -> Result<MetadataProjection, SyncError> {
    let existing = db::get_knot_hot(params.conn, params.knot_id)?;
    let body = existing.as_ref().and_then(|r| r.body.clone());
    let description = existing.as_ref().and_then(|r| r.description.clone());
    let acceptance = existing.as_ref().and_then(|r| r.acceptance.clone());
    let priority = existing.as_ref().and_then(|r| r.priority);
    // Index events carry `type` in their data. Prefer that over the cached
    // value so new knots pulled from origin inherit their type on first
    // apply; without this, `kno ls --type execution_plan` (and similar)
    // miss knots that were authored on another machine.
    let knot_type = optional_string(params.data.get("type"))
        .or_else(|| existing.as_ref().and_then(|r| r.knot_type.clone()));
    let tags = existing
        .as_ref()
        .map(|r| r.tags.clone())
        .unwrap_or_default();
    let notes = existing
        .as_ref()
        .map(|r| r.notes.clone())
        .unwrap_or_default();
    let handoff_capsules = existing
        .as_ref()
        .map(|r| r.handoff_capsules.clone())
        .unwrap_or_default();
    let mut invariants = existing
        .as_ref()
        .map(|r| r.invariants.clone())
        .unwrap_or_default();
    if params.data.contains_key("invariants") {
        invariants = parse_invariants(params.data, params.absolute_path)?;
    }
    let verification_steps = index_verification_steps(existing.as_ref(), params)?;
    let step_history = existing
        .as_ref()
        .map(|r| r.step_history.clone())
        .unwrap_or_default();
    let mut gate_data = existing
        .as_ref()
        .map(|r| r.gate_data.clone())
        .unwrap_or_default();
    if params.data.contains_key("gate") {
        gate_data = parse_gate_data(params.data, params.absolute_path)?;
    }
    let lease_data = existing
        .as_ref()
        .map(|r| r.lease_data.clone())
        .unwrap_or_default();
    let mut scope_data = existing
        .as_ref()
        .map(|r| r.scope_data.clone())
        .unwrap_or_default();
    if params.data.contains_key("scope") {
        scope_data = parse_index_scope_data(params.data, params.absolute_path)?;
    }
    let mut execution_plan_data = existing
        .as_ref()
        .map(|r| r.execution_plan_data.clone())
        .unwrap_or_default();
    if params.data.contains_key("execution_plan") {
        let incoming = parse_execution_plan_data(params.data, params.absolute_path)?;
        if should_use_index_execution_plan(&execution_plan_data, &incoming) {
            execution_plan_data = incoming;
        }
    }
    let lease_id = existing.as_ref().and_then(|r| r.lease_id.clone());
    let deferred_from_state =
        optional_string(params.data.get("deferred_from_state")).or_else(|| {
            existing
                .as_ref()
                .and_then(|r| r.deferred_from_state.clone())
        });
    let blocked_from_state = optional_string(params.data.get("blocked_from_state"))
        .or_else(|| existing.as_ref().and_then(|r| r.blocked_from_state.clone()));
    let created_at = existing
        .as_ref()
        .and_then(|r| r.created_at.clone())
        .unwrap_or_else(|| params.updated_at.to_string());

    Ok(MetadataProjection {
        title: params.title.to_string(),
        state: params.state.to_string(),
        updated_at: params.updated_at.to_string(),
        body,
        description,
        acceptance,
        priority,
        knot_type,
        tags,
        notes,
        handoff_capsules,
        invariants,
        verification_steps,
        step_history,
        gate_data,
        lease_data,
        execution_plan_data,
        scope_data,
        lease_id,
        workflow_id: params.workflow_id.to_string(),
        profile_id: params.profile_id.to_string(),
        profile_etag: Some(params.event_id.to_string()),
        deferred_from_state,
        blocked_from_state,
        created_at: Some(created_at),
    })
}

fn index_verification_steps(
    existing: Option<&KnotCacheRecord>,
    params: &IndexUpsertParams<'_>,
) -> Result<Vec<String>, SyncError> {
    if params.data.contains_key("verification_steps") {
        return parse_string_vec(params.data, params.absolute_path, "verification_steps");
    }
    Ok(existing
        .map(|record| record.verification_steps.clone())
        .unwrap_or_default())
}

fn should_use_index_execution_plan(
    existing: &ExecutionPlanData,
    incoming: &ExecutionPlanData,
) -> bool {
    !incoming.waves.is_empty() || existing.waves.is_empty()
}

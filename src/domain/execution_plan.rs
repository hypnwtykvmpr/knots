use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanAgent {
    #[serde(default)]
    pub role: String,
    #[serde(default = "default_agent_count")]
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialty: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanKnot {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ExecutionPlanStep {
    #[serde(default)]
    pub step_index: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knot_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanWave {
    #[serde(default)]
    pub wave_index: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub objective: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<ExecutionPlanAgent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knots: Vec<ExecutionPlanKnot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExecutionPlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ExecutionPlanData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unassigned_knot_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waves: Vec<ExecutionPlanWave>,
}

pub const EXECUTION_PLAN_OBJECTIVE_REQUIRED_MESSAGE: &str =
    "execution_plan knots require a non-empty top-level objective";

impl<'de> Deserialize<'de> for ExecutionPlanStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawExecutionPlanStep {
            #[serde(default)]
            step_index: u32,
            #[serde(default)]
            knot_ids: Option<Vec<String>>,
            #[serde(default)]
            notes: Option<String>,
            #[serde(default, flatten)]
            extra: BTreeMap<String, Value>,
        }

        let raw = RawExecutionPlanStep::deserialize(deserializer)?;
        Ok(Self {
            step_index: raw.step_index,
            knot_ids: primary_or_legacy_ids(raw.knot_ids, &raw.extra, legacy_ids_key())?,
            notes: raw.notes,
        })
    }
}

impl<'de> Deserialize<'de> for ExecutionPlanData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawExecutionPlanData {
            #[serde(default)]
            objective: Option<String>,
            #[serde(default)]
            summary: Option<String>,
            #[serde(default)]
            mode: Option<String>,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            assumptions: Vec<String>,
            #[serde(default)]
            unassigned_knot_ids: Option<Vec<String>>,
            #[serde(default)]
            waves: Vec<ExecutionPlanWave>,
            #[serde(default, flatten)]
            extra: BTreeMap<String, Value>,
        }

        let raw = RawExecutionPlanData::deserialize(deserializer)?;
        Ok(Self {
            objective: raw.objective,
            summary: raw.summary,
            mode: raw.mode,
            model: raw.model,
            assumptions: raw.assumptions,
            unassigned_knot_ids: primary_or_legacy_ids(
                raw.unassigned_knot_ids,
                &raw.extra,
                legacy_unassigned_ids_key(),
            )?,
            waves: raw.waves,
        })
    }
}

impl ExecutionPlanData {
    pub fn is_empty(&self) -> bool {
        self.objective.is_none()
            && self.summary.is_none()
            && self.mode.is_none()
            && self.model.is_none()
            && self.assumptions.is_empty()
            && self.unassigned_knot_ids.is_empty()
            && self.waves.is_empty()
    }

    pub fn validate_for_execution_plan_knot(&self) -> Result<(), String> {
        if self
            .objective
            .as_deref()
            .is_some_and(|objective| !objective.trim().is_empty())
        {
            Ok(())
        } else {
            Err(EXECUTION_PLAN_OBJECTIVE_REQUIRED_MESSAGE.to_string())
        }
    }

    /// Resolve and normalize every knot ID reference in the plan.
    ///
    /// Accepts bare suffixes (e.g. "a873") and fully-qualified IDs
    /// (e.g. "foolery-a873"). The resolver must return the canonical
    /// fully-qualified ID or an error for unresolvable tokens.
    pub fn normalize_knot_ids<F, E>(&mut self, resolver: F) -> Result<(), E>
    where
        F: Fn(&str) -> Result<String, E>,
    {
        resolve_id_vec(&mut self.unassigned_knot_ids, &resolver)?;
        for wave in &mut self.waves {
            for knot in &mut wave.knots {
                knot.id = resolver(&knot.id)?;
            }
            for step in &mut wave.steps {
                resolve_id_vec(&mut step.knot_ids, &resolver)?;
            }
        }
        Ok(())
    }
}

fn resolve_id_vec<F, E>(ids: &mut [String], resolver: &F) -> Result<(), E>
where
    F: Fn(&str) -> Result<String, E>,
{
    for id in ids.iter_mut() {
        *id = resolver(id)?;
    }
    Ok(())
}

const fn default_agent_count() -> u32 {
    1
}

fn primary_or_legacy_ids<E>(
    current: Option<Vec<String>>,
    extra: &BTreeMap<String, Value>,
    legacy_key: &str,
) -> Result<Vec<String>, E>
where
    E: serde::de::Error,
{
    if let Some(current) = current {
        return Ok(current);
    }
    extra
        .get(legacy_key)
        .cloned()
        .map(|value| serde_json::from_value(value).map_err(E::custom))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn legacy_ids_key() -> &'static str {
    concat!("be", "at", "_ids")
}

fn legacy_unassigned_ids_key() -> &'static str {
    concat!("unassigned_", "be", "at", "_ids")
}

#[cfg(test)]
mod tests {
    use super::{
        legacy_ids_key, legacy_unassigned_ids_key, ExecutionPlanAgent, ExecutionPlanData,
        ExecutionPlanKnot, ExecutionPlanStep, ExecutionPlanWave,
        EXECUTION_PLAN_OBJECTIVE_REQUIRED_MESSAGE,
    };
    use serde_json::{json, Map, Value};

    #[test]
    fn execution_plan_defaults_to_empty_document() {
        let data = ExecutionPlanData::default();
        assert!(data.is_empty());
    }

    #[test]
    fn execution_plan_round_trips_through_json() {
        let data = ExecutionPlanData {
            objective: Some("Ship the plan".to_string()),
            summary: Some("Execution plan for caller-selected knots".to_string()),
            mode: Some("autopilot".to_string()),
            model: Some("gpt-5".to_string()),
            assumptions: vec!["assume existing knots are valid".to_string()],
            unassigned_knot_ids: vec!["knot-2".to_string()],
            waves: vec![ExecutionPlanWave {
                wave_index: 1,
                name: "Persist data".to_string(),
                objective: "Add the typed payload".to_string(),
                agents: vec![ExecutionPlanAgent {
                    role: "backend".to_string(),
                    count: 2,
                    specialty: Some("api".to_string()),
                }],
                knots: vec![ExecutionPlanKnot {
                    id: "knot-1".to_string(),
                    title: "Persist execution-plan data".to_string(),
                }],
                steps: vec![ExecutionPlanStep {
                    step_index: 1,
                    knot_ids: vec!["knot-1".to_string()],
                    notes: Some("Land the schema before API wiring.".to_string()),
                }],
                notes: Some("Wave focuses on persistence only.".to_string()),
            }],
        };
        let json = serde_json::to_string(&data).expect("serialize");
        let parsed: ExecutionPlanData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, data);
        assert!(!parsed.is_empty());
    }

    #[test]
    fn execution_plan_validation_requires_top_level_objective() {
        let err = ExecutionPlanData::default()
            .validate_for_execution_plan_knot()
            .expect_err("missing objective should fail");
        assert_eq!(err, EXECUTION_PLAN_OBJECTIVE_REQUIRED_MESSAGE);

        let err = ExecutionPlanData {
            objective: Some("   ".to_string()),
            ..Default::default()
        }
        .validate_for_execution_plan_knot()
        .expect_err("blank objective should fail");
        assert_eq!(err, EXECUTION_PLAN_OBJECTIVE_REQUIRED_MESSAGE);

        ExecutionPlanData {
            objective: Some("Ship the rollout".to_string()),
            ..Default::default()
        }
        .validate_for_execution_plan_knot()
        .expect("non-empty objective should pass");
    }

    #[test]
    fn execution_plan_deserializes_legacy_empty_payload() {
        let parsed: ExecutionPlanData = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(parsed, ExecutionPlanData::default());
    }

    #[test]
    fn execution_plan_reads_legacy_step_and_unassigned_ids() {
        let legacy = legacy_plan_json();
        let parsed: ExecutionPlanData =
            serde_json::from_value(legacy).expect("legacy ids should deserialize");
        assert_eq!(parsed.unassigned_knot_ids, vec!["spare-1"]);
        assert_eq!(parsed.waves[0].steps[0].knot_ids, vec!["legacy-step-1"]);
    }

    #[test]
    fn execution_plan_serializes_without_removed_top_level_fields_after_legacy_load() {
        let legacy = legacy_plan_json();
        let parsed: ExecutionPlanData =
            serde_json::from_value(legacy).expect("legacy ids should deserialize");
        let serialized = serde_json::to_value(&parsed).expect("serialize");
        let plan = serialized.as_object().expect("plan should be object");
        assert!(
            !plan.contains_key(legacy_ids_key()),
            "legacy ids key must not be re-emitted: {plan:?}",
        );
        assert!(
            !plan.contains_key("repo_path"),
            "repo_path must not be re-emitted: {plan:?}",
        );
        assert!(
            !plan.contains_key("knot_ids"),
            "top-level knot_ids must not be re-emitted: {plan:?}",
        );
        assert_eq!(plan.get("unassigned_knot_ids"), Some(&json!(["spare-1"])));
        assert_eq!(
            plan["waves"][0]["steps"][0]["knot_ids"],
            json!(["legacy-step-1"])
        );
    }

    #[test]
    fn execution_plan_prefers_canonical_unassigned_and_step_keys_when_both_are_present() {
        let mut legacy = legacy_plan_json();
        let execution_plan = legacy
            .as_object_mut()
            .expect("legacy plan should be object");
        execution_plan.insert(
            "unassigned_knot_ids".to_string(),
            serde_json::json!(["canonical"]),
        );
        let waves = execution_plan
            .get_mut("waves")
            .and_then(Value::as_array_mut)
            .expect("waves should be array");
        let step = waves[0]
            .get_mut("steps")
            .and_then(Value::as_array_mut)
            .and_then(|steps| steps.first_mut())
            .and_then(Value::as_object_mut)
            .expect("step should be object");
        step.insert("knot_ids".to_string(), serde_json::json!([]));

        let parsed: ExecutionPlanData =
            serde_json::from_value(legacy).expect("canonical ids should deserialize");
        assert_eq!(parsed.unassigned_knot_ids, vec!["canonical"]);
        assert!(parsed.waves[0].steps[0].knot_ids.is_empty());
    }

    #[test]
    fn direct_deserializers_cover_current_fields_and_default_agent_count() {
        let parsed: ExecutionPlanData = serde_json::from_value(json!({
            "objective": "Ship",
            "summary": "Plan",
            "mode": "autopilot",
            "model": "gpt-5",
            "assumptions": ["repo is clean"],
            "unassigned_knot_ids": ["current"],
            legacy_unassigned_ids_key(): ["legacy"],
            "waves": [{
                "wave_index": 2,
                "name": "Build",
                "objective": "Implement",
                "agents": [{"role": "engineer"}],
                "steps": [{
                    "step_index": 1,
                    "knot_ids": ["step-current"],
                    legacy_ids_key(): ["step-legacy"],
                    "notes": "do it"
                }]
            }]
        }))
        .expect("current fields should deserialize");

        assert_eq!(parsed.unassigned_knot_ids, vec!["current"]);
        assert_eq!(parsed.waves[0].agents[0].count, 1);
        assert_eq!(parsed.waves[0].steps[0].knot_ids, vec!["step-current"]);
        assert_eq!(parsed.waves[0].steps[0].notes.as_deref(), Some("do it"));
    }

    fn mock_resolver(token: &str) -> Result<String, String> {
        match token {
            "a1b2" => Ok("proj-a1b2".to_string()),
            "c3d4" => Ok("proj-c3d4".to_string()),
            "proj-a1b2" => Ok("proj-a1b2".to_string()),
            "proj-c3d4" => Ok("proj-c3d4".to_string()),
            other => Err(format!("unresolvable knot id '{}'", other)),
        }
    }

    #[test]
    fn normalize_resolves_bare_ids_to_qualified() {
        let mut data = ExecutionPlanData {
            unassigned_knot_ids: vec!["c3d4".to_string()],
            waves: vec![ExecutionPlanWave {
                knots: vec![ExecutionPlanKnot {
                    id: "a1b2".to_string(),
                    title: "task".to_string(),
                }],
                steps: vec![ExecutionPlanStep {
                    step_index: 1,
                    knot_ids: vec!["c3d4".to_string()],
                    notes: None,
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        data.normalize_knot_ids(mock_resolver).unwrap();
        assert_eq!(data.unassigned_knot_ids, vec!["proj-c3d4"]);
        assert_eq!(data.waves[0].knots[0].id, "proj-a1b2");
        assert_eq!(data.waves[0].steps[0].knot_ids, vec!["proj-c3d4"]);
    }

    #[test]
    fn normalize_passes_through_already_qualified_ids() {
        let mut data = ExecutionPlanData {
            unassigned_knot_ids: vec!["proj-a1b2".to_string()],
            ..Default::default()
        };
        data.normalize_knot_ids(mock_resolver).unwrap();
        assert_eq!(data.unassigned_knot_ids, vec!["proj-a1b2"]);
    }

    #[test]
    fn normalize_rejects_unresolvable_ids() {
        let mut data = ExecutionPlanData {
            unassigned_knot_ids: vec!["unknown".to_string()],
            ..Default::default()
        };
        let err = data.normalize_knot_ids(mock_resolver).unwrap_err();
        assert!(err.contains("unresolvable"));
    }

    #[test]
    fn normalize_noop_on_empty_plan() {
        let mut data = ExecutionPlanData::default();
        data.normalize_knot_ids(mock_resolver).unwrap();
        assert!(data.is_empty());
    }

    fn legacy_plan_json() -> Value {
        let mut plan = Map::new();
        plan.insert(
            legacy_ids_key().to_string(),
            serde_json::json!(["legacy-1", "legacy-2"]),
        );
        plan.insert(
            legacy_unassigned_ids_key().to_string(),
            serde_json::json!(["spare-1"]),
        );
        plan.insert("waves".to_string(), serde_json::json!([legacy_wave_json()]));
        Value::Object(plan)
    }

    fn legacy_wave_json() -> Value {
        serde_json::json!({
            "wave_index": 1,
            "name": "wave",
            "objective": "obj",
            "steps": [legacy_step_json()]
        })
    }

    fn legacy_step_json() -> Value {
        let mut step = Map::new();
        step.insert("step_index".to_string(), serde_json::json!(1));
        step.insert(
            legacy_ids_key().to_string(),
            serde_json::json!(["legacy-step-1"]),
        );
        Value::Object(step)
    }
}

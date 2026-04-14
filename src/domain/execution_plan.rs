use serde::{Deserialize, Serialize};

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
pub struct ExecutionPlanBeat {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanStep {
    #[serde(default)]
    pub step_index: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beat_ids: Vec<String>,
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
    pub beats: Vec<ExecutionPlanBeat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExecutionPlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
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
    pub beat_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unassigned_beat_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waves: Vec<ExecutionPlanWave>,
}

impl ExecutionPlanData {
    pub fn is_empty(&self) -> bool {
        self.repo_path.is_none()
            && self.objective.is_none()
            && self.summary.is_none()
            && self.mode.is_none()
            && self.model.is_none()
            && self.assumptions.is_empty()
            && self.beat_ids.is_empty()
            && self.unassigned_beat_ids.is_empty()
            && self.waves.is_empty()
    }
}

const fn default_agent_count() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionPlanAgent, ExecutionPlanBeat, ExecutionPlanData, ExecutionPlanStep,
        ExecutionPlanWave,
    };

    #[test]
    fn execution_plan_defaults_to_empty_document() {
        let data = ExecutionPlanData::default();
        assert!(data.is_empty());
    }

    #[test]
    fn execution_plan_round_trips_through_json() {
        let data = ExecutionPlanData {
            repo_path: Some("/repo".to_string()),
            objective: Some("Ship the plan".to_string()),
            summary: Some("Execution plan for caller-selected beats".to_string()),
            mode: Some("autopilot".to_string()),
            model: Some("gpt-5".to_string()),
            assumptions: vec!["assume existing beats are valid".to_string()],
            beat_ids: vec!["beat-1".to_string()],
            unassigned_beat_ids: vec!["beat-2".to_string()],
            waves: vec![ExecutionPlanWave {
                wave_index: 1,
                name: "Persist data".to_string(),
                objective: "Add the typed payload".to_string(),
                agents: vec![ExecutionPlanAgent {
                    role: "backend".to_string(),
                    count: 2,
                    specialty: Some("api".to_string()),
                }],
                beats: vec![ExecutionPlanBeat {
                    id: "beat-1".to_string(),
                    title: "Persist execution-plan data".to_string(),
                }],
                steps: vec![ExecutionPlanStep {
                    step_index: 1,
                    beat_ids: vec!["beat-1".to_string()],
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
    fn execution_plan_deserializes_legacy_empty_payload() {
        let parsed: ExecutionPlanData = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(parsed, ExecutionPlanData::default());
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlanStatus {
    #[default]
    Draft,
    Active,
    Complete,
    Aborted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlanStepStatus {
    #[default]
    Pending,
    InProgress,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanStep {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: ExecutionPlanStepStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beat_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by_step_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanWave {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beat_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by_wave_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExecutionPlanStep>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlanData {
    #[serde(default)]
    pub status: ExecutionPlanStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
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
        self.status == ExecutionPlanStatus::Draft
            && self.repo_path.is_none()
            && self.objective.is_none()
            && self.mode.is_none()
            && self.model.is_none()
            && self.assumptions.is_empty()
            && self.beat_ids.is_empty()
            && self.unassigned_beat_ids.is_empty()
            && self.waves.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionPlanData, ExecutionPlanStatus, ExecutionPlanStep, ExecutionPlanStepStatus,
        ExecutionPlanWave,
    };

    #[test]
    fn execution_plan_defaults_to_empty_draft() {
        let data = ExecutionPlanData::default();
        assert_eq!(data.status, ExecutionPlanStatus::Draft);
        assert!(data.is_empty());
    }

    #[test]
    fn execution_plan_round_trips_through_json() {
        let data = ExecutionPlanData {
            status: ExecutionPlanStatus::Active,
            repo_path: Some("/repo".to_string()),
            objective: Some("Ship the plan".to_string()),
            mode: Some("autopilot".to_string()),
            model: Some("gpt-5".to_string()),
            assumptions: vec!["assume existing beats are valid".to_string()],
            beat_ids: vec!["beat-1".to_string()],
            unassigned_beat_ids: vec!["beat-2".to_string()],
            waves: vec![ExecutionPlanWave {
                id: "wave-1".to_string(),
                title: "Persist data".to_string(),
                summary: "Add the typed payload".to_string(),
                beat_ids: vec!["beat-1".to_string()],
                blocked_by_wave_ids: vec!["wave-0".to_string()],
                steps: vec![ExecutionPlanStep {
                    id: "step-1".to_string(),
                    title: "Land DB schema".to_string(),
                    summary: "Add the new column".to_string(),
                    status: ExecutionPlanStepStatus::InProgress,
                    beat_ids: vec!["beat-1".to_string()],
                    blocked_by_step_ids: vec!["step-0".to_string()],
                    assignee: Some("codex".to_string()),
                }],
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

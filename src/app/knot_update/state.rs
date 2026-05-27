use crate::db::KnotCacheRecord;
use crate::domain::knot_type::parse_knot_type;

pub(super) struct UpdateState {
    pub title: String,
    pub state: String,
    pub description: Option<String>,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub priority: Option<i64>,
    pub knot_type: crate::domain::knot_type::KnotType,
    pub deferred: Option<String>,
    pub blocked: Option<String>,
    pub tags: Vec<String>,
    pub notes: Vec<crate::domain::metadata::MetadataEntry>,
    pub handoff_capsules: Vec<crate::domain::metadata::MetadataEntry>,
    pub invariants: Vec<crate::domain::invariant::Invariant>,
    pub verification_steps: Vec<String>,
    pub gate_data: crate::domain::gate::GateData,
    pub execution_plan_data: crate::domain::execution_plan::ExecutionPlanData,
    pub current_precondition: Option<String>,
}

impl UpdateState {
    pub fn from_record(record: &KnotCacheRecord, precondition: Option<String>) -> Self {
        Self {
            title: record.title.clone(),
            state: record.state.clone(),
            description: record.description.clone(),
            body: record.body.clone(),
            acceptance: record.acceptance.clone(),
            priority: record.priority,
            knot_type: parse_knot_type(record.knot_type.as_deref()),
            deferred: record.deferred_from_state.clone(),
            blocked: record.blocked_from_state.clone(),
            tags: record.tags.clone(),
            notes: record.notes.clone(),
            handoff_capsules: record.handoff_capsules.clone(),
            invariants: record.invariants.clone(),
            verification_steps: record.verification_steps.clone(),
            gate_data: record.gate_data.clone(),
            execution_plan_data: record.execution_plan_data.clone(),
            current_precondition: precondition,
        }
    }

    pub fn refresh_from_record(&mut self, record: &KnotCacheRecord) {
        self.title = record.title.clone();
        self.state = record.state.clone();
        self.description = record.description.clone();
        self.body = record.body.clone();
        self.acceptance = record.acceptance.clone();
        self.priority = record.priority;
        self.knot_type = parse_knot_type(record.knot_type.as_deref());
        self.deferred = record.deferred_from_state.clone();
        self.blocked = record.blocked_from_state.clone();
        self.tags = record.tags.clone();
        self.notes = record.notes.clone();
        self.handoff_capsules = record.handoff_capsules.clone();
        self.invariants = record.invariants.clone();
        self.verification_steps = record.verification_steps.clone();
        self.gate_data = record.gate_data.clone();
        self.execution_plan_data = record.execution_plan_data.clone();
    }
}

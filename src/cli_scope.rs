use clap::Args;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Args, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeArgs {
    #[arg(
        long = "scope-volume",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Scope volume estimate (positive integer)."
    )]
    pub scope_volume: Option<String>,

    #[arg(
        long = "scope-scale",
        help_heading = "Scope metadata",
        help = "Scope scale identifier, such as fib_v1."
    )]
    pub scope_scale: Option<String>,

    #[arg(
        long = "scope-volume-score-confidence",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Volume score confidence (0.0 to 1.0)."
    )]
    pub scope_volume_score_confidence: Option<String>,

    #[arg(
        long = "scope-volume-stddev",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Volume stddev (non-negative float)."
    )]
    pub scope_volume_stddev: Option<String>,

    #[arg(
        long = "scope-volume-result-id",
        help_heading = "Scope metadata",
        help = "Volume quorum result id."
    )]
    pub scope_volume_result_id: Option<String>,

    #[arg(
        long = "scope-reliability",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Reliability score (0-100 integer)."
    )]
    pub scope_reliability: Option<String>,

    #[arg(
        long = "scope-reliability-score-confidence",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Reliability score confidence (0.0 to 1.0)."
    )]
    pub scope_reliability_score_confidence: Option<String>,

    #[arg(
        long = "scope-reliability-stddev",
        help_heading = "Scope metadata",
        allow_hyphen_values = true,
        help = "Reliability stddev (non-negative float)."
    )]
    pub scope_reliability_stddev: Option<String>,

    #[arg(
        long = "scope-reliability-band",
        help_heading = "Scope metadata",
        help = "Reliability band label."
    )]
    pub scope_reliability_band: Option<String>,

    #[arg(
        long = "scope-reliability-result-id",
        help_heading = "Scope metadata",
        help = "Reliability quorum result id."
    )]
    pub scope_reliability_result_id: Option<String>,
}

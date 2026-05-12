use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, Default, PartialOrd)]
pub struct ScopeFloat(f64);

impl ScopeFloat {
    pub fn new(value: f64) -> Result<Self, String> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err("scope float values must be finite".to_string())
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for ScopeFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for ScopeFloat {}

impl fmt::Display for ScopeFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ScopeFloat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for ScopeFloat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        ScopeFloat::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_score_confidence: Option<ScopeFloat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_stddev: Option<ScopeFloat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_result_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_score_confidence: Option<ScopeFloat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_stddev: Option<ScopeFloat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_result_id: Option<String>,
}

impl ScopeData {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopePatch {
    pub volume: Option<u64>,
    pub scale: Option<String>,
    pub volume_score_confidence: Option<ScopeFloat>,
    pub volume_stddev: Option<ScopeFloat>,
    pub volume_result_id: Option<String>,
    pub reliability: Option<u8>,
    pub reliability_score_confidence: Option<ScopeFloat>,
    pub reliability_stddev: Option<ScopeFloat>,
    pub reliability_band: Option<String>,
    pub reliability_result_id: Option<String>,
}

impl ScopePatch {
    pub fn has_changes(&self) -> bool {
        self.volume.is_some()
            || self.scale.is_some()
            || self.volume_score_confidence.is_some()
            || self.volume_stddev.is_some()
            || self.volume_result_id.is_some()
            || self.reliability.is_some()
            || self.reliability_score_confidence.is_some()
            || self.reliability_stddev.is_some()
            || self.reliability_band.is_some()
            || self.reliability_result_id.is_some()
    }

    pub fn apply_to(&self, mut data: ScopeData) -> ScopeData {
        if let Some(value) = self.volume {
            data.volume = Some(value);
        }
        if let Some(value) = self.scale.clone() {
            data.scale = Some(value);
        }
        if let Some(value) = self.volume_score_confidence {
            data.volume_score_confidence = Some(value);
        }
        if let Some(value) = self.volume_stddev {
            data.volume_stddev = Some(value);
        }
        if let Some(value) = self.volume_result_id.clone() {
            data.volume_result_id = Some(value);
        }
        if let Some(value) = self.reliability {
            data.reliability = Some(value);
        }
        if let Some(value) = self.reliability_score_confidence {
            data.reliability_score_confidence = Some(value);
        }
        if let Some(value) = self.reliability_stddev {
            data.reliability_stddev = Some(value);
        }
        if let Some(value) = self.reliability_band.clone() {
            data.reliability_band = Some(value);
        }
        if let Some(value) = self.reliability_result_id.clone() {
            data.reliability_result_id = Some(value);
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::{ScopeData, ScopeFloat, ScopePatch};

    #[test]
    fn scope_float_formats_and_round_trips_json() {
        let value = ScopeFloat::new(0.5).expect("finite value should parse");

        assert_eq!(value.to_string(), "0.5");
        assert_eq!(serde_json::to_string(&value).expect("json"), "0.5");
        let decoded: ScopeFloat = serde_json::from_str("0.5").expect("json should decode");
        assert_eq!(decoded, value);
        assert!(ScopeFloat::new(f64::NAN).is_err());
    }

    #[test]
    fn scope_patch_detects_and_applies_every_field() {
        assert!(ScopeData::default().is_empty());
        assert!(!ScopePatch::default().has_changes());

        let patch = ScopePatch {
            volume: Some(8),
            scale: Some("fib_v1".to_string()),
            volume_score_confidence: Some(ScopeFloat::new(0.72).expect("finite")),
            volume_stddev: Some(ScopeFloat::new(1.25).expect("finite")),
            volume_result_id: Some("vol-1".to_string()),
            reliability: Some(44),
            reliability_score_confidence: Some(ScopeFloat::new(0.91).expect("finite")),
            reliability_stddev: Some(ScopeFloat::new(2.5).expect("finite")),
            reliability_band: Some("medium".to_string()),
            reliability_result_id: Some("rel-1".to_string()),
        };

        assert!(patch.has_changes());
        let data = patch.apply_to(ScopeData::default());
        assert!(!data.is_empty());
        assert_eq!(data.volume, Some(8));
        assert_eq!(data.scale.as_deref(), Some("fib_v1"));
        assert_eq!(data.volume_score_confidence.unwrap().get(), 0.72);
        assert_eq!(data.volume_stddev.unwrap().get(), 1.25);
        assert_eq!(data.volume_result_id.as_deref(), Some("vol-1"));
        assert_eq!(data.reliability, Some(44));
        assert_eq!(data.reliability_score_confidence.unwrap().get(), 0.91);
        assert_eq!(data.reliability_stddev.unwrap().get(), 2.5);
        assert_eq!(data.reliability_band.as_deref(), Some("medium"));
        assert_eq!(data.reliability_result_id.as_deref(), Some("rel-1"));
    }
}

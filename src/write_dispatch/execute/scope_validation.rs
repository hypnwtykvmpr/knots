use crate::app::AppError;
use crate::cli_scope::ScopeArgs;
use crate::domain::scope::{ScopeFloat, ScopePatch};

pub(super) fn parse_scope_patch(args: &ScopeArgs) -> Result<ScopePatch, AppError> {
    Ok(ScopePatch {
        volume: parse_volume(args.scope_volume.as_deref())?,
        scale: clean_string(args.scope_scale.as_deref()),
        volume_score_confidence: parse_confidence(
            "scope_volume_score_confidence",
            args.scope_volume_score_confidence.as_deref(),
        )?,
        volume_stddev: parse_stddev("scope_volume_stddev", args.scope_volume_stddev.as_deref())?,
        volume_result_id: clean_string(args.scope_volume_result_id.as_deref()),
        reliability: parse_reliability(args.scope_reliability.as_deref())?,
        reliability_score_confidence: parse_confidence(
            "scope_reliability_score_confidence",
            args.scope_reliability_score_confidence.as_deref(),
        )?,
        reliability_stddev: parse_stddev(
            "scope_reliability_stddev",
            args.scope_reliability_stddev.as_deref(),
        )?,
        reliability_band: clean_string(args.scope_reliability_band.as_deref()),
        reliability_result_id: clean_string(args.scope_reliability_result_id.as_deref()),
    })
}

fn parse_volume(raw: Option<&str>) -> Result<Option<u64>, AppError> {
    let Some(raw) = clean(raw) else {
        return Ok(None);
    };
    let value = raw.parse::<i64>().map_err(|_| {
        AppError::InvalidArgument("scope_volume must be a positive integer".to_string())
    })?;
    if value <= 0 {
        return Err(AppError::InvalidArgument(
            "scope_volume must be > 0".to_string(),
        ));
    }
    Ok(Some(value as u64))
}

fn parse_reliability(raw: Option<&str>) -> Result<Option<u8>, AppError> {
    let Some(raw) = clean(raw) else {
        return Ok(None);
    };
    let value = raw.parse::<i64>().map_err(|_| {
        AppError::InvalidArgument("scope_reliability must be an integer".to_string())
    })?;
    if !(0..=100).contains(&value) {
        return Err(AppError::InvalidArgument(
            "scope_reliability must be between 0 and 100".to_string(),
        ));
    }
    Ok(Some(value as u8))
}

fn parse_confidence(field: &str, raw: Option<&str>) -> Result<Option<ScopeFloat>, AppError> {
    let Some(value) = parse_float(field, raw)? else {
        return Ok(None);
    };
    if !(0.0..=1.0).contains(&value.get()) {
        return Err(AppError::InvalidArgument(format!(
            "{field} must be between 0.0 and 1.0"
        )));
    }
    Ok(Some(value))
}

fn parse_stddev(field: &str, raw: Option<&str>) -> Result<Option<ScopeFloat>, AppError> {
    let Some(value) = parse_float(field, raw)? else {
        return Ok(None);
    };
    if value.get() < 0.0 {
        return Err(AppError::InvalidArgument(format!("{field} must be >= 0.0")));
    }
    Ok(Some(value))
}

fn parse_float(field: &str, raw: Option<&str>) -> Result<Option<ScopeFloat>, AppError> {
    let Some(raw) = clean(raw) else {
        return Ok(None);
    };
    let value = raw
        .parse::<f64>()
        .map_err(|_| AppError::InvalidArgument(format!("{field} must be a float")))?;
    let value = ScopeFloat::new(value).map_err(AppError::InvalidArgument)?;
    Ok(Some(value))
}

fn clean(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

fn clean_string(raw: Option<&str>) -> Option<String> {
    clean(raw).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::parse_scope_patch;
    use crate::cli_scope::ScopeArgs;

    #[test]
    fn parses_valid_scope_patch() {
        let patch = parse_scope_patch(&ScopeArgs {
            scope_volume: Some("5".to_string()),
            scope_scale: Some("fib_v1".to_string()),
            scope_volume_score_confidence: Some("0.72".to_string()),
            scope_volume_stddev: Some("1.25".to_string()),
            scope_volume_result_id: Some("vol-1".to_string()),
            scope_reliability: Some("62".to_string()),
            scope_reliability_score_confidence: Some("1.0".to_string()),
            scope_reliability_stddev: Some("0".to_string()),
            scope_reliability_band: Some("high".to_string()),
            scope_reliability_result_id: Some("rel-1".to_string()),
        })
        .expect("valid patch should parse");
        assert_eq!(patch.volume, Some(5));
        assert_eq!(patch.reliability, Some(62));
        assert_eq!(patch.volume_score_confidence.unwrap().get(), 0.72);
    }

    #[test]
    fn rejects_invalid_boundaries() {
        assert_error("scope_volume", |args| {
            args.scope_volume = Some("0".to_string())
        });
        assert_error("positive integer", |args| {
            args.scope_volume = Some("abc".to_string())
        });
        assert_error("scope_reliability", |args| {
            args.scope_reliability = Some("101".to_string())
        });
        assert_error("integer", |args| {
            args.scope_reliability = Some("abc".to_string())
        });
        assert_error("scope_volume_score_confidence", |args| {
            args.scope_volume_score_confidence = Some("1.1".to_string())
        });
        assert_error("float", |args| {
            args.scope_volume_score_confidence = Some("abc".to_string())
        });
        assert_error("scope_reliability_stddev", |args| {
            args.scope_reliability_stddev = Some("-0.1".to_string())
        });
        assert_error("finite", |args| {
            args.scope_reliability_stddev = Some("inf".to_string())
        });
    }

    fn assert_error(field: &str, mutate: impl FnOnce(&mut ScopeArgs)) {
        let mut args = ScopeArgs::default();
        mutate(&mut args);
        let err = parse_scope_patch(&args).expect_err("invalid scope should fail");
        assert!(err.to_string().contains(field), "{err}");
    }
}

use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::domain::state::normalize_state_input;

/// Well-known terminal states that the tier classifier must treat as archival
/// regardless of which profile a knot belongs to. These are the terminal
/// markers emitted by every built-in workflow today; per-profile
/// terminal-state questions go through `ProfileDefinition::is_terminal_state`.
pub(crate) const TERMINAL_STATES: &[&str] = &["shipped", "abandoned", "lease_terminated"];

/// Terminal knots are held in hot tier for this many hours after the most recent
/// `updated_at` before they are eligible to be swept to cold storage. This gives
/// users a grace window to see recently-terminated knots via `kno ls`.
pub const ARCHIVE_AGE_HOURS: i64 = 72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheTier {
    Hot,
    Warm,
    Cold,
}

pub fn classify_knot_head_tier(
    state: &str,
    updated_at: &str,
    terminal_flag: bool,
    _hot_window_days: i64,
    now: OffsetDateTime,
) -> CacheTier {
    let normalized = normalize_state_input(state);
    let is_terminal = terminal_flag || TERMINAL_STATES.iter().any(|s| *s == normalized);

    if !is_terminal {
        return CacheTier::Hot;
    }

    let Some(updated) = OffsetDateTime::parse(updated_at, &Rfc3339).ok() else {
        return CacheTier::Warm;
    };
    let archive_cutoff = now - Duration::hours(ARCHIVE_AGE_HOURS);
    if updated < archive_cutoff {
        return CacheTier::Cold;
    }
    // Recently terminated: keep in hot for the grace window.
    CacheTier::Hot
}

#[cfg(test)]
mod tests {
    use super::{classify_knot_head_tier, CacheTier, ARCHIVE_AGE_HOURS};
    use time::format_description::well_known::Rfc3339;
    use time::{Duration, OffsetDateTime};

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-02-24T12:00:00Z", &Rfc3339).expect("now should parse")
    }

    fn fmt(ts: OffsetDateTime) -> String {
        ts.format(&Rfc3339).expect("format")
    }

    #[test]
    fn terminal_state_recent_is_hot() {
        // age < 72h → hot
        let recent = fmt(now() - Duration::hours(10));
        assert_eq!(
            classify_knot_head_tier("shipped", &recent, false, 7, now()),
            CacheTier::Hot
        );
    }

    #[test]
    fn terminal_flag_recent_is_hot() {
        let recent = fmt(now() - Duration::hours(2));
        assert_eq!(
            super::classify_knot_head_tier("done", &recent, true, 7, now()),
            CacheTier::Hot
        );
    }

    #[test]
    fn terminal_flag_stale_is_cold() {
        let stale = fmt(now() - Duration::hours(ARCHIVE_AGE_HOURS + 4));
        assert_eq!(
            super::classify_knot_head_tier("done", &stale, true, 7, now()),
            CacheTier::Cold
        );
    }

    #[test]
    fn terminal_state_below_boundary_is_hot() {
        // age == 71h → hot (below 72h boundary)
        let recent = fmt(now() - Duration::hours(71));
        assert_eq!(
            classify_knot_head_tier("abandoned", &recent, false, 7, now()),
            CacheTier::Hot
        );
    }

    #[test]
    fn terminal_state_stale_is_cold() {
        // age > 72h → cold
        let stale = fmt(now() - Duration::hours(ARCHIVE_AGE_HOURS + 1));
        assert_eq!(
            classify_knot_head_tier("shipped", &stale, false, 7, now()),
            CacheTier::Cold
        );
    }

    #[test]
    fn terminal_abandoned_stale_is_cold() {
        let stale = fmt(now() - Duration::hours(200));
        assert_eq!(
            classify_knot_head_tier("abandoned", &stale, false, 7, now()),
            CacheTier::Cold
        );
    }

    #[test]
    fn deferred_is_not_terminal_for_tiering() {
        // deferred is passive, not terminal, so it remains visible regardless of age.
        let recent = fmt(now() - Duration::days(400));
        assert_eq!(
            classify_knot_head_tier("deferred", &recent, false, 7, now()),
            CacheTier::Hot
        );
    }

    #[test]
    fn recent_non_terminal_is_hot() {
        let recent = fmt(now() - Duration::hours(25));
        assert_eq!(
            classify_knot_head_tier("implementing", &recent, false, 7, now()),
            CacheTier::Hot
        );
    }

    #[test]
    fn old_non_terminal_is_hot() {
        let tier = classify_knot_head_tier("work_item", "2025-12-01T00:00:00Z", false, 7, now());
        assert_eq!(tier, CacheTier::Hot);
    }

    #[test]
    fn unparseable_date_non_terminal_stays_hot() {
        let tier = classify_knot_head_tier("implementing", "not-a-date", false, 7, now());
        assert_eq!(tier, CacheTier::Hot);
    }

    #[test]
    fn unparseable_date_terminal_falls_back_to_warm() {
        let tier = classify_knot_head_tier("shipped", "not-a-date", false, 7, now());
        assert_eq!(tier, CacheTier::Warm);
    }
}

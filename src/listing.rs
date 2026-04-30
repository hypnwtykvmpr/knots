use crate::app::KnotView;
use crate::domain::knot_type::KnotType;
use crate::workflow::normalize_profile_id;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnotListFilter {
    pub include_all: bool,
    pub state: Option<String>,
    pub knot_type: Option<String>,
    pub profile_id: Option<String>,
    pub tags: Vec<String>,
    pub query: Option<String>,
}

pub fn apply_filters(knots: Vec<KnotView>, filter: &KnotListFilter) -> Vec<KnotView> {
    let normalized = NormalizedFilter::from(filter);
    if normalized.has_no_user_filters() && normalized.include_all {
        return knots;
    }

    knots
        .into_iter()
        .filter(|knot| matches_filter(knot, &normalized))
        .collect()
}

/// Filter `knots` against `filter`, then return the page starting at `offset`
/// limited by `limit` along with the total filtered count. The total reflects
/// the size of the filtered result set, so callers can advertise accurate
/// pagination metadata even when filters eliminate every match on a page.
pub fn filter_and_paginate(
    knots: Vec<KnotView>,
    filter: &KnotListFilter,
    offset: usize,
    limit: usize,
) -> (Vec<KnotView>, i64) {
    let filtered = apply_filters(knots, filter);
    let total = filtered.len() as i64;
    let page: Vec<KnotView> = filtered.into_iter().skip(offset).take(limit).collect();
    (page, total)
}

#[derive(Debug, Clone, Default)]
struct NormalizedFilter {
    include_all: bool,
    state: Option<String>,
    knot_type: Option<String>,
    profile_id: Option<String>,
    tags: Vec<String>,
    query: Option<String>,
}

impl NormalizedFilter {
    fn has_no_user_filters(&self) -> bool {
        self.state.is_none()
            && self.knot_type.is_none()
            && self.profile_id.is_none()
            && self.tags.is_empty()
            && self.query.is_none()
    }
}

impl From<&KnotListFilter> for NormalizedFilter {
    fn from(value: &KnotListFilter) -> Self {
        Self {
            include_all: value.include_all,
            state: normalize_scalar(value.state.as_deref()),
            knot_type: normalize_knot_type_filter(value.knot_type.as_deref()),
            profile_id: value.profile_id.as_deref().and_then(normalize_profile_id),
            tags: value
                .tags
                .iter()
                .filter_map(|tag| normalize_scalar(Some(tag)))
                .collect(),
            query: normalize_scalar(value.query.as_deref()),
        }
    }
}

fn matches_filter(knot: &KnotView, filter: &NormalizedFilter) -> bool {
    if should_hide_lease(knot, filter) {
        return false;
    }
    if should_hide_terminal(knot, filter) {
        return false;
    }

    if let Some(expected_state) = filter.state.as_deref() {
        let actual_state = knot.state.to_ascii_lowercase();
        if actual_state != expected_state {
            return false;
        }
    }

    if let Some(expected_type) = filter.knot_type.as_deref() {
        if knot.knot_type.as_str() != expected_type {
            return false;
        }
    }

    if let Some(expected_workflow) = filter.profile_id.as_deref() {
        if knot.profile_id.to_ascii_lowercase() != expected_workflow {
            return false;
        }
    }

    if !has_all_tags(knot, &filter.tags) {
        return false;
    }

    if let Some(query) = filter.query.as_deref() {
        return matches_query(knot, query);
    }

    true
}

fn should_hide_lease(knot: &KnotView, filter: &NormalizedFilter) -> bool {
    if knot.knot_type != KnotType::Lease {
        return false;
    }
    // Show if user explicitly requested lease type
    if let Some(ref ft) = filter.knot_type {
        return ft != "lease";
    }
    // Hide by default
    true
}

fn should_hide_terminal(knot: &KnotView, filter: &NormalizedFilter) -> bool {
    if filter.include_all {
        return false;
    }
    let state_lower = knot.state.trim().to_ascii_lowercase();
    let is_terminal = matches!(state_lower.as_str(), "shipped" | "abandoned");
    if !is_terminal {
        return false;
    }
    // Allow explicit --state filter to override hiding
    if let Some(ref explicit_state) = filter.state {
        return explicit_state != &state_lower;
    }
    true
}

fn has_all_tags(knot: &KnotView, required_tags: &[String]) -> bool {
    if required_tags.is_empty() {
        return true;
    }
    let knot_tags: Vec<String> = knot
        .tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .collect();
    required_tags
        .iter()
        .all(|tag| knot_tags.iter().any(|existing| existing == tag))
}

fn matches_query(knot: &KnotView, query: &str) -> bool {
    let query = query.to_ascii_lowercase();
    let alias = knot.alias.as_deref().unwrap_or("").to_ascii_lowercase();
    let description = knot
        .description
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let body = knot.body.as_deref().unwrap_or("").to_ascii_lowercase();

    knot.id.to_ascii_lowercase().contains(&query)
        || alias.contains(&query)
        || knot.title.to_ascii_lowercase().contains(&query)
        || description.contains(&query)
        || body.contains(&query)
        || knot.profile_id.to_ascii_lowercase().contains(&query)
}

fn normalize_scalar(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn normalize_knot_type_filter(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: KnotType = trimmed.parse().ok()?;
    Some(parsed.as_str().to_string())
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "listing_tests_ext.rs"]
mod tests_ext;

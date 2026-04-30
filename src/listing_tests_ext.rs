use super::{apply_filters, filter_and_paginate, normalize_knot_type_filter, KnotListFilter};
use crate::app::KnotView;
use crate::domain::knot_type::KnotType;

fn knot(
    id: &str,
    title: &str,
    state: &str,
    knot_type: Option<&str>,
    tags: &[&str],
    description: Option<&str>,
) -> KnotView {
    KnotView {
        id: id.to_string(),
        alias: None,
        title: title.to_string(),
        state: state.to_string(),
        updated_at: "2026-02-23T10:00:00Z".to_string(),
        body: None,
        description: description.map(|value| value.to_string()),
        acceptance: None,
        priority: None,
        knot_type: crate::domain::knot_type::parse_knot_type(knot_type),
        tags: tags.iter().map(|value| (*value).to_string()).collect(),
        notes: Vec::new(),
        handoff_capsules: Vec::new(),
        invariants: Vec::new(),
        step_history: Vec::new(),
        gate: None,
        lease: None,
        execution_plan: None,
        lease_id: None,
        lease_expiry_ts: 0,
        lease_agent: None,
        workflow_id: "work_sdlc".to_string(),
        profile_id: "autopilot".to_string(),
        profile_etag: None,
        deferred_from_state: None,
        blocked_from_state: None,
        created_at: None,
        step_metadata: None,
        next_step_metadata: None,
        edges: Vec::new(),
        child_summaries: vec![],
    }
}

#[test]
fn filters_by_type_normalizes_legacy_aliases() {
    let knots = vec![
        knot("K-1", "Alpha", "work_item", Some("work"), &[], None),
        knot("K-2", "Beta", "work_item", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: Some("task".to_string()),
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn invalid_type_filter_is_ignored() {
    let knots = vec![knot("K-1", "Alpha", "work_item", Some("work"), &[], None)];
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: Some("epic".to_string()),
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
}

#[test]
fn include_all_with_user_filter_includes_terminal_knots() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("work"), &[], None),
        knot("K-2", "Done", "shipped", Some("work"), &["cli"], None),
    ];
    let filter = KnotListFilter {
        include_all: true,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: vec!["cli".to_string()],
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

#[test]
fn normalize_knot_type_filter_covers_edge_cases() {
    assert_eq!(normalize_knot_type_filter(None), None);
    assert_eq!(normalize_knot_type_filter(Some("")), None);
    assert_eq!(normalize_knot_type_filter(Some("  ")), None);
    assert_eq!(
        normalize_knot_type_filter(Some("task")),
        Some("work".to_string())
    );
    assert_eq!(
        normalize_knot_type_filter(Some("work")),
        Some("work".to_string())
    );
    assert_eq!(normalize_knot_type_filter(Some("epic")), None);
}

#[test]
fn empty_state_filter_is_treated_as_no_filter() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("work"), &[], None),
        knot("K-2", "Other", "work_item", Some("work"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("".to_string()),
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn whitespace_only_query_filter_is_treated_as_no_filter() {
    let knots = vec![knot(
        "K-1",
        "Active",
        "implementing",
        Some("work"),
        &[],
        None,
    )];
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: Some("   ".to_string()),
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
}

#[test]
fn hides_lease_knots_by_default() {
    let mut lease = knot("K-3", "My Lease", "lease_ready", Some("lease"), &[], None);
    lease.knot_type = KnotType::Lease;
    let knots = vec![lease];
    let filter = KnotListFilter::default();

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 0);
}

#[test]
fn shows_lease_knots_with_type_filter() {
    let mut lease = knot("K-3", "My Lease", "lease_ready", Some("lease"), &[], None);
    lease.knot_type = KnotType::Lease;
    let knots = vec![lease];
    let filter = KnotListFilter {
        knot_type: Some("lease".to_string()),
        ..KnotListFilter::default()
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-3");
}

#[test]
fn filters_by_alias_query() {
    let mut with_alias = knot(
        "repo-a1b2",
        "Alias Target",
        "work_item",
        Some("task"),
        &[],
        None,
    );
    with_alias.alias = Some("repo-root.1".to_string());
    let without_alias = knot("repo-c3d4", "Other", "work_item", Some("task"), &[], None);
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: Some("root.1".to_string()),
    };

    let filtered = apply_filters(vec![with_alias, without_alias], &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "repo-a1b2");
}

#[test]
fn filters_by_profile_id() {
    let mut triage = knot("K-1", "Triage item", "work_item", Some("task"), &[], None);
    triage.profile_id = "triage".to_string();
    let default = knot("K-2", "Default item", "work_item", Some("task"), &[], None);
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: None,
        profile_id: Some("triage".to_string()),
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(vec![triage, default], &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn does_not_hide_non_lease_knots() {
    let knots = vec![knot(
        "K-1",
        "Active Work",
        "implementing",
        Some("work"),
        &[],
        None,
    )];
    let filter = KnotListFilter::default();

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

fn tagged_knot(id: &str, tags: &[&str]) -> KnotView {
    knot(id, id, "implementing", Some("work"), tags, None)
}

#[test]
fn filter_and_paginate_total_reflects_filtered_count() {
    let knots = vec![
        tagged_knot("K-1", &["release"]),
        tagged_knot("K-2", &[]),
        tagged_knot("K-3", &["release"]),
        tagged_knot("K-4", &[]),
    ];
    let filter = KnotListFilter {
        tags: vec!["release".to_string()],
        ..KnotListFilter::default()
    };

    let (page, total) = filter_and_paginate(knots, &filter, 0, 50);
    assert_eq!(total, 2);
    assert_eq!(page.len(), 2);
}

#[test]
fn filter_and_paginate_limit_one_returns_one_match_when_present() {
    let knots = vec![
        tagged_knot("K-1", &[]),
        tagged_knot("K-2", &["release"]),
        tagged_knot("K-3", &[]),
    ];
    let filter = KnotListFilter {
        tags: vec!["release".to_string()],
        ..KnotListFilter::default()
    };

    let (page, total) = filter_and_paginate(knots, &filter, 0, 1);
    assert_eq!(total, 1);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, "K-2");
}

#[test]
fn filter_and_paginate_pages_are_stable_across_offsets() {
    let knots: Vec<KnotView> = (0..5)
        .map(|i| tagged_knot(&format!("K-{i}"), &["release"]))
        .collect();
    let filter = KnotListFilter {
        tags: vec!["release".to_string()],
        ..KnotListFilter::default()
    };

    let (page0, total0) = filter_and_paginate(knots.clone(), &filter, 0, 2);
    let (page1, total1) = filter_and_paginate(knots.clone(), &filter, 2, 2);
    let (page2, total2) = filter_and_paginate(knots, &filter, 4, 2);

    assert_eq!(total0, 5);
    assert_eq!(total1, 5);
    assert_eq!(total2, 5);
    assert_eq!(page0.len(), 2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);

    let union: Vec<String> = page0
        .into_iter()
        .chain(page1)
        .chain(page2)
        .map(|k| k.id)
        .collect();
    assert_eq!(union.len(), 5);
    let mut sorted = union.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 5, "pages overlap or skip rows: {union:?}");
}

#[test]
fn filter_and_paginate_offset_beyond_total_returns_empty() {
    let knots = vec![tagged_knot("K-1", &["release"])];
    let filter = KnotListFilter {
        tags: vec!["release".to_string()],
        ..KnotListFilter::default()
    };

    let (page, total) = filter_and_paginate(knots, &filter, 100, 10);
    assert_eq!(total, 1);
    assert!(page.is_empty());
}

#[test]
fn filter_and_paginate_state_filter_paginates_shipped() {
    let knots = vec![
        knot("K-1", "K-1", "shipped", Some("work"), &[], None),
        knot("K-2", "K-2", "implementing", Some("work"), &[], None),
        knot("K-3", "K-3", "shipped", Some("work"), &[], None),
    ];
    let filter = KnotListFilter {
        state: Some("shipped".to_string()),
        ..KnotListFilter::default()
    };

    let (page, total) = filter_and_paginate(knots, &filter, 0, 1);
    assert_eq!(total, 2);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].state, "shipped");
}

#[test]
fn filter_and_paginate_zero_matches_reports_zero_total() {
    let knots = vec![tagged_knot("K-1", &["release"])];
    let filter = KnotListFilter {
        tags: vec!["nonexistent".to_string()],
        ..KnotListFilter::default()
    };

    let (page, total) = filter_and_paginate(knots, &filter, 0, 50);
    assert_eq!(total, 0);
    assert!(page.is_empty());
}

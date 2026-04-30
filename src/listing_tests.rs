use super::{apply_filters, KnotListFilter};
use crate::app::KnotView;

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
fn filters_by_state_case_insensitive() {
    let knots = vec![
        knot("K-1", "Plan filters", "idea", Some("task"), &["ux"], None),
        knot(
            "K-2",
            "Ship UI",
            "implementing",
            Some("task"),
            &["release"],
            None,
        ),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("ImPlementing".to_string()),
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

#[test]
fn filters_by_multiple_tags() {
    let knots = vec![
        knot(
            "K-1",
            "Importer",
            "work_item",
            Some("task"),
            &["migration", "sync"],
            None,
        ),
        knot(
            "K-2",
            "Docs",
            "work_item",
            Some("task"),
            &["migration"],
            None,
        ),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: vec!["migration".to_string(), "sync".to_string()],
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn filters_by_query_across_title_and_description() {
    let knots = vec![
        knot(
            "K-1",
            "Polish ls output",
            "reviewing",
            Some("task"),
            &["ux"],
            Some("needs style"),
        ),
        knot(
            "K-2",
            "Refactor imports",
            "implementing",
            Some("task"),
            &["infra"],
            Some("carry checkpoint"),
        ),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: Some("STYLE".to_string()),
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn combines_filters() {
    let knots = vec![
        knot(
            "K-1",
            "Release flow",
            "implementing",
            Some("work"),
            &["release", "cli"],
            None,
        ),
        knot(
            "K-2",
            "Release docs",
            "implementing",
            Some("work"),
            &["release", "docs"],
            None,
        ),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("implementing".to_string()),
        knot_type: Some("work".to_string()),
        profile_id: None,
        tags: vec!["release".to_string()],
        query: Some("flow".to_string()),
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn excludes_shipped_by_default() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Done", "shipped", Some("task"), &[], None),
    ];
    let filter = KnotListFilter::default();

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn includes_shipped_with_all_flag() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Done", "shipped", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: true,
        state: None,
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn allows_state_shipped_without_all_flag() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Done", "shipped", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("shipped".to_string()),
        knot_type: None,
        profile_id: None,
        tags: Vec::new(),
        query: None,
    };

    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

#[test]
fn excludes_abandoned_by_default() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Gone", "abandoned", Some("task"), &[], None),
    ];
    let filter = KnotListFilter::default();
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-1");
}

#[test]
fn includes_deferred_by_default() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Later", "deferred", Some("task"), &[], None),
    ];
    let filter = KnotListFilter::default();
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn includes_abandoned_and_deferred_with_all_flag() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Gone", "abandoned", Some("task"), &[], None),
        knot("K-3", "Later", "deferred", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: true,
        ..KnotListFilter::default()
    };
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 3);
}

#[test]
fn allows_state_abandoned_explicit() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Gone", "abandoned", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("abandoned".to_string()),
        ..KnotListFilter::default()
    };
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

#[test]
fn filters_by_knot_type() {
    let knots = vec![
        knot("K-1", "Bug fix", "planning", Some("task"), &[], None),
        knot("K-2", "Quality gate", "planning", Some("gate"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        knot_type: Some("gate".to_string()),
        ..KnotListFilter::default()
    };
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

#[test]
fn allows_state_deferred_explicit() {
    let knots = vec![
        knot("K-1", "Active", "implementing", Some("task"), &[], None),
        knot("K-2", "Later", "deferred", Some("task"), &[], None),
    ];
    let filter = KnotListFilter {
        include_all: false,
        state: Some("deferred".to_string()),
        ..KnotListFilter::default()
    };
    let filtered = apply_filters(knots, &filter);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "K-2");
}

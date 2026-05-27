use crate::app::{KnotView, PaginatedList};
use crate::listing;

pub fn compute_paginated_list(
    knots: Vec<KnotView>,
    filter: &listing::KnotListFilter,
    offset: usize,
    limit: usize,
) -> PaginatedList<KnotView> {
    let filtered = listing::apply_filters(knots, filter);
    let total = filtered.len() as i64;
    let page: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
    PaginatedList::new(page, total, offset, limit)
}

#[cfg(test)]
mod tests {
    use super::compute_paginated_list;
    use crate::app::KnotView;
    use crate::domain::knot_type::KnotType;
    use crate::listing::KnotListFilter;

    fn knot_view(id: &str, title: &str, description: Option<&str>) -> KnotView {
        KnotView {
            id: id.to_string(),
            alias: None,
            title: title.to_string(),
            state: "implementation".to_string(),
            updated_at: "2026-02-23T10:00:00Z".to_string(),
            body: None,
            description: description.map(|d| d.to_string()),
            acceptance: None,
            priority: None,
            knot_type: KnotType::Work,
            tags: Vec::new(),
            notes: Vec::new(),
            handoff_capsules: Vec::new(),
            invariants: Vec::new(),
            verification_steps: Vec::new(),
            step_history: Vec::new(),
            gate: None,
            lease: None,
            execution_plan: None,
            scope: None,
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
            child_summaries: Vec::new(),
        }
    }

    fn matching_query_filter() -> KnotListFilter {
        KnotListFilter {
            include_all: true,
            state: None,
            knot_type: None,
            profile_id: None,
            tags: Vec::new(),
            query: Some("escalation".to_string()),
        }
    }

    fn matching_tag_filter() -> KnotListFilter {
        KnotListFilter {
            include_all: true,
            state: None,
            knot_type: None,
            profile_id: None,
            tags: vec!["tagged".to_string()],
            query: None,
        }
    }

    fn tagged_knot_view(
        id: &str,
        title: &str,
        tags: Vec<&str>,
        description: Option<&str>,
    ) -> KnotView {
        KnotView {
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            ..knot_view(id, title, description)
        }
    }

    #[test]
    fn empty_match_set_reports_zero_total_and_no_more() {
        let knots: Vec<KnotView> = (0..50)
            .map(|i| {
                knot_view(
                    &format!("K-{i}"),
                    &format!("Routine knot {i}"),
                    Some("ordinary description"),
                )
            })
            .collect();

        let page = compute_paginated_list(knots, &matching_query_filter(), 0, 50);
        assert!(
            page.data.is_empty(),
            "expected empty data for non-matching query"
        );
        assert_eq!(page.total, 0, "total must reflect filtered match count");
        assert!(!page.has_more, "has_more must be false when no rows match");
        assert_eq!(page.offset, 0);
        assert_eq!(page.limit, 50);
    }

    #[test]
    fn multi_page_match_set_preserves_totals_and_has_more() {
        let make_knots = || -> Vec<KnotView> {
            (0..120)
                .map(|i| {
                    knot_view(
                        &format!("K-{i}"),
                        &format!("escalation flow {i}"),
                        Some("matches the query"),
                    )
                })
                .collect()
        };

        let page = compute_paginated_list(make_knots(), &matching_query_filter(), 0, 50);
        assert_eq!(page.data.len(), 50);
        assert_eq!(page.total, 120);
        assert!(page.has_more);

        let last_page = compute_paginated_list(make_knots(), &matching_query_filter(), 100, 50);
        assert_eq!(last_page.data.len(), 20);
        assert_eq!(last_page.total, 120);
        assert!(!last_page.has_more);
    }

    #[test]
    fn tag_filter_limit_one_returns_one_match_when_present() {
        let make_knots = || -> Vec<KnotView> {
            vec![
                tagged_knot_view("K-1", "One", vec!["other"], None),
                tagged_knot_view("K-2", "Two", vec!["tagged"], None),
                tagged_knot_view("K-3", "Three", vec!["other"], None),
            ]
        };

        let page = compute_paginated_list(make_knots(), &matching_tag_filter(), 0, 1);
        assert_eq!(page.data.len(), 1, "limit 1 should return one row");
        assert_eq!(page.data[0].id, "K-2");
        assert_eq!(page.total, 1, "total must reflect filtered count");
        assert!(!page.has_more);
    }

    #[test]
    fn tag_filter_pages_are_stable_across_offsets() {
        let make_knots = || -> Vec<KnotView> {
            (0..5)
                .map(|i| {
                    tagged_knot_view(
                        &format!("K-{i}"),
                        &format!("Knot {i}"),
                        vec!["tagged"],
                        None,
                    )
                })
                .chain((5..10).map(|i| {
                    tagged_knot_view(&format!("K-{i}"), &format!("Knot {i}"), vec!["other"], None)
                }))
                .collect()
        };

        let page0 = compute_paginated_list(make_knots(), &matching_tag_filter(), 0, 2);
        let page1 = compute_paginated_list(make_knots(), &matching_tag_filter(), 2, 2);
        let page2 = compute_paginated_list(make_knots(), &matching_tag_filter(), 4, 2);

        assert_eq!(page0.total, 5);
        assert_eq!(page1.total, 5);
        assert_eq!(page2.total, 5);
        assert_eq!(page0.data.len(), 2);
        assert_eq!(page1.data.len(), 2);
        assert_eq!(page2.data.len(), 1);

        let union: Vec<String> = page0
            .data
            .into_iter()
            .chain(page1.data)
            .chain(page2.data)
            .map(|k| k.id)
            .collect();
        assert_eq!(union.len(), 5);
        let mut sorted = union.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5, "pages overlap or skip rows: {union:?}");
    }

    #[test]
    fn tag_filter_offset_beyond_total_returns_empty() {
        let knots = vec![tagged_knot_view("K-1", "One", vec!["tagged"], None)];
        let page = compute_paginated_list(knots, &matching_tag_filter(), 100, 10);
        assert_eq!(page.total, 1);
        assert!(page.data.is_empty());
        assert!(!page.has_more);
    }

    #[test]
    fn tag_filter_zero_matches_reports_zero_total() {
        let knots = vec![tagged_knot_view("K-1", "One", vec!["other"], None)];
        let page = compute_paginated_list(knots, &matching_tag_filter(), 0, 50);
        assert_eq!(page.total, 0);
        assert!(page.data.is_empty());
        assert!(!page.has_more);
    }
}

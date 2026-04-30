mod cli_dispatch_helpers;

use cli_dispatch_helpers::*;
use serde_json::Value;

fn ls_page_ids(output: &std::process::Output) -> Vec<String> {
    let payload: Value = serde_json::from_slice(&output.stdout).expect("ls json");
    payload
        .get("data")
        .and_then(Value::as_array)
        .expect("ls payload should have data array")
        .iter()
        .filter_map(|knot| knot.get("id").and_then(Value::as_str))
        .map(|s| s.to_string())
        .collect()
}

fn ls_total(output: &std::process::Output) -> i64 {
    let payload: Value = serde_json::from_slice(&output.stdout).expect("ls json");
    payload
        .get("total")
        .and_then(Value::as_i64)
        .expect("ls payload should have total")
}

#[test]
fn ls_tag_filter_pagination_is_stable_and_total_matches_filtered() {
    let root = unique_workspace("knots-cli-ls-tag-pagination");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    for i in 0..5 {
        let title = format!("Tagged {i}");
        let result = run_knots(
            &root,
            &db,
            &[
                "new",
                &title,
                "--profile",
                "autopilot",
                "--state",
                "idea",
                "--tag",
                "approval-harness",
            ],
        );
        assert_success(&result);
    }
    for i in 0..3 {
        let title = format!("Other {i}");
        let result = run_knots(
            &root,
            &db,
            &["new", &title, "--profile", "autopilot", "--state", "idea"],
        );
        assert_success(&result);
    }

    let limit_one = run_knots(
        &root,
        &db,
        &["ls", "--tag", "approval-harness", "--json", "--limit", "1"],
    );
    assert_success(&limit_one);
    assert_eq!(ls_total(&limit_one), 5);
    let limit_one_ids = ls_page_ids(&limit_one);
    assert_eq!(limit_one_ids.len(), 1, "page should contain one match");

    let bulk = run_knots(
        &root,
        &db,
        &[
            "ls",
            "--tag",
            "approval-harness",
            "--json",
            "--limit",
            "1000",
        ],
    );
    assert_success(&bulk);
    assert_eq!(ls_total(&bulk), 5);
    let bulk_ids = ls_page_ids(&bulk);
    assert_eq!(bulk_ids.len(), 5);

    let mut seen: Vec<String> = Vec::new();
    for offset in 0..5 {
        let page = run_knots(
            &root,
            &db,
            &[
                "ls",
                "--tag",
                "approval-harness",
                "--json",
                "--limit",
                "1",
                "--offset",
                &offset.to_string(),
            ],
        );
        assert_success(&page);
        assert_eq!(
            ls_total(&page),
            5,
            "filtered total should be 5 at offset {offset}"
        );
        let ids = ls_page_ids(&page);
        assert_eq!(ids.len(), 1, "page at offset {offset} should have 1 row");
        seen.push(ids.into_iter().next().expect("one id"));
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 5, "pages should not overlap or skip rows");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ls_state_shipped_pagination_returns_only_shipped() {
    let root = unique_workspace("knots-cli-ls-state-shipped");
    setup_repo(&root);
    let db = root.join(".knots/cache/state.sqlite");

    for i in 0..3 {
        let title = format!("Done {i}");
        let result = run_knots(
            &root,
            &db,
            &[
                "new",
                &title,
                "--profile",
                "autopilot",
                "--state",
                "shipped",
            ],
        );
        assert_success(&result);
    }
    for i in 0..2 {
        let title = format!("Active {i}");
        let result = run_knots(
            &root,
            &db,
            &["new", &title, "--profile", "autopilot", "--state", "idea"],
        );
        assert_success(&result);
    }

    let page = run_knots(
        &root,
        &db,
        &[
            "ls", "--state", "shipped", "--json", "--limit", "2", "--offset", "0",
        ],
    );
    assert_success(&page);
    let payload: Value = serde_json::from_slice(&page.stdout).expect("ls json");
    assert_eq!(payload.get("total").and_then(Value::as_i64), Some(3));
    let data = payload
        .get("data")
        .and_then(Value::as_array)
        .expect("data array");
    assert_eq!(data.len(), 2);
    for knot in data {
        let state = knot
            .get("state")
            .and_then(Value::as_str)
            .expect("state field");
        assert_eq!(state, "shipped");
    }

    let _ = std::fs::remove_dir_all(root);
}

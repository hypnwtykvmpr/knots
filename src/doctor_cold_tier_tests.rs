use std::path::{Path, PathBuf};

use rusqlite::Connection;
use uuid::Uuid;

use super::{
    check_cold_tier_imbalance, check_cold_tier_imbalance_at, fix_cold_tier_imbalance,
    COLD_TIER_HOT_TARGET,
};
use crate::db;
use crate::doctor::DoctorStatus;
use crate::project::StorePaths;

fn unique_workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("knots-cold-tier-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&root).expect("workspace should be creatable");
    root
}

fn open_db(root: &Path) -> (String, Connection) {
    let db_path = root.join(".knots/cache/state.sqlite");
    let db_path_str = db_path.to_str().expect("utf8 db path").to_string();
    std::fs::create_dir_all(db_path.parent().expect("db parent should exist"))
        .expect("db parent should be creatable");
    let conn = db::open_connection(&db_path_str).expect("db should open");
    (db_path_str, conn)
}

fn insert_hot(conn: &Connection, id: &str, title: &str) {
    let tags = "[]";
    let notes = "[]";
    let handoff = "[]";
    let invariants = "[]";
    let history = "[]";
    let gate = "{}";
    let lease = "{}";
    let plan = "{}";
    conn.execute(
        r#"
INSERT INTO knot_hot (
    id, title, state, updated_at, body, description, acceptance,
    priority, knot_type, tags_json, notes_json,
    handoff_capsules_json, invariants_json, step_history_json,
    gate_data_json, lease_data_json, execution_plan_data_json, lease_id,
    workflow_id, profile_id, profile_etag,
    deferred_from_state, blocked_from_state, created_at
)
VALUES (
    ?1, ?2, 'implementation', '2026-04-10T00:00:00Z', NULL, NULL, NULL,
    NULL, 'work', ?3, ?4,
    ?5, ?6, ?7,
    ?8, ?9, ?10, NULL,
    'work_sdlc', 'autopilot', NULL,
    NULL, NULL, NULL
)
"#,
        rusqlite::params![id, title, tags, notes, handoff, invariants, history, gate, lease, plan],
    )
    .expect("hot insert should succeed");
}

fn insert_cold(conn: &Connection, id: &str, title: &str, updated_at: &str) {
    db::upsert_cold_catalog(conn, id, title, "implementation", updated_at)
        .expect("cold insert should succeed");
    db::upsert_knot_warm(conn, id, title).expect("warm insert should succeed");
}

fn write_knot_head_event(root: &Path, seq: u64, id: &str, title: &str, updated_at: &str) {
    let ym_dir = root
        .join(".knots")
        .join("index")
        .join("2026")
        .join("04")
        .join("10");
    std::fs::create_dir_all(&ym_dir).expect("index dir should be creatable");
    let path = ym_dir.join(format!("{seq:04}-idx.knot_head.json"));
    std::fs::write(
        &path,
        format!(
            concat!(
                "{{\n",
                "  \"event_id\": \"{seq}\",\n",
                "  \"occurred_at\": \"2026-04-10T00:00:00Z\",\n",
                "  \"type\": \"idx.knot_head\",\n",
                "  \"data\": {{\n",
                "    \"knot_id\": \"{id}\",\n",
                "    \"title\": \"{title}\",\n",
                "    \"state\": \"implementation\",\n",
                "    \"workflow_id\": \"work_sdlc\",\n",
                "    \"profile_id\": \"autopilot\",\n",
                "    \"updated_at\": \"{updated_at}\",\n",
                "    \"terminal\": false\n",
                "  }}\n",
                "}}\n"
            ),
            seq = seq,
            id = id,
            title = title,
            updated_at = updated_at
        ),
    )
    .expect("index event should be writable");
}

#[test]
fn check_passes_when_hot_at_or_above_target() {
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    for i in 0..COLD_TIER_HOT_TARGET {
        insert_hot(&conn, &format!("H-{i:03}"), "hot");
    }
    insert_cold(&conn, "C-001", "cold", "2026-04-09T00:00:00Z");

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Pass);
    assert!(check.detail.contains("100 hot"));
    assert!(check.detail.contains("1 cold"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_passes_when_cold_is_zero() {
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    insert_hot(&conn, "H-001", "hot");

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Pass);
    let data = check.data.as_ref().expect("data should be present");
    assert_eq!(data["hot_count"], 1);
    assert_eq!(data["cold_count"], 0);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_warns_when_hot_below_target_and_cold_present() {
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    for i in 0..10 {
        insert_hot(&conn, &format!("H-{i:03}"), "hot");
    }
    for i in 0..50 {
        insert_cold(
            &conn,
            &format!("C-{i:03}"),
            "cold",
            &format!("2026-04-0{}T00:00:00Z", (i % 9) + 1),
        );
    }

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Warn);
    assert!(check.detail.contains("10 hot / 50 cold"));
    // cap = min(100 - 10, 50) = 50
    assert!(check.detail.contains("rehydrate up to 50"));
    let data = check.data.as_ref().expect("data should be present");
    assert_eq!(data["hot_count"], 10);
    assert_eq!(data["cold_count"], 50);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_warn_cap_is_hot_target_minus_hot_when_cold_is_larger() {
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    for i in 0..10 {
        insert_hot(&conn, &format!("H-{i:03}"), "hot");
    }
    for i in 0..200 {
        insert_cold(
            &conn,
            &format!("C-{i:03}"),
            "cold",
            &format!("2026-04-{:02}T00:00:00Z", (i % 28) + 1),
        );
    }

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Warn);
    assert!(check.detail.contains("rehydrate up to 90"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_at_handles_missing_db_as_pass() {
    let root = unique_workspace();
    let store_paths = StorePaths {
        root: root.join(".knots"),
    };
    let check = check_cold_tier_imbalance_at(&store_paths).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Pass);
    assert!(check.detail.contains("no cache database found"));
    assert!(check.data.is_none());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_at_reads_live_database_counts() {
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    insert_hot(&conn, "H-1", "hot");
    insert_cold(&conn, "C-1", "cold", "2026-04-10T00:00:00Z");
    drop(conn);

    let store_paths = StorePaths {
        root: root.join(".knots"),
    };
    let check = check_cold_tier_imbalance_at(&store_paths).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Warn);
    let data = check.data.as_ref().expect("data should be present");
    assert_eq!(data["hot_count"], 1);
    assert_eq!(data["cold_count"], 1);

    let _ = std::fs::remove_dir_all(root);
}

fn seed_cold_with_events(root: &Path, count: usize) {
    let (_, conn) = open_db(root);
    for i in 0..count {
        let id = format!("C-{i:03}");
        // Encode updated_at to sort: newer when i is larger.
        let updated_at = format!("2026-04-{:02}T00:00:0{}Z", 10 + (i / 10), i % 10);
        insert_cold(&conn, &id, "cold", &updated_at);
        write_knot_head_event(root, 1000 + i as u64, &id, "cold", &updated_at);
    }
    drop(conn);
}

#[test]
fn fix_noop_when_hot_at_or_above_target() {
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        for i in 0..COLD_TIER_HOT_TARGET {
            insert_hot(&conn, &format!("H-{i:03}"), "hot");
        }
        insert_cold(&conn, "C-1", "cold", "2026-04-09T00:00:00Z");
    }
    fix_cold_tier_imbalance(&root);

    let (_, conn) = open_db(&root);
    let cold_count = db::count_cold_catalog(&conn).expect("count should run");
    assert_eq!(cold_count, 1, "cold catalog must remain untouched");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_noop_when_db_missing() {
    let root = unique_workspace();
    // No db at all — must not panic.
    fix_cold_tier_imbalance(&root);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_drains_cold_when_capacity_exceeds_cold_count() {
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        for i in 0..10 {
            insert_hot(&conn, &format!("H-{i:03}"), "hot");
        }
    }
    seed_cold_with_events(&root, 50);

    fix_cold_tier_imbalance(&root);

    let (_, conn) = open_db(&root);
    let hot = db::count_knot_hot(&conn).expect("hot count should run");
    let cold = db::count_cold_catalog(&conn).expect("cold count should run");
    assert_eq!(hot, 60, "all 50 cold knots should have been rehydrated");
    assert_eq!(cold, 0, "cold catalog should be drained");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_caps_rehydrate_at_hot_target_when_cold_is_larger() {
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        for i in 0..10 {
            insert_hot(&conn, &format!("H-{i:03}"), "hot");
        }
    }
    seed_cold_with_events(&root, 200);

    fix_cold_tier_imbalance(&root);

    let (_, conn) = open_db(&root);
    let hot = db::count_knot_hot(&conn).expect("hot count should run");
    let cold = db::count_cold_catalog(&conn).expect("cold count should run");
    assert_eq!(hot, 100, "hot cache should be filled to the target");
    assert_eq!(cold, 110, "cold catalog should retain the overflow");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_rehydrates_newest_first_by_updated_at() {
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        for i in 0..99 {
            insert_hot(&conn, &format!("H-{i:03}"), "hot");
        }
    }
    // Three cold records; newest first should be the one with the latest
    // updated_at. Cap = 100 - 99 = 1.
    {
        let (_, conn) = open_db(&root);
        insert_cold(&conn, "C-old", "cold-old", "2026-04-01T00:00:00Z");
        insert_cold(&conn, "C-mid", "cold-mid", "2026-04-05T00:00:00Z");
        insert_cold(&conn, "C-new", "cold-new", "2026-04-09T00:00:00Z");
    }
    write_knot_head_event(&root, 2001, "C-old", "cold-old", "2026-04-01T00:00:00Z");
    write_knot_head_event(&root, 2002, "C-mid", "cold-mid", "2026-04-05T00:00:00Z");
    write_knot_head_event(&root, 2003, "C-new", "cold-new", "2026-04-09T00:00:00Z");

    fix_cold_tier_imbalance(&root);

    let (_, conn) = open_db(&root);
    let newly_hot: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knot_hot WHERE id = 'C-new'",
            [],
            |row| row.get(0),
        )
        .expect("query should run");
    assert_eq!(newly_hot, 1, "newest cold knot should have been rehydrated");
    let old_still_cold: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cold_catalog WHERE id = 'C-old'",
            [],
            |row| row.get(0),
        )
        .expect("query should run");
    assert_eq!(old_still_cold, 1, "older cold knot should remain cold");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_warns_and_reports_shadowed_when_all_cold_already_hot() {
    // Regression for the "edge case": the only cold row is also in hot. It
    // is not a rehydrate candidate — but counting it as plain cold produced
    // a permanent warn the fix could never clear. Now the warn detail
    // surfaces the shadowed count and prompts the user to prune.
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    for i in 0..30 {
        insert_hot(&conn, &format!("H-{i:03}"), "hot");
    }
    insert_hot(&conn, "SHADOW", "shared");
    insert_cold(&conn, "SHADOW", "shared", "2026-04-09T00:00:00Z");

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Warn);
    assert!(
        check.detail.contains("shadowed"),
        "detail should mention shadowed rows: {}",
        check.detail
    );
    let data = check.data.as_ref().expect("data should be present");
    assert_eq!(data["hot_count"], 31);
    assert_eq!(data["cold_count"], 1);
    assert_eq!(data["shadowed"], 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_warn_cap_excludes_shadowed_cold_rows() {
    // Mixed scenario: one genuine cold record + one shadowed. The rehydrate
    // cap should cover the genuine one only; the detail should still call
    // out the shadowed row so the user sees both effects of --fix.
    let root = unique_workspace();
    let (_, conn) = open_db(&root);
    for i in 0..30 {
        insert_hot(&conn, &format!("H-{i:03}"), "hot");
    }
    insert_hot(&conn, "SHADOW", "shared");
    insert_cold(&conn, "SHADOW", "shared", "2026-04-09T00:00:00Z");
    insert_cold(&conn, "GENUINE", "genuine", "2026-04-10T00:00:00Z");

    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Warn);
    assert!(
        check.detail.contains("rehydrate up to 1"),
        "cap should exclude shadowed row: {}",
        check.detail
    );
    assert!(
        check.detail.contains("1 shadowed by hot will be pruned"),
        "detail should surface shadowed prune: {}",
        check.detail
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_prunes_cold_rows_shadowed_by_hot_and_clears_warn() {
    // End-to-end repro of the bug: a single cold row shadowed by hot must
    // be pruned by --fix so the next doctor run reports pass.
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        for i in 0..30 {
            insert_hot(&conn, &format!("H-{i:03}"), "hot");
        }
        insert_hot(&conn, "SHADOW", "shared");
        insert_cold(&conn, "SHADOW", "shared", "2026-04-09T00:00:00Z");
    }

    fix_cold_tier_imbalance(&root);

    let (_, conn) = open_db(&root);
    let cold = db::count_cold_catalog(&conn).expect("count cold should run");
    assert_eq!(cold, 0, "shadowed cold row should be pruned by --fix");
    let hot = db::count_knot_hot(&conn).expect("count hot should run");
    assert_eq!(
        hot, 31,
        "hot rows are unaffected by pruning cold duplicates"
    );
    let check = check_cold_tier_imbalance(&conn).expect("check should run");
    assert_eq!(check.status, DoctorStatus::Pass);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fix_rehydrated_knot_has_fresh_updated_at() {
    let root = unique_workspace();
    {
        let (_, conn) = open_db(&root);
        insert_hot(&conn, "H-1", "hot");
        insert_cold(&conn, "C-1", "cold", "2020-01-01T00:00:00Z");
    }
    write_knot_head_event(&root, 3001, "C-1", "cold", "2020-01-01T00:00:00Z");

    let before = time::OffsetDateTime::now_utc();
    fix_cold_tier_imbalance(&root);
    let after = time::OffsetDateTime::now_utc();

    let (_, conn) = open_db(&root);
    let updated_at: String = conn
        .query_row(
            "SELECT updated_at FROM knot_hot WHERE id = 'C-1'",
            [],
            |row| row.get(0),
        )
        .expect("knot should be rehydrated into hot");
    let parsed =
        time::OffsetDateTime::parse(&updated_at, &time::format_description::well_known::Rfc3339)
            .expect("rehydrated updated_at should parse");
    assert!(
        parsed >= before - time::Duration::seconds(1)
            && parsed <= after + time::Duration::seconds(1),
        "updated_at {updated_at} should be within 1s of now (before={before}, after={after})"
    );

    let _ = std::fs::remove_dir_all(root);
}

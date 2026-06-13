#[cfg(not(tarpaulin_include))]
use std::time::Duration;

#[cfg(not(tarpaulin_include))]
use crate::runner::KnoRunner;
use serde_json::Value;

#[cfg(not(tarpaulin_include))]
pub fn spawn_background_sync(runner: KnoRunner, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            match runner.run("sync", &[]) {
                Ok(value) => {
                    if let Some(detail) = deferred_sync_detail(&value) {
                        eprintln!("kno-mcp sync deferred; retry pending: {detail}");
                    }
                }
                Err(err) => eprintln!("kno-mcp sync retry pending: {}", err.stderr),
            }
        }
    });
}

fn deferred_sync_detail(value: &Value) -> Option<String> {
    if value.get("status").and_then(Value::as_str) != Some("deferred") {
        return None;
    }
    let Some(active_leases) = value.get("active_leases").and_then(Value::as_u64) else {
        return Some("status=deferred".to_string());
    };
    Some(format!("active_leases={active_leases}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deferred_sync_detail_reports_active_leases() {
        let detail = deferred_sync_detail(&json!({
            "status": "deferred",
            "active_leases": 2
        }));
        assert_eq!(detail, Some("active_leases=2".to_string()));
    }

    #[test]
    fn deferred_sync_detail_covers_missing_count_and_completed() {
        assert_eq!(
            deferred_sync_detail(&json!({ "status": "deferred" })),
            Some("status=deferred".to_string())
        );
        assert_eq!(
            deferred_sync_detail(&json!({ "status": "completed" })),
            None
        );
    }
}

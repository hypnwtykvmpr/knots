#[cfg(not(tarpaulin_include))]
use std::time::Duration;

#[cfg(not(tarpaulin_include))]
use crate::runner::{KnoFailure, KnoRunner};
use serde_json::Value;

#[cfg(not(tarpaulin_include))]
pub fn spawn_background_sync(runner: KnoRunner, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            // The sync subprocess blocks; keep it off the async workers.
            let sync_runner = runner.clone();
            let result = tokio::task::spawn_blocking(move || sync_runner.run("sync", &[])).await;
            let message = match result {
                Ok(outcome) => sync_retry_message(outcome),
                Err(err) => Some(format!("kno-mcp sync worker failed: {err}")),
            };
            if let Some(message) = message {
                eprintln!("{message}");
            }
        }
    });
}

#[cfg(not(tarpaulin_include))]
fn sync_retry_message(result: Result<Value, KnoFailure>) -> Option<String> {
    match result {
        Ok(value) => deferred_sync_detail(&value)
            .map(|detail| format!("kno-mcp sync deferred; retry pending: {detail}")),
        Err(err) => Some(format!("kno-mcp sync retry pending: {}", err.stderr)),
    }
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

    #[tokio::test]
    async fn background_sync_executes_its_first_tick() {
        let fixture = if cfg!(windows) {
            "tests/fixtures/kno-stub.ps1"
        } else {
            "tests/fixtures/kno-stub.sh"
        };
        let runner = KnoRunner::new(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture),
            std::path::PathBuf::from("/tmp/repo"),
        );
        // The first interval tick fires immediately; give the blocking sync
        // one moment to run against the stub (which reports deferred).
        spawn_background_sync(runner, Duration::from_secs(3600));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

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

    #[test]
    fn sync_retry_message_formats_deferred_and_error_results() {
        let deferred = sync_retry_message(Ok(json!({
            "status": "deferred",
            "active_leases": 3
        })));
        assert_eq!(
            deferred,
            Some("kno-mcp sync deferred; retry pending: active_leases=3".to_string())
        );

        let completed = sync_retry_message(Ok(json!({ "status": "completed" })));
        assert_eq!(completed, None);

        let failed = sync_retry_message(Err(KnoFailure {
            exit_code: Some(1),
            stderr: "still busy".to_string(),
        }));
        assert_eq!(
            failed,
            Some("kno-mcp sync retry pending: still busy".to_string())
        );
    }
}

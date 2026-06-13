use std::time::Duration;

use crate::runner::KnoRunner;

#[cfg(not(tarpaulin_include))]
pub fn spawn_background_sync(runner: KnoRunner, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            let result = runner.run("sync", &[]);
            if let Err(err) = result {
                eprintln!("kno-mcp sync retry pending: {}", err.stderr);
            }
        }
    });
}

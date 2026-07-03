#[path = "kno_mcp/args.rs"]
mod args;
#[path = "kno_mcp/auth.rs"]
mod auth;
#[path = "../native_command.rs"]
mod native_command;
#[path = "kno_mcp/runner.rs"]
mod runner;
#[path = "kno_mcp/server.rs"]
mod server;
#[path = "kno_mcp/session.rs"]
mod session;
#[path = "kno_mcp/sync_loop.rs"]
mod sync_loop;
#[path = "kno_mcp/tools.rs"]
mod tools;

use std::error::Error;

use args::{Cli, CommandMode};
use clap::Parser;
use rmcp::ServiceExt;

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let config = cli.into_config()?;
    match config.mode {
        CommandMode::Version => {
            println!("kno-mcp {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        CommandMode::Stdio => {
            let server = server::KnoMcp::new(config.server);
            let shutdown_handle = server.clone();
            let running = server.serve(rmcp::transport::stdio()).await?;
            running.waiting().await?;
            // The stdio transport is gone; release the session lease so
            // sync is not deferred for the rest of the lease timeout.
            tokio::task::spawn_blocking(move || shutdown_handle.terminate_session_leases()).await?;
            Ok(())
        }
        CommandMode::Serve(http) => server::serve_http(config.server, http).await,
    }
}

#[cfg(tarpaulin_include)]
fn main() {}

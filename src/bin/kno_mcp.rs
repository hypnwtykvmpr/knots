#[path = "kno_mcp/args.rs"]
mod args;
#[path = "kno_mcp/auth.rs"]
mod auth;
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
            let running = server.serve(rmcp::transport::stdio()).await?;
            running.waiting().await?;
            Ok(())
        }
        CommandMode::Serve(http) => server::serve_http(config.server, http).await,
    }
}

#[cfg(tarpaulin_include)]
fn main() {}

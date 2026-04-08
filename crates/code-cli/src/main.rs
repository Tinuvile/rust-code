//! CLI entry point: argument parsing, bootstrap sequence, mode dispatch.
//!
//! Ref: src/main.tsx, src/entrypoints/init.ts, src/replLauncher.tsx

mod args;
mod bootstrap;
mod output;
mod mcp_server;
mod telemetry;
mod update_check;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    // Initialize tracing before anything else so startup events are captured.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    // Build the tokio runtime and hand off to async entry point.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<()> {
    use args::Cli;
    use clap::Parser;

    let cli = Cli::parse();
    bootstrap::run(cli).await
}

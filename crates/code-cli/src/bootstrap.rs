use anyhow::Result;

use crate::args::{Cli, Subcommand};

/// Main bootstrap sequence:
/// config → settings → auth → tools → commands → MCP → hooks → skills → QueryEngine → dispatch
///
/// Ref: src/main.tsx, src/entrypoints/init.ts
pub async fn run(cli: Cli) -> Result<()> {
    // TODO: Phase 1 — parallel prefetch (MDM settings, keychain, API preconnect, feature flags)
    // TODO: Phase 2 — load config + settings
    // TODO: Phase 3 — initialize auth
    // TODO: Phase 4 — build tool registry
    // TODO: Phase 5 — build command registry
    // TODO: Phase 6 — connect MCP servers
    // TODO: Phase 7 — load hooks, skills, agents
    // TODO: Phase 8 — create QueryEngine
    // TODO: Phase 9 — dispatch to mode

    match &cli.subcommand {
        Some(Subcommand::Mcp(_)) => crate::mcp_server::serve().await,
        None => {
            if cli.print || cli.command.is_some() {
                crate::output::run_non_interactive(cli).await
            } else {
                // TODO: launch TUI REPL
                println!("TUI not yet implemented. Use --print for non-interactive mode.");
                Ok(())
            }
        }
    }
}

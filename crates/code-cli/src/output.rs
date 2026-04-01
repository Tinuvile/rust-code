use anyhow::Result;

use crate::args::Cli;

/// Non-interactive mode: run a single query, print result, exit.
///
/// Ref: src/main.tsx print/command handling
pub async fn run_non_interactive(cli: Cli) -> Result<()> {
    let _prompt = cli.prompt.or(cli.command).unwrap_or_default();
    // TODO: initialize QueryEngine, run query, print result
    Ok(())
}

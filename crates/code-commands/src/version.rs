//! `/version` — print the CLI version.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct VersionCommand;

#[async_trait]
impl Command for VersionCommand {
    fn name(&self) -> &str { "version" }
    fn description(&self) -> &str { "Show the Claude Code version" }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        Ok(CommandOutput::Text(format!(
            "Claude Code {}",
            env!("CARGO_PKG_VERSION")
        )))
    }
}

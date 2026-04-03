//! `/resume` — resume a previous session.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ResumeCommand;

#[async_trait]
impl Command for ResumeCommand {
    fn name(&self) -> &str {
        "resume"
    }

    fn description(&self) -> &str {
        "Resume a previous session."
    }

    fn usage(&self) -> Option<&str> {
        Some("/resume [session-id]")
    }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        Ok(CommandOutput::Markdown("No previous sessions found.".to_owned()))
    }
}

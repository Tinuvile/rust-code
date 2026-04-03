//! `/review` — ask the model to review the current git diff.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ReviewCommand;

#[async_trait]
impl Command for ReviewCommand {
    fn name(&self) -> &str {
        "review"
    }

    fn description(&self) -> &str {
        "Ask the model to review the current git diff and suggest improvements."
    }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        Ok(CommandOutput::Query(
            "Please review the current git diff and suggest improvements.".to_owned(),
        ))
    }
}

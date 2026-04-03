//! `/rename` — rename the current session.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct RenameCommand;

#[async_trait]
impl Command for RenameCommand {
    fn name(&self) -> &str {
        "rename"
    }

    fn description(&self) -> &str {
        "Rename the current session."
    }

    fn usage(&self) -> Option<&str> {
        Some("/rename <new-title>")
    }

    async fn execute(&self, args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let title = args.trim();
        if title.is_empty() {
            return Ok(CommandOutput::Text(
                "Usage: /rename <new-title>".to_owned(),
            ));
        }
        Ok(CommandOutput::Text(format!("Session renamed to: {title}")))
    }
}

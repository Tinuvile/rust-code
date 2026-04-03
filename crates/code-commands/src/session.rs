//! `/session` — show current session information.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct SessionCommand;

#[async_trait]
impl Command for SessionCommand {
    fn name(&self) -> &str {
        "session"
    }

    fn description(&self) -> &str {
        "Show current session information."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let session_id = format!("{}", ctx.session_id);
        let cwd = ctx.cwd.display().to_string();
        let count = ctx.conversation.read().await.len();

        let md = format!(
            "## Current Session\n\n\
             | Field | Value |\n\
             |-------|-------|\n\
             | Session ID | {session_id} |\n\
             | Working Directory | {cwd} |\n\
             | Messages | {count} |"
        );
        Ok(CommandOutput::Markdown(md))
    }
}

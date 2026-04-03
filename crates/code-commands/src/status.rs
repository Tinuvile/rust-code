//! `/status` — show current session status.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct StatusCommand;

#[async_trait]
impl Command for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "Show current session status (session ID, CWD, message count)."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let session_id = format!("{}", ctx.session_id);
        let cwd = ctx.cwd.display().to_string();
        let count = ctx.conversation.read().await.len();

        let md = format!(
            "| Field | Value |\n\
             |-------|-------|\n\
             | Session | {session_id} |\n\
             | CWD | {cwd} |\n\
             | Messages | {count} |"
        );
        Ok(CommandOutput::Markdown(md))
    }
}

//! `/clear` — clear the conversation history.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ClearCommand;

#[async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "Clear the conversation history."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.conversation.write().await.clear();
        Ok(CommandOutput::Text("Conversation cleared.".to_owned()))
    }
}

//! `/logout` — clear authentication credentials.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct LogoutCommand;

#[async_trait]
impl Command for LogoutCommand {
    fn name(&self) -> &str {
        "logout"
    }

    fn description(&self) -> &str {
        "Clear authentication credentials and API key."
    }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        Ok(CommandOutput::Text("Logged out. API key cleared.".to_owned()))
    }
}

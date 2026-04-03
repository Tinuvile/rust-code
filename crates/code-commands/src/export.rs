//! `/export` — export the conversation to a JSON file.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ExportCommand;

#[async_trait]
impl Command for ExportCommand {
    fn name(&self) -> &str {
        "export"
    }

    fn description(&self) -> &str {
        "Export the conversation to a JSON file."
    }

    fn usage(&self) -> Option<&str> {
        Some("/export [filename]")
    }

    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let filename = if args.trim().is_empty() {
            "conversation.json".to_owned()
        } else {
            args.trim().to_owned()
        };

        let path = ctx.cwd.join(&filename);
        let messages = ctx.conversation.read().await;
        let json = serde_json::to_string_pretty(&*messages)?;
        drop(messages);

        tokio::fs::write(&path, json).await?;

        Ok(CommandOutput::Text(format!(
            "Exported to {}",
            path.display()
        )))
    }
}

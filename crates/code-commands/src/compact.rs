//! `/compact` — compact the conversation with an optional custom instruction.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct CompactCommand;

#[async_trait]
impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self) -> &str {
        "Compact the conversation context, optionally with a custom instruction."
    }

    fn usage(&self) -> Option<&str> {
        Some("/compact [custom instruction]")
    }

    async fn execute(&self, args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let custom_instruction = if args.trim().is_empty() {
            None
        } else {
            Some(args.trim().to_owned())
        };
        Ok(CommandOutput::Compact { custom_instruction })
    }
}

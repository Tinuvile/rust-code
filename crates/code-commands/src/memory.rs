//! `/memory` — list loaded memory entries.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct MemoryCommand;

#[async_trait]
impl Command for MemoryCommand {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "List loaded memory entries."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        if ctx.memory_entries.is_empty() {
            return Ok(CommandOutput::Text("No memory entries loaded.".to_owned()));
        }

        let mut rows = String::from("| # | Label |\n|---|-------|\n");
        for (i, entry) in ctx.memory_entries.iter().enumerate() {
            rows.push_str(&format!("| {} | {} |\n", i + 1, entry.label()));
        }

        Ok(CommandOutput::Markdown(rows))
    }
}

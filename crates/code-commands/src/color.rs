//! `/color` — toggle ANSI color output.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ColorCommand;

#[async_trait]
impl Command for ColorCommand {
    fn name(&self) -> &str {
        "color"
    }

    fn description(&self) -> &str {
        "Toggle ANSI color output on or off."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.color_enabled = !ctx.color_enabled;
        let state = if ctx.color_enabled { "enabled" } else { "disabled" };
        Ok(CommandOutput::Text(format!("Color output {state}.")))
    }
}

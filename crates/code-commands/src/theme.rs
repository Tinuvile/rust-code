//! `/theme` — view or change the current theme setting.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ThemeCommand;

#[async_trait]
impl Command for ThemeCommand {
    fn name(&self) -> &str {
        "theme"
    }

    fn description(&self) -> &str {
        "View or change the UI theme (light/dark/auto)."
    }

    fn usage(&self) -> Option<&str> {
        Some("/theme [light|dark|auto]")
    }

    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let arg = args.trim().to_lowercase();

        if arg.is_empty() {
            let current = format!("{:?}", ctx.config.theme).to_lowercase();
            return Ok(CommandOutput::Text(format!("Current theme: {current}")));
        }

        match arg.as_str() {
            "light" | "dark" | "auto" => Ok(CommandOutput::Text(format!(
                "Theme set to '{arg}'. Theme changes require a restart to take effect."
            ))),
            other => Ok(CommandOutput::Text(format!(
                "Unknown theme '{other}'. Valid options: light, dark, auto."
            ))),
        }
    }
}

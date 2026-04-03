//! `/config` — view or set configuration values.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct ConfigCommand;

#[async_trait]
impl Command for ConfigCommand {
    fn name(&self) -> &str {
        "config"
    }

    fn description(&self) -> &str {
        "View or set configuration values."
    }

    fn usage(&self) -> Option<&str> {
        Some("/config [key value]")
    }

    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if !args.trim().is_empty() {
            // Stub: setting a key is not yet implemented.
            return Ok(CommandOutput::Text(format!(
                "Unknown key: {}. Config changes require a restart to take effect.",
                args.trim()
            )));
        }

        let theme = &ctx.config.theme;
        let verbose = ctx.config.verbose;
        let auto_compact = ctx.config.auto_compact_enabled;

        let md = format!(
            "| Key | Value |\n\
             |-----|-------|\n\
             | theme | {theme:?} |\n\
             | verbose | {verbose} |\n\
             | auto_compact_enabled | {auto_compact} |"
        );
        Ok(CommandOutput::Markdown(md))
    }
}

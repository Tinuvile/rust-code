//! `/help` — display help for available commands.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

#[derive(Default)]
pub struct HelpCommand;

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn aliases(&self) -> &[&str] {
        &["?"]
    }

    fn description(&self) -> &str {
        "Show help for available commands."
    }

    fn usage(&self) -> Option<&str> {
        Some("/help [command]")
    }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let md = "## Claude Code — Slash Commands\n\n\
                  Type `/` followed by a command name. Common commands:\n\n\
                  | Command | Description |\n\
                  |---------|-------------|\n\
                  | /clear | Clear conversation history |\n\
                  | /compact | Compact conversation context |\n\
                  | /status | Show session status |\n\
                  | /config | View configuration |\n\
                  | /memory | List memory entries |\n\
                  | /doctor | Run environment diagnostics |\n\
                  | /init | Create CLAUDE.md in current directory |\n\
                  | /exit | Exit the CLI |\n\
                  | /session | Show session info |\n\
                  | /export | Export conversation to JSON |\n\
                  | /cost | Show token usage and estimated cost |\n\
                  | /usage | Show per-turn token usage |\n\
                  | /commit | Commit staged changes |\n\
                  | /diff | Show git diff |\n\
                  | /review | Ask model to review git diff |\n\
                  | /mcp | List MCP servers |\n\
                  | /theme | View or change theme |\n\
                  | /keybindings | Show keyboard shortcuts |\n\
                  | /color | Toggle color output |\n\
                  | /rename | Rename current session |\n\
                  | /help | Show this help |"
            .to_owned();
        Ok(CommandOutput::Markdown(md))
    }
}

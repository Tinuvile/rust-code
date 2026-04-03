//! `/mcp` — list configured MCP servers.

use async_trait::async_trait;

use code_config::settings::McpServerConfig;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct McpCommand;

#[async_trait]
impl Command for McpCommand {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self) -> &str {
        "List configured MCP servers."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let servers = match &ctx.settings.mcp_servers {
            Some(s) if !s.is_empty() => s,
            _ => {
                return Ok(CommandOutput::Text(
                    "No MCP servers configured.".to_owned(),
                ))
            }
        };

        let mut rows = String::from(
            "| Server | Type |\n\
             |--------|------|\n",
        );

        for (name, cfg) in servers {
            let kind = match cfg {
                McpServerConfig::Stdio(_) => "stdio",
                McpServerConfig::Http(_) => "http",
            };
            rows.push_str(&format!("| {name} | {kind} |\n"));
        }

        Ok(CommandOutput::Markdown(rows))
    }
}

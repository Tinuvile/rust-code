//! `/init` — create a CLAUDE.md file in the current working directory.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct InitCommand;

#[async_trait]
impl Command for InitCommand {
    fn name(&self) -> &str {
        "init"
    }

    fn description(&self) -> &str {
        "Create a CLAUDE.md file in the current working directory."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let path = ctx.cwd.join("CLAUDE.md");

        if path.exists() {
            return Ok(CommandOutput::Text("CLAUDE.md already exists.".to_owned()));
        }

        let contents = "# Claude Memory\n\nAdd project-specific instructions and context here.\n";
        tokio::fs::write(&path, contents).await?;

        let cwd = ctx.cwd.display().to_string();
        Ok(CommandOutput::Text(format!("Created CLAUDE.md in {cwd}")))
    }
}

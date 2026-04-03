//! `/diff` — show git diff output.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct DiffCommand;

#[async_trait]
impl Command for DiffCommand {
    fn name(&self) -> &str {
        "diff"
    }

    fn description(&self) -> &str {
        "Show git diff output."
    }

    fn usage(&self) -> Option<&str> {
        Some("/diff [extra git args]")
    }

    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("diff").current_dir(&ctx.cwd);

        if !args.trim().is_empty() {
            // Split extra args by whitespace and pass them along.
            for part in args.split_whitespace() {
                cmd.arg(part);
            }
        }

        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !stderr.is_empty() && stdout.is_empty() {
            return Ok(CommandOutput::Text(stderr.trim().to_owned()));
        }

        let md = format!("```diff\n{}\n```", stdout.trim_end());
        Ok(CommandOutput::Markdown(md))
    }
}

//! `/commit` — commit staged changes with the given message.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct CommitCommand;

#[async_trait]
impl Command for CommitCommand {
    fn name(&self) -> &str {
        "commit"
    }

    fn description(&self) -> &str {
        "Commit staged git changes with the given message."
    }

    fn usage(&self) -> Option<&str> {
        Some("/commit <message>")
    }

    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let msg = args.trim();
        if msg.is_empty() {
            return Ok(CommandOutput::Query(
                "Please provide a commit message:".to_owned(),
            ));
        }

        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(&ctx.cwd)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let combined = if stderr.is_empty() {
            stdout
        } else if stdout.is_empty() {
            stderr
        } else {
            format!("{stdout}\n{stderr}")
        };

        Ok(CommandOutput::Text(combined.trim().to_owned()))
    }
}

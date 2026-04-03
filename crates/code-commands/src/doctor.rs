//! `/doctor` — run environment diagnostics.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct DoctorCommand;

#[async_trait]
impl Command for DoctorCommand {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self) -> &str {
        "Run environment diagnostics."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let api_key_ok = ctx.config.primary_api_key.is_some();
        let api_key_status = if api_key_ok { "✓ API key configured" } else { "✗ No API key found" };

        // Check for git in PATH by trying to run `git --version`.
        let git_ok = tokio::process::Command::new("git")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        let git_status = if git_ok { "✓ git found in PATH" } else { "✗ git not found in PATH" };

        let md = format!(
            "## Environment Diagnostics\n\n\
             | Check | Status |\n\
             |-------|--------|\n\
             | API Key | {api_key_status} |\n\
             | Git | {git_status} |"
        );
        Ok(CommandOutput::Markdown(md))
    }
}

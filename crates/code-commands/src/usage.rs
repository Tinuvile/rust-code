//! `/usage` — show per-turn and total token usage.

use async_trait::async_trait;

use code_types::message::Message;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct UsageCommand;

#[async_trait]
impl Command for UsageCommand {
    fn name(&self) -> &str {
        "usage"
    }

    fn description(&self) -> &str {
        "Show token usage summary for this session."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let messages = ctx.conversation.read().await;

        let mut total_input: u64 = 0;
        let mut total_output: u64 = 0;
        let mut turn = 0usize;

        let mut rows = String::from(
            "| Turn | Input Tokens | Output Tokens |\n\
             |------|-------------|---------------|\n",
        );

        for msg in messages.iter() {
            if let Message::Assistant(a) = msg {
                turn += 1;
                let inp = a.usage.input_tokens;
                let out = a.usage.output_tokens;
                total_input += inp as u64;
                total_output += out as u64;
                rows.push_str(&format!("| {turn} | {inp} | {out} |\n"));
            }
        }
        drop(messages);

        if turn == 0 {
            return Ok(CommandOutput::Text("No assistant turns recorded yet.".to_owned()));
        }

        rows.push_str(&format!(
            "| **Total** | **{total_input}** | **{total_output}** |"
        ));

        Ok(CommandOutput::Markdown(rows))
    }
}

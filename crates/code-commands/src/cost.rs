//! `/cost` — show estimated token usage and cost for this session.

use async_trait::async_trait;

use code_types::message::Message;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

/// Cost per token (rough estimates).
const INPUT_COST_PER_TOKEN: f64 = 0.000003;
const OUTPUT_COST_PER_TOKEN: f64 = 0.000015;

pub struct CostCommand;

#[async_trait]
impl Command for CostCommand {
    fn name(&self) -> &str {
        "cost"
    }

    fn description(&self) -> &str {
        "Show estimated token usage and cost for this session."
    }

    async fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let messages = ctx.conversation.read().await;

        let mut total_input: u64 = 0;
        let mut total_output: u64 = 0;

        for msg in messages.iter() {
            if let Message::Assistant(a) = msg {
                total_input += a.usage.input_tokens as u64;
                total_output += a.usage.output_tokens as u64;
            }
        }
        drop(messages);

        let input_cost = total_input as f64 * INPUT_COST_PER_TOKEN;
        let output_cost = total_output as f64 * OUTPUT_COST_PER_TOKEN;
        let total_cost = input_cost + output_cost;

        let md = format!(
            "## Token Usage & Cost\n\n\
             | Metric | Value |\n\
             |--------|-------|\n\
             | Input tokens | {total_input} |\n\
             | Output tokens | {total_output} |\n\
             | Input cost | ${input_cost:.6} |\n\
             | Output cost | ${output_cost:.6} |\n\
             | **Total cost** | **${total_cost:.6}** |"
        );
        Ok(CommandOutput::Markdown(md))
    }
}

//! `/keybindings` — show available keyboard shortcuts.

use async_trait::async_trait;

use crate::{Command, CommandContext, CommandOutput, CommandResult};

pub struct KeybindingsCommand;

#[async_trait]
impl Command for KeybindingsCommand {
    fn name(&self) -> &str {
        "keybindings"
    }

    fn aliases(&self) -> &[&str] {
        &["keys"]
    }

    fn description(&self) -> &str {
        "Show available keyboard shortcuts."
    }

    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let md = "## Keyboard Shortcuts\n\n\
                  | Key | Action |\n\
                  |-----|--------|\n\
                  | Ctrl+C | Interrupt |\n\
                  | Ctrl+D | Exit |\n\
                  | Up/Down | History navigation |\n\
                  | Tab | Autocomplete |"
            .to_owned();
        Ok(CommandOutput::Markdown(md))
    }
}

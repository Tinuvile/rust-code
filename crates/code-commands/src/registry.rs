//! Command registry: lookup by name / alias and bulk registration.
//!
//! Ref: src/commands.ts (commandRegistry)

use std::collections::HashMap;
use std::sync::Arc;

use crate::Command;

/// Registry of all available slash commands.
pub struct CommandRegistry {
    /// Maps command name and all aliases → `Arc<dyn Command>`.
    by_name: HashMap<String, Arc<dyn Command>>,
    /// Insertion-order list (for `/help` listing).
    ordered: Vec<Arc<dyn Command>>,
}

impl CommandRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            by_name: HashMap::new(),
            ordered: Vec::new(),
        }
    }

    /// Register a command under its primary name and all aliases.
    pub fn register(&mut self, cmd: impl Command + 'static) {
        let cmd = Arc::new(cmd) as Arc<dyn Command>;
        self.by_name.insert(cmd.name().to_owned(), Arc::clone(&cmd));
        for alias in cmd.aliases() {
            self.by_name.insert(alias.to_string(), Arc::clone(&cmd));
        }
        self.ordered.push(cmd);
    }

    /// Look up a command by name or alias.
    pub fn get(&self, name: &str) -> Option<&dyn Command> {
        self.by_name.get(name).map(|c| c.as_ref())
    }

    /// All registered commands in insertion order (no duplicates from aliases).
    pub fn all(&self) -> &[Arc<dyn Command>] {
        &self.ordered
    }

    /// Build a registry pre-populated with all standard commands.
    pub fn with_all_commands() -> Self {
        use crate::*;

        let mut r = Self::new();
        r.register(version::VersionCommand);
        r.register(clear::ClearCommand);
        r.register(compact::CompactCommand);
        r.register(status::StatusCommand);
        r.register(config::ConfigCommand);
        r.register(memory::MemoryCommand);
        r.register(doctor::DoctorCommand);
        r.register(login::LoginCommand);
        r.register(logout::LogoutCommand);
        r.register(init::InitCommand);
        r.register(exit::ExitCommand);
        r.register(resume::ResumeCommand);
        r.register(session::SessionCommand);
        r.register(export::ExportCommand);
        r.register(share::ShareCommand);
        r.register(cost::CostCommand);
        r.register(usage::UsageCommand);
        r.register(commit::CommitCommand);
        r.register(diff::DiffCommand);
        r.register(review::ReviewCommand);
        r.register(mcp::McpCommand);
        r.register(theme::ThemeCommand);
        r.register(keybindings::KeybindingsCommand);
        r.register(color::ColorCommand);
        r.register(tasks::TasksCommand);
        r.register(skills::SkillsCommand);
        r.register(rename::RenameCommand);
        // Help must be last so it can reference the full registry.
        // For now register a placeholder; callers that need full help output
        // should build HelpCommand with Arc<CommandRegistry>.
        r.register(help::HelpCommand::default());
        r
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, CommandContext, CommandOutput, CommandResult};
    use async_trait::async_trait;

    struct Ping;

    #[async_trait]
    impl Command for Ping {
        fn name(&self) -> &str { "ping" }
        fn aliases(&self) -> &[&str] { &["p"] }
        fn description(&self) -> &str { "pong" }
        async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
            Ok(CommandOutput::Text("pong".to_owned()))
        }
    }

    #[test]
    fn register_and_lookup_by_name() {
        let mut r = CommandRegistry::new();
        r.register(Ping);
        assert!(r.get("ping").is_some());
    }

    #[test]
    fn lookup_by_alias() {
        let mut r = CommandRegistry::new();
        r.register(Ping);
        assert!(r.get("p").is_some());
    }

    #[test]
    fn unknown_name_returns_none() {
        let r = CommandRegistry::new();
        assert!(r.get("unknown").is_none());
    }

    #[test]
    fn all_returns_ordered_list() {
        let mut r = CommandRegistry::new();
        r.register(Ping);
        assert_eq!(r.all().len(), 1);
    }
}

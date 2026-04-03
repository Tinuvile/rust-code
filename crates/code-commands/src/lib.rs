//! Slash command system: 60+ commands.
//!
//! Ref: src/commands.ts, src/commands/ (87 subdirectories)

pub mod registry;
pub mod context;

// Core commands
pub mod compact;
pub mod help;
pub mod clear;
pub mod config;
pub mod memory;
pub mod doctor;
pub mod login;
pub mod logout;
pub mod version;
pub mod status;
pub mod init;
pub mod exit;

// Session commands
pub mod resume;
pub mod session;
pub mod export;
pub mod share;
pub mod cost;
pub mod usage;

// Git commands
pub mod commit;
pub mod diff;
pub mod review;

// MCP commands
pub mod mcp;

// UI commands
pub mod theme;
pub mod keybindings;
pub mod color;

// Task / skill commands
pub mod tasks;
pub mod skills;

// Rename
pub mod rename;

// ── Core types ────────────────────────────────────────────────────────────────

use async_trait::async_trait;

pub use context::CommandContext;
pub use registry::CommandRegistry;

/// The output produced by a command execution.
pub enum CommandOutput {
    /// Plain text to display as-is.
    Text(String),
    /// Markdown-formatted text.
    Markdown(String),
    /// No visible output.
    None,
    /// A query string to send to the model (e.g. /review).
    Query(String),
    /// Trigger a context compaction with an optional custom instruction.
    Compact { custom_instruction: Option<String> },
    /// Exit the CLI.
    Exit,
}

pub type CommandResult = anyhow::Result<CommandOutput>;

/// Trait implemented by all slash commands.
#[async_trait]
pub trait Command: Send + Sync {
    /// Primary name (without leading `/`).
    fn name(&self) -> &str;

    /// Optional aliases (without leading `/`).
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// One-line description shown in `/help`.
    fn description(&self) -> &str;

    /// Optional usage string shown in `/help <command>`.
    fn usage(&self) -> Option<&str> {
        None
    }

    /// Execute the command.
    ///
    /// `args` is the trimmed remainder of the slash command line after the
    /// command name (empty string if no arguments were given).
    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult;
}

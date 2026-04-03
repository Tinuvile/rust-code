//! Execution context passed to every command.
//!
//! Ref: src/commands.ts (CommandContext)

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use code_config::global::GlobalConfig;
use code_config::settings::SettingsJson;
use code_memory::MemoryEntry;
use code_types::ids::SessionId;
use code_types::message::Message;

/// All state a command needs to read or modify during execution.
pub struct CommandContext {
    /// The current session identifier.
    pub session_id: SessionId,
    /// Current working directory.
    pub cwd: PathBuf,
    /// User-level global configuration.
    pub config: Arc<GlobalConfig>,
    /// Merged settings (project + user layers).
    pub settings: Arc<SettingsJson>,
    /// Loaded memory entries for the current session.
    pub memory_entries: Vec<MemoryEntry>,
    /// The live conversation message list (shared with the query engine).
    pub conversation: Arc<RwLock<Vec<Message>>>,
    /// Whether ANSI colour is enabled in the current terminal.
    pub color_enabled: bool,
}

impl CommandContext {
    /// Convenience constructor.
    pub fn new(
        session_id: SessionId,
        cwd: impl Into<PathBuf>,
        config: Arc<GlobalConfig>,
        settings: Arc<SettingsJson>,
        memory_entries: Vec<MemoryEntry>,
        conversation: Arc<RwLock<Vec<Message>>>,
        color_enabled: bool,
    ) -> Self {
        Self {
            session_id,
            cwd: cwd.into(),
            config,
            settings,
            memory_entries,
            conversation,
            color_enabled,
        }
    }
}

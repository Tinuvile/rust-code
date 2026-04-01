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

// Re-export trait (uncomment when CommandRegistry is implemented)
// pub use registry::CommandRegistry;

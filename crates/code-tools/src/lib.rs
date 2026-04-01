//! Tool trait definition and all tool implementations.
//!
//! Ref: src/Tool.ts, src/tools.ts, src/services/tools/toolOrchestration.ts

pub mod registry;
pub mod orchestration;
pub mod execution;
pub mod result_storage;
pub mod progress;

// Tool implementations (Tier 1 — core)
pub mod bash;
pub mod file_read;
pub mod file_write;
pub mod file_edit;
pub mod grep;
pub mod glob;

// Tool implementations (Tier 2 — important)
pub mod web_fetch;
pub mod web_search;
pub mod ask_user;
pub mod todo_write;
pub mod notebook_edit;

// Tool implementations (Tier 3 — specialized)
pub mod mcp_tool;
pub mod task_tools;
pub mod plan_mode;
pub mod worktree;
pub mod config_tool;
pub mod tool_search;
pub mod lsp;
pub mod powershell;
pub mod synthetic_output;
pub mod brief;

// Feature-gated tools
#[cfg(any(feature = "proactive", feature = "kairos"))]
pub mod sleep;

#[cfg(feature = "agent_triggers")]
pub mod cron;

#[cfg(feature = "agent_triggers_remote")]
pub mod remote_trigger;

#[cfg(feature = "coordinator_mode")]
pub mod coordinator_tools;

#[cfg(feature = "monitor_tool")]
pub mod monitor;

// Re-export the core trait for ergonomic use (uncomment when ToolRegistry is implemented)
// pub use registry::ToolRegistry;

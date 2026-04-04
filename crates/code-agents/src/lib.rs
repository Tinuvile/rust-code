//! Agent and subagent system: built-in agents, fork/resume, coordinator.
//!
//! Ref: src/tools/AgentTool/runAgent.ts, src/tools/AgentTool/forkSubagent.ts,
//!      src/tools/AgentTool/built-in/, src/coordinator/coordinatorMode.ts

pub mod definition;
pub mod registry;
pub mod loader;
pub mod runner;
pub mod fork;
pub mod resume;
pub mod color;
pub mod memory_ctx;

// Built-in agent definitions
pub mod builtin;

#[cfg(feature = "coordinator_mode")]
pub mod coordinator;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use definition::{AgentDefinition, AgentSource};
pub use registry::AgentRegistry;
pub use runner::{run_agent, AgentRunResult, RunOptions};
pub use fork::{fork_subagent, ForkOptions};
pub use resume::{AgentState, default_state_dir};
pub use color::AgentColorManager;
pub use builtin::all_builtin_agents;

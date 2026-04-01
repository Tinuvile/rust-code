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

//! Background tasks: shell tasks, agent tasks, todo management.
//!
//! Ref: src/tasks/LocalShellTask/, src/tasks/LocalAgentTask/,
//!      src/tools/TaskOutputTool/, src/tools/TodoWriteTool/

pub mod task;
pub mod shell_task;
pub mod agent_task;
pub mod output;
pub mod todo;
pub mod store;

#[cfg(any(feature = "kairos", feature = "agent_triggers"))]
pub mod scheduled;

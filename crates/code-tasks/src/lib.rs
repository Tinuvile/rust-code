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

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use task::{TaskId, TaskKind, TaskRecord, TaskStatus};
pub use store::{SharedTaskStore, TaskStore};
pub use output::TaskOutput;
pub use todo::{TodoItem, TodoList, TodoPriority, TodoStatus, todos_path};
pub use shell_task::spawn_shell_task;
pub use agent_task::{spawn_agent_task, AgentTaskOptions};

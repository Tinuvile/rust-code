//! Core task types: status, kind, TaskId, TaskRecord.
//!
//! Ref: src/tasks/types.ts, src/Task.ts

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── TaskStatus ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

// ── TaskKind ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Shell command task (`local_bash` in TypeScript).
    Shell,
    /// Sub-agent task.
    Agent,
}

// ── TaskId ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── TaskRecord ────────────────────────────────────────────────────────────────

/// A persisted task record — fully serializable to/from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: TaskId,
    pub kind: TaskKind,
    pub status: TaskStatus,
    /// Human-readable label shown in the status bar / task list.
    pub label: String,
    /// Shell command string (shell tasks only).
    pub command: Option<String>,
    /// Agent type identifier (agent tasks only).
    pub agent_type: Option<String>,
    /// Process exit code (shell tasks, set after completion).
    pub exit_code: Option<i32>,
    /// Path to the output log file.
    pub log_path: Option<std::path::PathBuf>,
    /// Whether this task was explicitly moved to the background.
    pub is_backgrounded: bool,
    /// Unix timestamp (seconds) when the task was created.
    pub created_at: u64,
    /// Unix timestamp (seconds) when the task reached a terminal state.
    pub finished_at: Option<u64>,
    /// Error description on failure.
    pub error: Option<String>,
}

impl TaskRecord {
    pub fn new_shell(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            kind: TaskKind::Shell,
            status: TaskStatus::Pending,
            label: label.into(),
            command: Some(command.into()),
            agent_type: None,
            exit_code: None,
            log_path: None,
            is_backgrounded: false,
            created_at: unix_now(),
            finished_at: None,
            error: None,
        }
    }

    pub fn new_agent(label: impl Into<String>, agent_type: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            kind: TaskKind::Agent,
            status: TaskStatus::Pending,
            label: label.into(),
            command: None,
            agent_type: Some(agent_type.into()),
            exit_code: None,
            log_path: None,
            is_backgrounded: true,
            created_at: unix_now(),
            finished_at: None,
            error: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    pub fn mark_completed(&mut self, exit_code: Option<i32>) {
        self.status = TaskStatus::Completed;
        self.exit_code = exit_code;
        self.finished_at = Some(unix_now());
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = TaskStatus::Failed;
        self.error = Some(error.into());
        self.finished_at = Some(unix_now());
    }

    pub fn mark_cancelled(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.finished_at = Some(unix_now());
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

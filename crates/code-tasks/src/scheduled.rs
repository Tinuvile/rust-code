//! Scheduled / cron task support.
//!
//! Only compiled when `kairos` or `agent_triggers` feature is enabled.
//!
//! Ref: src/tools/ScheduleCronTool/

use serde::{Deserialize, Serialize};

use crate::task::TaskId;

// ── CronSchedule ─────────────────────────────────────────────────────────────

/// A cron expression string (e.g. `"0 9 * * 1-5"` for weekday mornings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule(pub String);

impl CronSchedule {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── ScheduledTask ─────────────────────────────────────────────────────────────

/// A persistent scheduled task specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// Unique identifier for this scheduled task.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Cron expression controlling when the task fires.
    pub schedule: CronSchedule,
    /// Command or prompt to execute when the cron fires.
    pub command: String,
    /// Whether this is a shell command (`true`) or agent prompt (`false`).
    pub is_shell: bool,
    /// Agent type (agent tasks only).
    pub agent_type: Option<String>,
    /// Whether the schedule is currently active.
    pub enabled: bool,
    /// Unix timestamp (s) of the last invocation.
    pub last_run_at: Option<u64>,
    /// Task id of the most recent execution.
    pub last_task_id: Option<TaskId>,
}

impl ScheduledTask {
    pub fn new_shell(label: impl Into<String>, schedule: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
            schedule: CronSchedule(schedule.into()),
            command: command.into(),
            is_shell: true,
            agent_type: None,
            enabled: true,
            last_run_at: None,
            last_task_id: None,
        }
    }

    pub fn new_agent(
        label: impl Into<String>,
        schedule: impl Into<String>,
        prompt: impl Into<String>,
        agent_type: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
            schedule: CronSchedule(schedule.into()),
            command: prompt.into(),
            is_shell: false,
            agent_type: Some(agent_type.into()),
            enabled: true,
            last_run_at: None,
            last_task_id: None,
        }
    }
}

// ── ScheduledTaskStore ────────────────────────────────────────────────────────

/// Persistent store for scheduled tasks.
///
/// Persisted as `~/.claude/scheduled-tasks.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScheduledTaskStore {
    pub tasks: Vec<ScheduledTask>,
}

impl ScheduledTaskStore {
    pub async fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        match tokio::fs::read_to_string(path).await {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    pub fn add(&mut self, task: ScheduledTask) -> &ScheduledTask {
        self.tasks.push(task);
        self.tasks.last().unwrap()
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        self.tasks.len() < before
    }

    pub fn find(&self, id: &str) -> Option<&ScheduledTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn enabled(&self) -> Vec<&ScheduledTask> {
        self.tasks.iter().filter(|t| t.enabled).collect()
    }
}

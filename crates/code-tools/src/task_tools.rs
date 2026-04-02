//! Background-task tools — TaskCreate / TaskOutput / TaskStop.
//!
//! These tools let the model spawn and manage background sub-agents.
//! Full sub-agent execution requires the Phase 9 TUI and agent loop.
//!
//! In Phase 5 tasks are tracked as JSON files under
//! `{session_dir}/tasks/{task_id}.json` so that the data structures are
//! exercised even without a live agent runner.
//!
//! Ref: src/tools/TaskTool/TaskTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Task record ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub prompt: String,
    pub status: TaskStatus,
    pub created_at: u64, // Unix seconds
    pub output: Option<String>,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn task_path(session_dir: &Path, task_id: &str) -> std::path::PathBuf {
    session_dir.join("tasks").join(format!("{task_id}.json"))
}

async fn write_task(session_dir: &Path, record: &TaskRecord) -> std::io::Result<()> {
    let dir = session_dir.join("tasks");
    tokio::fs::create_dir_all(&dir).await?;
    let json = serde_json::to_string_pretty(record).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;
    tokio::fs::write(task_path(session_dir, &record.id), json).await
}

async fn read_task(session_dir: &Path, task_id: &str) -> Option<TaskRecord> {
    let path = task_path(session_dir, task_id);
    let raw = tokio::fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&raw).ok()
}

// ── TaskCreate ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TaskCreateInput {
    prompt: String,
    task_id: Option<String>,
}

pub struct TaskCreateTool;

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str { "Task" }

    fn description(&self) -> &str {
        "Launch a background agent to handle a sub-task. \
        The agent runs asynchronously; use TaskOutput to retrieve results and \
        TaskStop to cancel it. \
        (Note: background agent execution requires an interactive session.)"
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task description / instructions for the sub-agent"
                },
                "task_id": {
                    "type": "string",
                    "description": "Optional stable ID for this task (auto-generated if omitted)"
                }
            },
            "required": ["prompt"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("prompt is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("prompt").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: false,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: TaskCreateInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let task_id = parsed
            .task_id
            .unwrap_or_else(|| format!("task-{}", tool_use_id));

        let record = TaskRecord {
            id: task_id.clone(),
            prompt: parsed.prompt,
            status: TaskStatus::Pending,
            created_at: unix_now(),
            output: None,
        };

        if let Err(e) = write_task(&ctx.session_dir, &record).await {
            return error_result(tool_use_id, format!("Failed to persist task: {e}"));
        }

        // Phase 5: tasks are recorded but not actively executed.
        // Phase 9 will attach the agent runner that picks up Pending tasks.
        ok_result(
            tool_use_id,
            format!(
                "Task '{task_id}' created (status: pending). \
                Background execution requires an interactive session (Phase 9)."
            ),
        )
    }
}

// ── TaskOutput ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TaskOutputInput {
    task_id: String,
}

pub struct TaskOutputTool;

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str { "TaskOutput" }

    fn description(&self) -> &str {
        "Retrieve the current output / status of a background task started with Task."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "ID of the task to query (as returned by Task)"
                }
            },
            "required": ["task_id"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("task_id").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: TaskOutputInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        match read_task(&ctx.session_dir, &parsed.task_id).await {
            Some(record) => {
                let output = record.output.as_deref().unwrap_or("(no output yet)");
                let status = serde_json::to_string(&record.status).unwrap_or_default();
                ok_result(
                    tool_use_id,
                    format!("Task '{}' status: {status}\n{output}", parsed.task_id),
                )
            }
            None => error_result(
                tool_use_id,
                format!("Task '{}' not found.", parsed.task_id),
            ),
        }
    }
}

// ── TaskStop ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TaskStopInput {
    task_id: String,
}

pub struct TaskStopTool;

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str { "TaskStop" }

    fn description(&self) -> &str {
        "Cancel a running background task."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "ID of the task to stop"
                }
            },
            "required": ["task_id"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("task_id").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: false,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: TaskStopInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        match read_task(&ctx.session_dir, &parsed.task_id).await {
            Some(mut record) => {
                record.status = TaskStatus::Stopped;
                if let Err(e) = write_task(&ctx.session_dir, &record).await {
                    return error_result(tool_use_id, format!("Failed to update task: {e}"));
                }
                ok_result(
                    tool_use_id,
                    format!("Task '{}' stopped.", parsed.task_id),
                )
            }
            None => error_result(
                tool_use_id,
                format!("Task '{}' not found.", parsed.task_id),
            ),
        }
    }
}

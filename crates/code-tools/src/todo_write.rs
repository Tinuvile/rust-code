//! TodoWriteTool — manage the session todo list.
//!
//! The todo list is persisted as JSON at
//! `{session_dir}/todos.json`.  This tool replaces the entire list on each
//! write (idempotent, mirrors the TypeScript implementation).
//!
//! Ref: src/tools/TodoWriteTool/TodoWriteTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub priority: Option<String>, // "high" | "medium" | "low"
}

#[derive(Deserialize)]
struct TodoWriteInput {
    todos: Vec<TodoItem>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str { "TodoWrite" }

    fn description(&self) -> &str {
        "Use this tool to create and manage a structured task list for the current coding session. \
        This helps track progress, organize complex tasks, and demonstrate thoroughness.\n\n\
        Replaces the entire todo list on each call — always pass the complete list."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id":       { "type": "string" },
                            "content":  { "type": "string" },
                            "status":   { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                            "priority": { "type": "string", "enum": ["high", "medium", "low"] }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }
    fn is_concurrency_safe(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        if input.get("todos").is_none() {
            return ValidationResult::err("todos is required", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: None,
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
        let parsed: TodoWriteInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let todo_path = ctx.session_dir.join("todos.json");

        // Ensure parent directory exists.
        if let Some(parent) = todo_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return error_result(tool_use_id, format!("Cannot create session dir: {e}"));
            }
        }

        let json_str = match serde_json::to_string_pretty(&parsed.todos) {
            Ok(s) => s,
            Err(e) => return error_result(tool_use_id, format!("Serialization error: {e}")),
        };

        if let Err(e) = tokio::fs::write(&todo_path, &json_str).await {
            return error_result(tool_use_id, format!("Failed to write todos: {e}"));
        }

        let n = parsed.todos.len();
        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(format!(
                "Todos have been modified successfully. ({n} item{})",
                if n == 1 { "" } else { "s" }
            )),
            is_error: false,
            was_truncated: false,
        }
    }
}

//! BriefTool — surface a structured brief/summary to the coordinator.
//!
//! Used by sub-agents in coordinator mode to send a structured summary back
//! to the parent coordinator.  The brief is written to
//! `{session_dir}/brief.json` so the coordinator can read it.
//!
//! Ref: src/tools/BriefTool/BriefTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BriefInput {
    /// Summary text for the coordinator.
    summary: String,
    /// Optional structured result data.
    result: Option<Value>,
    /// Whether the task completed successfully.
    success: Option<bool>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct BriefTool;

#[async_trait]
impl Tool for BriefTool {
    fn name(&self) -> &str { "Brief" }

    fn description(&self) -> &str {
        "Send a structured brief/summary to the coordinator agent. \
        Call this when you have completed your assigned sub-task to report results."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "A concise summary of what was accomplished"
                },
                "result": {
                    "description": "Optional structured result data (any JSON value)"
                },
                "success": {
                    "type": "boolean",
                    "description": "Whether the task completed successfully (default: true)"
                }
            },
            "required": ["summary"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("summary").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("summary is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("summary").and_then(|v| v.as_str()),
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
        let parsed: BriefInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let brief = json!({
            "summary": parsed.summary,
            "result": parsed.result,
            "success": parsed.success.unwrap_or(true),
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });

        let brief_path = ctx.session_dir.join("brief.json");
        if let Some(parent) = brief_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return error_result(tool_use_id, format!("Cannot create session dir: {e}"));
            }
        }

        let json_str = serde_json::to_string_pretty(&brief).unwrap_or_default();
        if let Err(e) = tokio::fs::write(&brief_path, &json_str).await {
            return error_result(tool_use_id, format!("Failed to write brief: {e}"));
        }

        ok_result(tool_use_id, format!("Brief submitted: {}", parsed.summary))
    }
}

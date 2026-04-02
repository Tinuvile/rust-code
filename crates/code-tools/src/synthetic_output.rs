//! SyntheticOutputTool — inject synthetic tool results into the conversation.
//!
//! Used when resuming a session: the agent needs to replay tool results from a
//! previous run without actually re-executing the tools.  The tool simply
//! passes through the provided content as a `ToolResult`.
//!
//! This tool is always read-only (it never touches the file system or network).
//!
//! Ref: src/tools/SyntheticOutputTool/SyntheticOutputTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SyntheticOutputInput {
    /// The content to surface as the tool result.
    content: String,
    /// Whether this synthetic result represents an error.
    is_error: Option<bool>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct SyntheticOutputTool;

#[async_trait]
impl Tool for SyntheticOutputTool {
    fn name(&self) -> &str { "SyntheticOutput" }

    fn description(&self) -> &str {
        "Inject a pre-computed tool result into the conversation without executing any real tool. \
        Used internally when restoring a session from a previous run."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to surface as the tool result"
                },
                "is_error": {
                    "type": "boolean",
                    "description": "Whether this result represents an error (default: false)"
                }
            },
            "required": ["content"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("content").and_then(|v| v.as_str()) {
            Some(_) => ValidationResult::ok(),
            None => ValidationResult::err("content is required", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: None,
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: SyntheticOutputInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(parsed.content),
            is_error: parsed.is_error.unwrap_or(false),
            was_truncated: false,
        }
    }
}

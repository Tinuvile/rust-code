//! SleepTool — pause execution for a specified duration.
//!
//! Enabled by features `proactive` or `kairos`.
//! Used by autonomous/proactive agents that need to wait between actions.
//!
//! Ref: src/tools/SleepTool/SleepTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

const MAX_SLEEP_MS: u64 = 300_000; // 5 minutes

#[derive(Deserialize)]
struct SleepInput {
    /// Duration to sleep in milliseconds.
    duration_ms: u64,
}

pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str { "Sleep" }

    fn description(&self) -> &str {
        "Pause execution for the specified duration. \
        Maximum duration is 5 minutes (300 000 ms)."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "duration_ms": {
                    "type": "number",
                    "description": "Duration to sleep in milliseconds (max 300000)"
                }
            },
            "required": ["duration_ms"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("duration_ms").and_then(|v| v.as_u64()) {
            Some(0) => ValidationResult::err("duration_ms must be greater than 0", 1),
            Some(d) if d > MAX_SLEEP_MS => {
                ValidationResult::err(
                    format!("duration_ms exceeds maximum ({MAX_SLEEP_MS}ms)"),
                    1,
                )
            }
            Some(_) => ValidationResult::ok(),
            None => ValidationResult::err("duration_ms must be a positive number", 1),
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
        let parsed: SleepInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let ms = parsed.duration_ms.min(MAX_SLEEP_MS);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        ok_result(tool_use_id, format!("Slept for {ms}ms."))
    }
}

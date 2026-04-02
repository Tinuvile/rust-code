//! RemoteTriggerTool — register/deregister remote agent trigger endpoints.
//!
//! Enabled by feature `agent_triggers_remote`.
//! Allows external systems (webhooks, API calls) to trigger an agent session.
//! Trigger registrations are persisted to `{session_dir}/triggers/{id}.json`.
//! The HTTP listener is part of Phase 8+.
//!
//! Ref: src/tools/RemoteTriggerTool/RemoteTriggerTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Record ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerRecord {
    pub id: String,
    pub description: String,
    pub prompt_template: String,
    pub enabled: bool,
    pub created_at: u64,
}

fn trigger_path(session_dir: &Path, id: &str) -> std::path::PathBuf {
    session_dir.join("triggers").join(format!("{id}.json"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── RemoteTrigger ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RemoteTriggerInput {
    id: Option<String>,
    description: String,
    prompt_template: String,
}

pub struct RemoteTriggerTool;

#[async_trait]
impl Tool for RemoteTriggerTool {
    fn name(&self) -> &str { "RemoteTrigger" }

    fn description(&self) -> &str {
        "Register a remote trigger endpoint that allows external systems to invoke \
        this agent session via HTTP. Returns the trigger ID for use in webhook URLs."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique trigger ID (auto-generated if omitted)" },
                "description": { "type": "string", "description": "Human-readable description of this trigger" },
                "prompt_template": { "type": "string", "description": "Prompt template; use {payload} as a placeholder for the incoming request body" }
            },
            "required": ["description", "prompt_template"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        if input.get("description").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
            return ValidationResult::err("description is required", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("description").and_then(|v| v.as_str()),
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
        let parsed: RemoteTriggerInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let id = parsed.id.unwrap_or_else(|| format!("trigger-{}", unix_now()));

        let rec = TriggerRecord {
            id: id.clone(),
            description: parsed.description,
            prompt_template: parsed.prompt_template,
            enabled: true,
            created_at: unix_now(),
        };

        let dir = ctx.session_dir.join("triggers");
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            return error_result(tool_use_id, format!("Cannot create triggers dir: {e}"));
        }

        let json_str = serde_json::to_string_pretty(&rec).unwrap_or_default();
        if let Err(e) = tokio::fs::write(trigger_path(&ctx.session_dir, &id), json_str).await {
            return error_result(tool_use_id, format!("Failed to persist trigger: {e}"));
        }

        ok_result(
            tool_use_id,
            format!("Remote trigger '{id}' registered. (HTTP listener requires Phase 8+.)"),
        )
    }
}

//! Coordinator mode tools — spawn and manage sub-agents.
//!
//! Enabled by feature `coordinator_mode`.
//! These tools are used by a "coordinator" agent that orchestrates multiple
//! parallel sub-agents.  Full execution requires the Phase 9 agent loop;
//! this module provides the data structures and file-based stub implementation.
//!
//! Ref: src/tools/CoordinatorTool/CoordinatorTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Agent record ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub role: String,
    pub prompt: String,
    pub status: AgentStatus,
    pub output: Option<String>,
    pub created_at: u64,
}

fn agent_path(session_dir: &Path, id: &str) -> std::path::PathBuf {
    session_dir.join("agents").join(format!("{id}.json"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn write_agent(session_dir: &Path, rec: &AgentRecord) -> std::io::Result<()> {
    let dir = session_dir.join("agents");
    tokio::fs::create_dir_all(&dir).await?;
    let json = serde_json::to_string_pretty(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    tokio::fs::write(agent_path(session_dir, &rec.id), json).await
}

async fn read_agent(session_dir: &Path, id: &str) -> Option<AgentRecord> {
    let raw = tokio::fs::read_to_string(agent_path(session_dir, id)).await.ok()?;
    serde_json::from_str(&raw).ok()
}

// ── SpawnAgent ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SpawnAgentInput {
    agent_id: Option<String>,
    role: String,
    prompt: String,
}

pub struct SpawnAgentTool;

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str { "SpawnAgent" }

    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a specific part of the task. \
        The sub-agent runs in its own context and reports back via the Brief tool. \
        (Requires Phase 9 agent loop for actual execution.)"
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Optional unique ID for this agent" },
                "role": { "type": "string", "description": "Short description of the agent's role" },
                "prompt": { "type": "string", "description": "Full instructions for the sub-agent" }
            },
            "required": ["role", "prompt"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        if input.get("prompt").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
            return ValidationResult::err("prompt is required", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("role").and_then(|v| v.as_str()),
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
        let parsed: SpawnAgentInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let id = parsed.agent_id.unwrap_or_else(|| format!("agent-{}", tool_use_id));
        let rec = AgentRecord {
            id: id.clone(),
            role: parsed.role.clone(),
            prompt: parsed.prompt,
            status: AgentStatus::Pending,
            output: None,
            created_at: unix_now(),
        };

        if let Err(e) = write_agent(&ctx.session_dir, &rec).await {
            return error_result(tool_use_id, format!("Failed to persist agent record: {e}"));
        }

        ok_result(
            tool_use_id,
            format!(
                "Sub-agent '{id}' ({}) queued. \
                (Background execution requires Phase 9 agent loop.)",
                parsed.role
            ),
        )
    }
}

// ── GetAgentOutput ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GetAgentOutputInput {
    agent_id: String,
}

pub struct GetAgentOutputTool;

#[async_trait]
impl Tool for GetAgentOutputTool {
    fn name(&self) -> &str { "GetAgentOutput" }

    fn description(&self) -> &str {
        "Retrieve the output and status of a sub-agent spawned with SpawnAgent."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "ID of the agent to query" }
            },
            "required": ["agent_id"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("agent_id").and_then(|v| v.as_str()),
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
        let parsed: GetAgentOutputInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        match read_agent(&ctx.session_dir, &parsed.agent_id).await {
            Some(rec) => {
                let status = serde_json::to_string(&rec.status).unwrap_or_default();
                let output = rec.output.as_deref().unwrap_or("(no output yet)");
                ok_result(
                    tool_use_id,
                    format!("Agent '{}' [{}]\n{}", rec.id, status, output),
                )
            }
            None => error_result(
                tool_use_id,
                format!("Agent '{}' not found.", parsed.agent_id),
            ),
        }
    }
}

//! Cron tools — schedule recurring agent triggers.
//!
//! Enabled by feature `agent_triggers`.
//! Cron jobs are persisted under `{session_dir}/crons/{id}.json`.
//! The actual scheduler that fires them is part of Phase 8+.
//!
//! Ref: src/tools/CronTool/CronTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Record ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CronRecord {
    pub id: String,
    pub schedule: String, // cron expression, e.g. "0 * * * *"
    pub prompt: String,
    pub enabled: bool,
    pub created_at: u64,
}

fn cron_path(session_dir: &Path, id: &str) -> std::path::PathBuf {
    session_dir.join("crons").join(format!("{id}.json"))
}

async fn write_cron(session_dir: &Path, rec: &CronRecord) -> std::io::Result<()> {
    let dir = session_dir.join("crons");
    tokio::fs::create_dir_all(&dir).await?;
    let json = serde_json::to_string_pretty(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    tokio::fs::write(cron_path(session_dir, &rec.id), json).await
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── CronCreate ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CronCreateInput {
    id: Option<String>,
    schedule: String,
    prompt: String,
}

pub struct CronCreateTool;

#[async_trait]
impl Tool for CronCreateTool {
    fn name(&self) -> &str { "CronCreate" }

    fn description(&self) -> &str {
        "Schedule a recurring agent trigger using a cron expression. \
        The agent will be invoked with the given prompt on the schedule."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique identifier for this cron job (auto-generated if omitted)" },
                "schedule": { "type": "string", "description": "Cron expression (e.g. '0 * * * *' for hourly)" },
                "prompt": { "type": "string", "description": "Prompt to send to the agent on each trigger" }
            },
            "required": ["schedule", "prompt"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("schedule").and_then(|v| v.as_str()),
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
        let parsed: CronCreateInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let id = parsed.id.unwrap_or_else(|| format!("cron-{}", unix_now()));
        let rec = CronRecord {
            id: id.clone(),
            schedule: parsed.schedule,
            prompt: parsed.prompt,
            enabled: true,
            created_at: unix_now(),
        };

        if let Err(e) = write_cron(&ctx.session_dir, &rec).await {
            return error_result(tool_use_id, format!("Failed to persist cron: {e}"));
        }

        ok_result(tool_use_id, format!("Cron job '{id}' created (schedule: {}).", rec.schedule))
    }
}

// ── CronDelete ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CronDeleteInput {
    id: String,
}

pub struct CronDeleteTool;

#[async_trait]
impl Tool for CronDeleteTool {
    fn name(&self) -> &str { "CronDelete" }

    fn description(&self) -> &str { "Delete a scheduled cron job by ID." }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "ID of the cron job to delete" }
            },
            "required": ["id"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("id").and_then(|v| v.as_str()),
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
        let parsed: CronDeleteInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = cron_path(&ctx.session_dir, &parsed.id);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => ok_result(tool_use_id, format!("Cron job '{}' deleted.", parsed.id)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                error_result(tool_use_id, format!("Cron job '{}' not found.", parsed.id))
            }
            Err(e) => error_result(tool_use_id, format!("Failed to delete cron: {e}")),
        }
    }
}

// ── CronList ──────────────────────────────────────────────────────────────────

pub struct CronListTool;

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str { "CronList" }

    fn description(&self) -> &str { "List all scheduled cron jobs for the current session." }

    fn input_schema(&self) -> ToolInputSchema {
        json!({ "type": "object", "properties": {} })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

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
        _input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let dir = ctx.session_dir.join("crons");
        let mut crons: Vec<CronRecord> = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(raw) = tokio::fs::read_to_string(entry.path()).await {
                    if let Ok(rec) = serde_json::from_str::<CronRecord>(&raw) {
                        crons.push(rec);
                    }
                }
            }
        }

        crons.sort_by_key(|c| c.created_at);
        ok_result(
            tool_use_id,
            serde_json::to_string_pretty(&crons).unwrap_or_else(|_| "[]".into()),
        )
    }
}

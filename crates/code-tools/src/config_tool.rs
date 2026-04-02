//! ConfigTool — read and write Claude Code configuration.
//!
//! Configuration is persisted as JSON at `~/.claude/config.json` (global) or
//! `./.claude/config.json` (local).  This tool provides a key-based
//! read/write interface usable by the model without exposing raw file paths.
//!
//! Ref: src/tools/ConfigTool/ConfigTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConfigAction {
    Read,
    Write,
}

#[derive(Deserialize)]
struct ConfigInput {
    action: ConfigAction,
    key: String,
    value: Option<Value>,
    /// "local" (project .claude/) or "global" (~/.claude/).  Default: local.
    scope: Option<String>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct ConfigTool;

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str { "Config" }

    fn description(&self) -> &str {
        "Read or write Claude Code configuration settings. \
        Scope can be 'local' (project-level, stored in .claude/config.json) \
        or 'global' (user-level, stored in ~/.claude/config.json). \
        Default scope is 'local'."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "Whether to read or write the config key"
                },
                "key": {
                    "type": "string",
                    "description": "Dot-separated config key (e.g. 'model', 'theme.name')"
                },
                "value": {
                    "description": "New value to write (required for 'write' action)"
                },
                "scope": {
                    "type": "string",
                    "enum": ["local", "global"],
                    "description": "Configuration scope (default: local)"
                }
            },
            "required": ["action", "key"]
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        input
            .get("action")
            .and_then(|v| v.as_str())
            .map(|s| s == "read")
            .unwrap_or(false)
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if action == "write" && input.get("value").is_none() {
            return ValidationResult::err("value is required for write action", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("key").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: self.is_read_only(input),
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
        let parsed: ConfigInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let config_path = config_file_path(&parsed.scope.as_deref().unwrap_or("local"), &ctx.cwd);

        match parsed.action {
            ConfigAction::Read => read_key(&config_path, &parsed.key, tool_use_id).await,
            ConfigAction::Write => {
                write_key(
                    &config_path,
                    &parsed.key,
                    parsed.value.unwrap_or(Value::Null),
                    tool_use_id,
                )
                .await
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn config_file_path(scope: &str, cwd: &Path) -> std::path::PathBuf {
    if scope == "global" {
        dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude")
            .join("config.json")
    } else {
        cwd.join(".claude").join("config.json")
    }
}

async fn load_config(path: &Path) -> serde_json::Map<String, Value> {
    tokio::fs::read_to_string(path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

async fn save_config(path: &Path, map: &serde_json::Map<String, Value>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;
    tokio::fs::write(path, json).await
}

/// Get a value by dot-separated key path.
fn get_nested<'a>(map: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    let mut parts = key.splitn(2, '.');
    let head = parts.next()?;
    let val = map.get(head)?;
    match parts.next() {
        Some(rest) => val.as_object().and_then(|m| get_nested(m, rest)),
        None => Some(val),
    }
}

/// Set a value by dot-separated key path, creating intermediate objects.
fn set_nested(map: &mut serde_json::Map<String, Value>, key: &str, value: Value) {
    let mut parts = key.splitn(2, '.');
    let head = parts.next().unwrap_or(key);
    match parts.next() {
        Some(rest) => {
            let child = map
                .entry(head)
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Some(child_map) = child.as_object_mut() {
                set_nested(child_map, rest, value);
            }
        }
        None => {
            map.insert(head.to_owned(), value);
        }
    }
}

async fn read_key(path: &Path, key: &str, tool_use_id: &str) -> ToolResult {
    let config = load_config(path).await;
    match get_nested(&config, key) {
        Some(val) => ok_result(tool_use_id, serde_json::to_string_pretty(val).unwrap_or_default()),
        None => ok_result(tool_use_id, format!("Key '{key}' is not set.")),
    }
}

async fn write_key(path: &Path, key: &str, value: Value, tool_use_id: &str) -> ToolResult {
    let mut config = load_config(path).await;
    set_nested(&mut config, key, value);
    match save_config(path, &config).await {
        Ok(()) => ok_result(tool_use_id, format!("Config key '{key}' updated.")),
        Err(e) => error_result(tool_use_id, format!("Failed to save config: {e}")),
    }
}

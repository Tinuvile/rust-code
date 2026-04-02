//! FileWriteTool — write (create or overwrite) files on the filesystem.
//!
//! Ref: src/tools/FileWriteTool/FileWriteTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileWriteInput {
    file_path: String,
    file_contents: String,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Writes a file to the local filesystem.\n\n\
        Usage:\n\
        - This tool will overwrite the existing file if there is one at the provided path.\n\
        - If this is an existing file, you MUST use the Read tool first to read the file's \
          contents. This tool will fail if you did not read the file first.\n\
        - Prefer the Edit tool for modifying existing files — it only sends the diff. \
          Only use this tool to create new files or for complete rewrites."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "file_contents": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "file_contents"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ValidationResult::err("file_path is required", 1),
        };

        if file_path.is_empty() {
            return ValidationResult::err("file_path must not be empty", 1);
        }

        if file_path.starts_with("\\\\") {
            return ValidationResult::err("UNC paths are not supported", 1);
        }

        if input.get("file_contents").is_none() {
            return ValidationResult::err("file_contents is required", 1);
        }

        ValidationResult::ok()
    }

    fn permission_context<'a>(
        &'a self,
        input: &'a Value,
        cwd: &'a Path,
    ) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("file_path").and_then(|v| v.as_str()),
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
        let parsed: FileWriteInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = resolve_path(&parsed.file_path, &ctx.cwd);

        // Create parent directories if they don't exist.
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return error_result(
                    tool_use_id,
                    format!("Failed to create directory {}: {e}", parent.display()),
                );
            }
        }

        if let Err(e) = tokio::fs::write(&path, &parsed.file_contents).await {
            let msg = match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!("Permission denied writing to: {}", path.display())
                }
                _ => format!("Failed to write file {}: {e}", path.display()),
            };
            return error_result(tool_use_id, msg);
        }

        ok_result(
            tool_use_id,
            format!("The file {} has been saved successfully.", path.display()),
        )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_path(file_path: &str, cwd: &Path) -> std::path::PathBuf {
    let expanded = expand_tilde(file_path);
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(&expanded)
    }
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}

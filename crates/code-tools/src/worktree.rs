//! Git worktree tools — EnterWorktree / ExitWorktree.
//!
//! `EnterWorktree` creates a new git worktree in a temporary directory and
//! returns its path.  `ExitWorktree` removes the worktree.  These tools
//! allow the agent to work on isolated branches without touching the main tree.
//!
//! Ref: src/tools/WorktreeTool/WorktreeTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── EnterWorktree ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct EnterWorktreeInput {
    branch_name: String,
    base_branch: Option<String>,
}

pub struct EnterWorktreeTool;

#[async_trait]
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str { "EnterWorktree" }

    fn description(&self) -> &str {
        "Creates a new git worktree on a branch and returns its path. \
        Use this to work on code in isolation without affecting the main working tree. \
        The worktree is placed in a temporary directory."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "branch_name": {
                    "type": "string",
                    "description": "Name for the new branch (must not already exist)"
                },
                "base_branch": {
                    "type": "string",
                    "description": "Branch to base the new branch on (default: current HEAD)"
                }
            },
            "required": ["branch_name"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("branch_name").and_then(|v| v.as_str()) {
            Some(b) if !b.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("branch_name is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("branch_name").and_then(|v| v.as_str()),
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
        let parsed: EnterWorktreeInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        // Build a deterministic temporary path: /tmp/claude-worktree-{branch}
        let safe_branch = parsed
            .branch_name
            .replace(['/', '\\', ' '], "-");
        let worktree_dir = std::env::temp_dir()
            .join(format!("claude-worktree-{safe_branch}"));

        // git worktree add {path} [-b {branch}] [{base}]
        let mut args: Vec<String> = vec![
            "worktree".into(),
            "add".into(),
            worktree_dir.to_string_lossy().into_owned(),
            "-b".into(),
            parsed.branch_name.clone(),
        ];
        if let Some(base) = &parsed.base_branch {
            args.push(base.clone());
        }

        let output = Command::new("git")
            .args(&args)
            .current_dir(&ctx.cwd)
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                let path = worktree_dir.to_string_lossy().into_owned();
                ok_result(
                    tool_use_id,
                    format!("Worktree created at {path} on branch '{}'.", parsed.branch_name),
                )
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                error_result(tool_use_id, format!("git worktree add failed: {stderr}"))
            }
            Err(e) => error_result(tool_use_id, format!("Failed to run git: {e}")),
        }
    }
}

// ── ExitWorktree ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ExitWorktreeInput {
    worktree_path: String,
}

pub struct ExitWorktreeTool;

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str { "ExitWorktree" }

    fn description(&self) -> &str {
        "Removes a git worktree that was previously created with EnterWorktree. \
        This cleans up the temporary directory and unregisters the worktree from git."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "worktree_path": {
                    "type": "string",
                    "description": "Absolute path to the worktree to remove (as returned by EnterWorktree)"
                }
            },
            "required": ["worktree_path"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("worktree_path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("worktree_path is required", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("worktree_path").and_then(|v| v.as_str()),
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
        let parsed: ExitWorktreeInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let output = Command::new("git")
            .args(["worktree", "remove", "--force", &parsed.worktree_path])
            .current_dir(&ctx.cwd)
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                ok_result(
                    tool_use_id,
                    format!("Worktree '{}' removed.", parsed.worktree_path),
                )
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                error_result(
                    tool_use_id,
                    format!("git worktree remove failed: {stderr}"),
                )
            }
            Err(e) => error_result(tool_use_id, format!("Failed to run git: {e}")),
        }
    }
}

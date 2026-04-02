//! PowerShellTool — execute PowerShell commands.
//!
//! Uses `powershell.exe` on Windows and `pwsh` (PowerShell Core) on other
//! platforms.  Mirrors BashTool's streaming / timeout / background logic.
//!
//! Ref: src/tools/PowerShellTool/PowerShellTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_TIMEOUT_MS: u64 = 600_000;
const MAX_OUTPUT_CHARS: usize = 200_000;

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PowerShellInput {
    command: String,
    timeout_ms: Option<u64>,
    run_in_background: Option<bool>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct PowerShellTool;

#[async_trait]
impl Tool for PowerShellTool {
    fn name(&self) -> &str { "PowerShell" }

    fn description(&self) -> &str {
        "Execute a PowerShell command and return the combined stdout/stderr output. \
        Use for Windows-specific tasks, .NET API access, or when PowerShell syntax is preferred. \
        Timeout defaults to 120 seconds."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The PowerShell command to execute"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "Timeout in milliseconds (max 600000, default 120000)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "If true, detach the process and return immediately"
                }
            },
            "required": ["command"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("command").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("command is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("command").and_then(|v| v.as_str()),
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
        progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: PowerShellInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let timeout_ms = parsed
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        if parsed.run_in_background.unwrap_or(false) {
            return run_background(tool_use_id, &parsed.command, &ctx.cwd).await;
        }

        run_foreground(
            tool_use_id,
            &parsed.command,
            &ctx.cwd,
            timeout_ms,
            progress,
        )
        .await
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_command(command: &str, cwd: &Path) -> Command {
    let mut cmd;
    #[cfg(windows)]
    {
        cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    }
    #[cfg(not(windows))]
    {
        cmd = Command::new("pwsh");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    }
    cmd.current_dir(cwd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd
}

async fn run_background(tool_use_id: &str, command: &str, cwd: &Path) -> ToolResult {
    let mut cmd = make_command(command, cwd);
    // Detach stdout/stderr so the parent doesn't hold handles.
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id().unwrap_or(0);
            ok_result(
                tool_use_id,
                format!("Process started in background (PID {pid})."),
            )
        }
        Err(e) => error_result(tool_use_id, format!("Failed to spawn process: {e}")),
    }
}

async fn run_foreground(
    tool_use_id: &str,
    command: &str,
    cwd: &Path,
    timeout_ms: u64,
    progress: Option<&ProgressSender>,
) -> ToolResult {
    let mut cmd = make_command(command, cwd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let hint = if e.kind() == std::io::ErrorKind::NotFound {
                " (PowerShell not found — install PowerShell Core or run on Windows)"
            } else {
                ""
            };
            return error_result(
                tool_use_id,
                format!("Failed to spawn PowerShell: {e}{hint}"),
            );
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut output = String::new();
    let mut truncated = false;

    // Combine stdout and stderr via separate tasks.
    let tool_use_id_owned = tool_use_id.to_owned();
    let progress_clone = progress.cloned();

    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut buf = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(tx) = &progress_clone {
                let _ = tx.send(code_types::tool::ToolProgress {
                    tool_use_id: tool_use_id_owned.clone(),
                    tool_name: "PowerShell".to_owned(),
                    data: json!({ "line": line }),
                });
            }
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut buf = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let duration = std::time::Duration::from_millis(timeout_ms);
    let result = tokio::time::timeout(duration, child.wait()).await;

    let stdout_out = stdout_task.await.unwrap_or_default();
    let stderr_out = stderr_task.await.unwrap_or_default();

    let exit_status = match result {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return error_result(tool_use_id, format!("Process error: {e}")),
        Err(_) => {
            // Timeout — kill the process.
            let _ = child.kill().await;
            return error_result(
                tool_use_id,
                format!("Command timed out after {timeout_ms}ms"),
            );
        }
    };

    // Combine outputs.
    output.push_str(&stdout_out);
    if !stderr_out.is_empty() {
        output.push_str(&stderr_out);
    }

    // Truncate if necessary.
    if output.chars().count() > MAX_OUTPUT_CHARS {
        output = output.chars().take(MAX_OUTPUT_CHARS).collect();
        output.push_str("\n... (output truncated)");
        truncated = true;
    }

    if !exit_status.success() {
        let code = exit_status.code().unwrap_or(-1);
        output.push_str(&format!("\nExit code: {code}"));
        return ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(output),
            is_error: true,
            was_truncated: truncated,
        };
    }

    ToolResult {
        tool_use_id: tool_use_id.to_owned(),
        content: ToolResultPayload::Text(output),
        is_error: false,
        was_truncated: truncated,
    }
}

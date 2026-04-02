//! BashTool — execute shell commands.
//!
//! Ref: src/tools/BashTool/BashTool.tsx

use std::path::Path;

use async_trait::async_trait;
use code_permissions::bash_classifier::is_bash_command_read_only;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

use crate::{error_result, ProgressSender, Tool, ToolContext};
use crate::progress::emit_progress;

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_TIMEOUT_MS: u64 = 120_000; // 2 minutes
const MAX_TIMEOUT_MS: u64 = 600_000;     // 10 minutes
const MAX_OUTPUT_CHARS: usize = 200_000;

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BashInput {
    command: String,
    timeout_ms: Option<u64>,
    run_in_background: Option<bool>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Executes a given bash command and returns its output.\n\n\
        The working directory persists between commands, but shell state does not.\n\n\
        IMPORTANT: Avoid running commands that produce very large outputs.\n\
        You can set run_in_background to true to run a command in the background."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background"
                }
            },
            "required": ["command"]
        })
    }

    fn is_read_only(&self, input: &Value) -> bool {
        let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        is_bash_command_read_only(cmd)
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let command = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ValidationResult::err("command is required", 1),
        };

        if command.trim().is_empty() {
            return ValidationResult::err("command must not be empty", 1);
        }

        if let Some(timeout) = input.get("timeout_ms").and_then(|v| v.as_u64()) {
            if timeout > MAX_TIMEOUT_MS {
                return ValidationResult::err(
                    format!("timeout_ms must not exceed {MAX_TIMEOUT_MS}ms"),
                    1,
                );
            }
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
            content: input.get("command").and_then(|v| v.as_str()),
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
        progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: BashInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let timeout_ms = parsed.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        if parsed.run_in_background.unwrap_or(false) {
            return self.run_background(&parsed.command, &ctx.cwd, tool_use_id).await;
        }

        self.run_foreground(&parsed.command, &ctx.cwd, timeout_ms, tool_use_id, progress)
            .await
    }
}

impl BashTool {
    /// Run a command in the foreground with streaming output and a timeout.
    async fn run_foreground(
        &self,
        command: &str,
        cwd: &Path,
        timeout_ms: u64,
        tool_use_id: &str,
        progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let mut child = match spawn_shell(command, cwd) {
            Ok(c) => c,
            Err(e) => return error_result(tool_use_id, format!("Failed to spawn shell: {e}")),
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
        let mut stderr_reader = tokio::io::BufReader::new(stderr).lines();

        let mut output = String::new();
        let mut truncated = false;
        let tid = tool_use_id.to_owned();

        let run = async {
            loop {
                tokio::select! {
                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                emit_progress(progress, &tid, "Bash", json!({"line": l}));
                                if !truncated {
                                    output.push_str(&l);
                                    output.push('\n');
                                    if output.chars().count() >= MAX_OUTPUT_CHARS {
                                        truncated = true;
                                    }
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                if !truncated {
                                    output.push_str(&l);
                                    output.push('\n');
                                    if output.chars().count() >= MAX_OUTPUT_CHARS {
                                        truncated = true;
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(_) => {}
                        }
                    }
                }
            }

            // Drain any remaining stderr.
            while let Ok(Some(l)) = stderr_reader.next_line().await {
                if !truncated {
                    output.push_str(&l);
                    output.push('\n');
                    if output.chars().count() >= MAX_OUTPUT_CHARS {
                        truncated = true;
                    }
                }
            }

            child.wait().await
        };

        let timeout = std::time::Duration::from_millis(timeout_ms);
        let status = match tokio::time::timeout(timeout, run).await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                return error_result(tool_use_id, format!("Shell process error: {e}"));
            }
            Err(_) => {
                // Timeout — kill the child process.
                let _ = child.kill().await;
                return error_result(
                    tool_use_id,
                    format!(
                        "Command timed out after {}ms",
                        timeout_ms
                    ),
                );
            }
        };

        if truncated {
            output.push_str(&format!(
                "\n... [output truncated after {MAX_OUTPUT_CHARS} characters]"
            ));
        }

        let exit_code = status.code().unwrap_or(-1);
        if exit_code != 0 {
            output.push_str(&format!("\nExit code: {exit_code}"));
        }

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(output),
            is_error: exit_code != 0,
            was_truncated: truncated,
        }
    }

    /// Run a command in the background (fire-and-forget).
    async fn run_background(
        &self,
        command: &str,
        cwd: &Path,
        tool_use_id: &str,
    ) -> ToolResult {
        let mut child = match spawn_shell(command, cwd) {
            Ok(c) => c,
            Err(e) => return error_result(tool_use_id, format!("Failed to spawn shell: {e}")),
        };

        let pid = child.id().unwrap_or(0);

        // Detach by forgetting the Child handle (process continues independently).
        // We intentionally drop the JoinHandle.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(format!(
                "Process started in background with PID {pid}"
            )),
            is_error: false,
            was_truncated: false,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn spawn_shell(
    command: &str,
    cwd: &Path,
) -> std::io::Result<tokio::process::Child> {
    #[cfg(windows)]
    {
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", command])
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args(["-c", command])
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }
}


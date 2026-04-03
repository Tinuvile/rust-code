//! Shell hook executor: runs a command, writes event JSON to stdin, reads decision from stdout.
//!
//! Ref: src/utils/hooks/execAgentHook.ts (shell variant)

use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::event::{HookDecision, HookEvent};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Execute a shell hook command.
///
/// Spawns `sh -c <command>` (Unix) or `cmd /C <command>` (Windows), writes the
/// serialized `event` JSON to stdin, waits up to 30 s for stdout, then attempts
/// to parse the output as a `HookDecision`.
///
/// Any error (spawn failure, timeout, parse error) returns `Continue` so that
/// hook failures are never fatal.
pub async fn run_shell_hook(command: &str, event: &HookEvent) -> HookDecision {
    let event_json = match serde_json::to_string(event) {
        Ok(s) => s,
        Err(_) => return HookDecision::Continue,
    };

    let result = timeout(TIMEOUT, run_shell_inner(command, &event_json)).await;

    match result {
        Ok(Ok(decision)) => decision,
        _ => HookDecision::Continue,
    }
}

async fn run_shell_inner(command: &str, event_json: &str) -> anyhow::Result<HookDecision> {
    #[cfg(windows)]
    let mut child = Command::new("cmd")
        .args(["/C", command])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    #[cfg(not(windows))]
    let mut child = Command::new("sh")
        .args(["-c", command])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Write event JSON to stdin then close it.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(event_json.as_bytes()).await?;
        // stdin dropped here → EOF
    }

    let output = child.wait_with_output().await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(HookDecision::Continue);
    }

    let decision: HookDecision =
        serde_json::from_str(trimmed).unwrap_or(HookDecision::Continue);
    Ok(decision)
}

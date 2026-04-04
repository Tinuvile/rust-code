//! Shell task executor: run a command in the background with output streaming.
//!
//! Ref: src/tasks/LocalShellTask/

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::output::TaskOutput;
use crate::store::TaskStore;
use crate::task::{TaskId, TaskRecord};

// ── ShellTask ─────────────────────────────────────────────────────────────────

/// Spawn a shell command in the background, streaming its output to a log file.
///
/// Returns the task id. The task's status in `store` is updated as the process
/// runs and terminates.
pub async fn spawn_shell_task(
    command: &str,
    label: &str,
    store: Arc<TaskStore>,
    tasks_dir: &Path,
) -> Result<TaskId> {
    // Owned copy so it can move into the background task.
    let tasks_dir_buf: PathBuf = tasks_dir.to_path_buf();

    let mut record = TaskRecord::new_shell(label, command);
    let output = TaskOutput::new(&tasks_dir_buf, record.id.clone());
    record.log_path = Some(output.path().to_path_buf());

    let id = store.insert(record);
    store.update(&id, |r| r.mark_running());

    let mut cmd = shell_command(command);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    let store2 = Arc::clone(&store);
    let id2 = id.clone();

    tokio::spawn(async move {
        match cmd.spawn() {
            Err(e) => {
                store2.update(&id2, |r| r.mark_failed(e.to_string()));
            }
            Ok(mut child) => {
                let stdout = child.stdout.take().map(BufReader::new);
                let stderr = child.stderr.take().map(BufReader::new);

                // Stream stdout.
                if let Some(mut reader) = stdout {
                    let dir = tasks_dir_buf.clone();
                    let tid = id2.clone();
                    tokio::spawn(async move {
                        let out = TaskOutput::new(&dir, tid);
                        let mut line = String::new();
                        while let Ok(n) = reader.read_line(&mut line).await {
                            if n == 0 { break; }
                            let _ = out.append_line(&line).await;
                            line.clear();
                        }
                    });
                }

                // Stream stderr.
                if let Some(mut reader) = stderr {
                    let dir = tasks_dir_buf.clone();
                    let tid = id2.clone();
                    tokio::spawn(async move {
                        let out = TaskOutput::new(&dir, tid);
                        let mut line = String::new();
                        while let Ok(n) = reader.read_line(&mut line).await {
                            if n == 0 { break; }
                            let _ = out.append_line(&line).await;
                            line.clear();
                        }
                    });
                }

                // Wait for the process.
                match child.wait().await {
                    Ok(status) => {
                        let code = status.code();
                        if status.success() {
                            store2.update(&id2, |r| r.mark_completed(code));
                        } else {
                            store2.update(&id2, |r| {
                                r.exit_code = code;
                                r.mark_failed(format!(
                                    "process exited with code {}",
                                    code.unwrap_or(-1)
                                ));
                            });
                        }
                    }
                    Err(e) => {
                        store2.update(&id2, |r| r.mark_failed(e.to_string()));
                    }
                }
            }
        }
    });

    Ok(id)
}

// ── helpers ───────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(cmd);
    c
}

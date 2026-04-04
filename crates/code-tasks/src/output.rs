//! Task output streaming: append task stdout/stderr to a log file.
//!
//! Ref: src/tools/TaskOutputTool/

use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::io::AsyncWriteExt;

use crate::task::TaskId;

// ── TaskOutput ────────────────────────────────────────────────────────────────

/// Writes task output lines to an append-only log file.
pub struct TaskOutput {
    task_id: TaskId,
    path: PathBuf,
}

impl TaskOutput {
    /// Create a new task output file under `tasks_dir/<task_id>.log`.
    pub fn new(tasks_dir: &Path, task_id: TaskId) -> Self {
        let path = tasks_dir.join(format!("{task_id}.log"));
        Self { task_id, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    /// Append a line to the log file (creates the file if absent).
    pub async fn append_line(&self, line: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        if !line.ends_with('\n') {
            file.write_all(b"\n").await?;
        }
        Ok(())
    }

    /// Append raw bytes to the log file.
    pub async fn append_bytes(&self, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(bytes).await?;
        Ok(())
    }

    /// Read all output collected so far.
    pub async fn read_all(&self) -> Result<String> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Read at most `max_lines` lines from the tail of the log.
    pub async fn tail(&self, max_lines: usize) -> Result<Vec<String>> {
        let content = self.read_all().await?;
        let lines: Vec<String> = content
            .lines()
            .rev()
            .take(max_lines)
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(lines)
    }
}

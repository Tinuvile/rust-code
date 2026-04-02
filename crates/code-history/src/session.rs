//! Session directory management.
//!
//! Each session lives in `~/.claude/sessions/{session_id}/`.
//! This module provides helpers for building paths and ensuring directories.
//!
//! Ref: src/utils/sessionStorage.ts (getSessionDir, createSession)

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use code_types::ids::SessionId;
use code_types::message::SessionMetadata;

use crate::metadata::save_metadata;

// ── Paths ─────────────────────────────────────────────────────────────────────

/// Root directory for all sessions: `~/.claude/sessions/`.
pub fn sessions_root() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("sessions")
}

/// Directory for a specific session: `~/.claude/sessions/{session_id}/`.
pub fn session_dir(id: &SessionId) -> PathBuf {
    sessions_root().join(id.to_string())
}

// ── SessionHandle ─────────────────────────────────────────────────────────────

/// Handle to a live session's directory and identity.
///
/// Cheap to clone; just wraps a `SessionId` and its derived paths.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: SessionId,
    pub dir: PathBuf,
}

impl SessionHandle {
    /// Create or resume a session handle and ensure the directory exists.
    pub async fn create_or_resume(id: SessionId) -> anyhow::Result<Self> {
        let dir = session_dir(&id);
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("cannot create session dir {}", dir.display()))?;
        Ok(Self { id, dir })
    }

    /// Create a brand-new session, writing an initial `meta.json`.
    pub async fn new_session(
        cwd: impl Into<PathBuf>,
        model: impl Into<String>,
        git_branch: Option<String>,
    ) -> anyhow::Result<Self> {
        let id = SessionId::new();
        let dir = session_dir(&id);
        tokio::fs::create_dir_all(&dir).await?;

        let now = unix_now();
        let meta = SessionMetadata {
            session_id: id.clone(),
            title: None,
            created_at: now,
            last_active_at: now,
            model: model.into(),
            total_cost_usd: 0.0,
            message_count: 0,
            cwd: cwd.into().to_string_lossy().into_owned(),
            git_branch,
            tags: HashMap::new(),
        };
        save_metadata(&dir, &meta).await?;
        Ok(Self { id, dir })
    }

    /// Path to the JSONL transcript file.
    pub fn transcript_path(&self) -> PathBuf {
        self.dir.join("transcript.jsonl")
    }

    /// Path to the session metadata file.
    pub fn meta_path(&self) -> PathBuf {
        self.dir.join("meta.json")
    }

    /// Path to the tool-result storage directory.
    pub fn tool_results_dir(&self) -> PathBuf {
        self.dir.join("tool-results")
    }

    /// Path to the tasks directory.
    pub fn tasks_dir(&self) -> PathBuf {
        self.dir.join("tasks")
    }

    /// Path to the todos file.
    pub fn todos_path(&self) -> PathBuf {
        self.dir.join("todos.json")
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

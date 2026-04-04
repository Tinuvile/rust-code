//! SDK session management: create, list, get, fork sessions.
//!
//! Ref: src/entrypoints/sdk/controlSchemas.ts

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use code_types::ids::SessionId;

// ── SdkSessionInfo ────────────────────────────────────────────────────────────

/// Public session metadata returned by list/get operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkSessionInfo {
    pub session_id: String,
    /// Path to the session directory on disk.
    pub session_dir: PathBuf,
    /// Unix timestamp (s) of session creation.
    pub created_at: u64,
    /// Unix timestamp (s) of last activity.
    pub updated_at: u64,
    /// Whether a transcript file exists for this session.
    pub has_transcript: bool,
    /// Working directory at session start.
    pub cwd: Option<String>,
}

// ── CreateSessionOptions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CreateSessionOptions {
    /// If `Some`, attempt to resume this session; otherwise create a new one.
    pub session_id: Option<String>,
    /// Working directory for the new session.
    pub cwd: Option<PathBuf>,
    /// Model to use (falls back to default if not set).
    pub model: Option<String>,
}

// ── ForkSessionOptions ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ForkSessionOptions {
    /// Parent session to fork from.
    pub parent_session_id: String,
    /// Optional message index to fork at (forks at the end if `None`).
    pub message_index: Option<usize>,
}

// ── ForkSessionResult ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkSessionResult {
    pub session_id: String,
    pub session_dir: PathBuf,
}

// ── SessionManager ────────────────────────────────────────────────────────────

/// Manages session lifecycle for SDK consumers.
pub struct SessionManager {
    sessions_root: PathBuf,
}

impl SessionManager {
    /// Create a manager rooted at `~/.claude/sessions/`.
    pub fn new() -> Result<Self> {
        let home = dirs_next::home_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        Ok(Self { sessions_root: home.join(".claude").join("sessions") })
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { sessions_root: root }
    }

    /// Create or resume a session.
    pub async fn create(&self, opts: CreateSessionOptions) -> Result<SdkSessionInfo> {
        let session_id = if let Some(id) = opts.session_id {
            if let Ok(uuid) = id.parse() {
                SessionId::from_uuid(uuid)
            } else {
                SessionId::new()
            }
        } else {
            SessionId::new()
        };

        let dir = self.sessions_root.join(session_id.to_string());
        tokio::fs::create_dir_all(&dir).await?;

        Ok(SdkSessionInfo {
            session_id: session_id.to_string(),
            session_dir: dir,
            created_at: unix_now(),
            updated_at: unix_now(),
            has_transcript: false,
            cwd: opts.cwd.and_then(|p| p.to_str().map(str::to_owned)),
        })
    }

    /// List all sessions (sorted newest-first).
    pub async fn list(&self, _opts: ListSessionsOptions) -> Result<Vec<SdkSessionInfo>> {
        let mut sessions = Vec::new();

        let mut entries = match tokio::fs::read_dir(&self.sessions_root).await {
            Ok(e) => e,
            Err(_) => return Ok(sessions),
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
                let dir = entry.path();
                let id = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();

                let has_transcript = dir.join("transcript.jsonl").exists();
                let meta = tokio::fs::metadata(&dir).await;
                let updated_at = meta
                    .and_then(|m| {
                        m.modified().map(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs()
                        })
                    })
                    .unwrap_or(0);

                sessions.push(SdkSessionInfo {
                    session_id: id,
                    session_dir: dir,
                    created_at: 0,
                    updated_at,
                    has_transcript,
                    cwd: None,
                });
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Get info for a specific session.
    pub async fn get(&self, session_id: &str) -> Result<SdkSessionInfo> {
        let dir = self.sessions_root.join(session_id);
        if !dir.exists() {
            return Err(anyhow::anyhow!("session not found: {session_id}"));
        }
        let has_transcript = dir.join("transcript.jsonl").exists();
        Ok(SdkSessionInfo {
            session_id: session_id.to_owned(),
            session_dir: dir,
            created_at: 0,
            updated_at: unix_now(),
            has_transcript,
            cwd: None,
        })
    }

    /// Fork a session by copying its transcript to a new session directory.
    pub async fn fork(&self, opts: ForkSessionOptions) -> Result<ForkSessionResult> {
        let parent_dir = self.sessions_root.join(&opts.parent_session_id);
        if !parent_dir.exists() {
            return Err(anyhow::anyhow!("parent session not found: {}", opts.parent_session_id));
        }

        let new_id = SessionId::new();
        let new_dir = self.sessions_root.join(new_id.to_string());
        tokio::fs::create_dir_all(&new_dir).await?;

        // Copy transcript if it exists.
        let src_transcript = parent_dir.join("transcript.jsonl");
        if src_transcript.exists() {
            let dst_transcript = new_dir.join("transcript.jsonl");
            if let Some(idx) = opts.message_index {
                // Truncate at message_index.
                copy_transcript_truncated(&src_transcript, &dst_transcript, idx).await?;
            } else {
                tokio::fs::copy(&src_transcript, &dst_transcript).await?;
            }
        }

        Ok(ForkSessionResult {
            session_id: new_id.to_string(),
            session_dir: new_dir,
        })
    }
}

// ── ListSessionsOptions ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ListSessionsOptions {
    /// Maximum number of sessions to return.
    pub limit: Option<usize>,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn copy_transcript_truncated(src: &Path, dst: &Path, max_lines: usize) -> Result<()> {
    let content = tokio::fs::read_to_string(src).await?;
    let truncated: String = content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(dst, truncated).await?;
    Ok(())
}

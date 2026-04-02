//! Session listing and resume.
//!
//! Scans `~/.claude/sessions/` to enumerate available sessions, sorted by
//! `last_active_at`.  Used by `--resume` and the session-picker TUI.
//!
//! Ref: src/utils/sessionRestore.ts, src/utils/conversationRecovery.ts

use code_types::ids::SessionId;
use code_types::message::SessionMetadata;

use crate::metadata::load_metadata;
use crate::session::{session_dir, sessions_root};
use crate::transcript::Transcript;

// ── SessionSummary ────────────────────────────────────────────────────────────

/// Lightweight summary of a session, used for listing and resume.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: SessionId,
    pub title: Option<String>,
    pub last_active_at: u64,
    pub created_at: u64,
    pub cwd: String,
    pub message_count: u32,
    pub total_cost_usd: f64,
    pub model: String,
    pub git_branch: Option<String>,
}

impl From<SessionMetadata> for SessionSummary {
    fn from(m: SessionMetadata) -> Self {
        Self {
            id: m.session_id,
            title: m.title,
            last_active_at: m.last_active_at,
            created_at: m.created_at,
            cwd: m.cwd,
            message_count: m.message_count,
            total_cost_usd: m.total_cost_usd,
            model: m.model,
            git_branch: m.git_branch,
        }
    }
}

// ── Listing ───────────────────────────────────────────────────────────────────

/// List all sessions, sorted by `last_active_at` descending (most recent first).
pub async fn list_sessions() -> anyhow::Result<Vec<SessionSummary>> {
    let root = sessions_root();
    let mut sessions = Vec::new();

    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
        Err(e) => return Err(e.into()),
    };

    while let Some(entry) = entries.next_entry().await? {
        let ft = entry.file_type().await?;
        if !ft.is_dir() {
            continue;
        }
        let dir = entry.path();
        if let Some(meta) = load_metadata(&dir).await? {
            sessions.push(SessionSummary::from(meta));
        }
    }

    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(sessions)
}

/// Return the most recently active session, or `None` if no sessions exist.
pub async fn most_recent() -> anyhow::Result<Option<SessionSummary>> {
    let all = list_sessions().await?;
    Ok(all.into_iter().next())
}

/// Find a session by `SessionId`.
pub async fn find_session(id: &SessionId) -> anyhow::Result<Option<SessionSummary>> {
    let dir = session_dir(id);
    match load_metadata(&dir).await? {
        Some(meta) => Ok(Some(SessionSummary::from(meta))),
        None => Ok(None),
    }
}

// ── Resume ────────────────────────────────────────────────────────────────────

/// Load the complete message history for a session to allow resumption.
///
/// Returns `None` if the session directory or transcript do not exist.
pub async fn load_session_messages(
    id: &SessionId,
) -> anyhow::Result<Option<Vec<code_types::message::Message>>> {
    let dir = session_dir(id);
    if !dir.exists() {
        return Ok(None);
    }
    let transcript = Transcript::new(&dir);
    if !transcript.exists() {
        return Ok(None);
    }
    let messages = transcript.load_all().await?;
    Ok(Some(messages))
}

/// Delete a session directory entirely.
pub async fn delete_session(id: &SessionId) -> anyhow::Result<()> {
    let dir = session_dir(id);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

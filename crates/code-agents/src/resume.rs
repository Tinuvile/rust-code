//! Agent resume: serialize and restore agent state between sessions.
//!
//! Persists the conversation history and metadata to a JSON file under
//! `~/.claude/agents/<agent_id>.json` so an agent can be resumed later.
//!
//! Ref: src/tools/AgentTool/resumeAgent.ts

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use code_types::message::Message;

// ── AgentState ────────────────────────────────────────────────────────────────

/// Persisted state for a paused / completed agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Unique agent run identifier.
    pub agent_id: String,
    /// Agent type (matches `AgentDefinition::agent_type`).
    pub agent_type: String,
    /// Original task prompt.
    pub original_prompt: String,
    /// Conversation history at the time of suspension.
    pub conversation: Vec<Message>,
    /// Whether the agent completed normally.
    pub is_complete: bool,
    /// Final output text (set when `is_complete == true`).
    pub output: Option<String>,
    /// Wall-clock timestamp (Unix seconds) of last update.
    pub updated_at: u64,
}

impl AgentState {
    /// Create a new in-progress state.
    pub fn new(agent_id: impl Into<String>, agent_type: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_type: agent_type.into(),
            original_prompt: prompt.into(),
            conversation: Vec::new(),
            is_complete: false,
            output: None,
            updated_at: unix_now(),
        }
    }

    /// Mark the agent as complete with a final output.
    pub fn complete(&mut self, output: impl Into<String>) {
        self.is_complete = true;
        self.output = Some(output.into());
        self.updated_at = unix_now();
    }

    /// Save state to disk.
    pub async fn save(&self, dir: &Path) -> Result<()> {
        tokio::fs::create_dir_all(dir).await?;
        let path = state_path(dir, &self.agent_id);
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Load state from disk.
    pub async fn load(dir: &Path, agent_id: &str) -> Result<Self> {
        let path = state_path(dir, agent_id);
        let json = tokio::fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&json)?)
    }

    /// List all saved agent states in a directory.
    pub async fn list(dir: &Path) -> Vec<Self> {
        let mut result = Vec::new();
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return result,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(json) = tokio::fs::read_to_string(&path).await {
                    if let Ok(state) = serde_json::from_str::<Self>(&json) {
                        result.push(state);
                    }
                }
            }
        }
        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        result
    }
}

// ── Default state directory ───────────────────────────────────────────────────

/// Default directory for persisted agent states: `~/.claude/agent-states/`.
pub fn default_state_dir() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h: PathBuf| h.join(".claude").join("agent-states"))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn state_path(dir: &Path, agent_id: &str) -> PathBuf {
    dir.join(format!("{agent_id}.json"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

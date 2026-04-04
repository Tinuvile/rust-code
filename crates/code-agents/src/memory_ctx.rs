//! Memory context passed to agents: CLAUDE.md content + memory entries.
//!
//! Ref: src/tools/AgentTool/agentMemory.ts

use std::path::Path;

use anyhow::Result;

/// Memory context loaded for an agent from CLAUDE.md files and the memory dir.
#[derive(Debug, Default, Clone)]
pub struct AgentMemoryContext {
    /// Content collected from CLAUDE.md files visible to the agent.
    pub claude_md: Vec<String>,
    /// Raw memory entry contents.
    pub memory_entries: Vec<String>,
}

impl AgentMemoryContext {
    /// Load CLAUDE.md from `cwd` and each parent up to the root.
    pub async fn load_from_cwd(cwd: &Path) -> Self {
        let mut ctx = Self::default();
        let mut dir = cwd.to_path_buf();

        loop {
            let claude_md = dir.join("CLAUDE.md");
            if let Ok(content) = tokio::fs::read_to_string(&claude_md).await {
                if !content.trim().is_empty() {
                    ctx.claude_md.push(content);
                }
            }
            if !dir.pop() {
                break;
            }
        }

        ctx
    }

    /// Build the system-prompt appendix from this context.
    pub fn to_system_appendix(&self) -> Option<String> {
        let parts: Vec<&str> = self
            .claude_md
            .iter()
            .map(|s| s.as_str())
            .chain(self.memory_entries.iter().map(|s| s.as_str()))
            .collect();

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }
}

/// Snapshot of an agent's memory state for persistence/resume.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentMemorySnapshot {
    pub entries: Vec<String>,
}

impl AgentMemorySnapshot {
    /// Save to a JSON file.
    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Load from a JSON file.
    pub async fn load(path: &Path) -> Result<Self> {
        let json = tokio::fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&json)?)
    }
}

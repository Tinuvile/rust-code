//! Agent registry: combines built-in and user-defined agents.
//!
//! Ref: src/tools/AgentTool/loadAgentsDir.ts

use std::path::Path;
use std::sync::Arc;

use crate::builtin::all_builtin_agents;
use crate::definition::AgentDefinition;
use crate::loader::{load_agents_dir, load_global_agents};

// ── AgentRegistry ─────────────────────────────────────────────────────────────

/// Holds all available agent definitions (built-in + user-defined).
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    agents: Vec<Arc<AgentDefinition>>,
}

impl AgentRegistry {
    /// Create a registry containing only the built-in agents.
    pub fn with_builtin() -> Self {
        let agents = all_builtin_agents()
            .into_iter()
            .map(Arc::new)
            .collect();
        Self { agents }
    }

    /// Build a registry by loading built-in agents plus any user-defined ones
    /// from `<cwd>/.claude/agents/` and `~/.claude/agents/`.
    pub async fn load(cwd: &Path) -> Self {
        let mut all = all_builtin_agents();

        // Global user agents (~/.claude/agents/) — lower priority than project-local.
        let global = load_global_agents().await;
        all.extend(global);

        // Project-local agents (.claude/agents/) — take precedence.
        let local = load_agents_dir(cwd).await;
        all.extend(local);

        let agents = all.into_iter().map(Arc::new).collect();
        Self { agents }
    }

    /// Look up by `agent_type` (exact match) or by `name` (case-insensitive).
    pub fn find(&self, query: &str) -> Option<Arc<AgentDefinition>> {
        let lower = query.to_lowercase();
        self.agents
            .iter()
            .find(|a| a.agent_type == query || a.name.to_lowercase() == lower)
            .cloned()
    }

    /// All registered agents.
    pub fn all(&self) -> &[Arc<AgentDefinition>] {
        &self.agents
    }

    /// Number of registered agents.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

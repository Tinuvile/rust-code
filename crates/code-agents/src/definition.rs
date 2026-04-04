//! AgentDefinition struct and related types.
//!
//! Ref: src/tools/AgentTool/loadAgentsDir.ts

use serde::{Deserialize, Serialize};

// ── AgentSource ───────────────────────────────────────────────────────────────

/// Source of the agent definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSource {
    /// Ships with the binary.
    #[default]
    BuiltIn,
    /// Loaded from `.claude/agents/<name>.md` or `.yaml`.
    UserDefined,
    /// Loaded from an MCP server.
    Mcp,
}

// ── AgentDefinition ───────────────────────────────────────────────────────────

/// A complete agent specification that can be executed by the agent runner.
///
/// Mirrors `AgentDefinition` / `BuiltInAgentDefinition` from the TypeScript source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Machine-readable identifier, e.g. `"general-purpose"`.
    pub agent_type: String,

    /// Human-readable display name, e.g. `"General Purpose Agent"`.
    #[serde(default)]
    pub name: String,

    /// System prompt injected at the start of every conversation with this agent.
    #[serde(default)]
    pub system_prompt: String,

    /// Short description shown in `/agent` listings.
    #[serde(default)]
    pub description: String,

    /// Hint to the parent agent about when to invoke this sub-agent.
    #[serde(default)]
    pub when_to_use: String,

    /// Tool allow-list.  `["*"]` means all tools are allowed.
    #[serde(default)]
    pub tools: Vec<String>,

    /// Optional model override (falls back to session model when `None`).
    #[serde(default)]
    pub model: Option<String>,

    /// Optional TUI color name (e.g. `"blue"`, `"orange"`).
    #[serde(default)]
    pub color: Option<String>,

    /// Where the definition came from.
    #[serde(default)]
    pub source: AgentSource,

    /// Maximum agentic turns before the agent is forced to stop.
    #[serde(default)]
    pub max_turns: Option<u32>,
}

impl AgentDefinition {
    /// Returns true if the tool list is unrestricted (`["*"]`).
    pub fn allows_all_tools(&self) -> bool {
        self.tools.iter().any(|t| t == "*")
    }

    /// Returns true if this agent is allowed to use `tool_name`.
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        self.allows_all_tools() || self.tools.iter().any(|t| t == tool_name)
    }
}

//! Skill struct and related types.
//!
//! Ref: src/skills/bundledSkills.ts, src/skills/loadSkillsDir.ts

use serde::{Deserialize, Serialize};

// ── SkillSource ───────────────────────────────────────────────────────────────

/// Where the skill originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSource {
    #[default]
    Bundled,
    UserDefined,
    Mcp,
}

// ── SkillContext ──────────────────────────────────────────────────────────────

/// Whether the skill runs inline or as a forked sub-agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillContext {
    /// Runs in the current conversation.
    #[default]
    Inline,
    /// Runs in a forked sub-agent (isolated QueryEngine).
    Fork,
}

// ── Skill ─────────────────────────────────────────────────────────────────────

/// A skill that can be invoked via slash command or by the model using SkillTool.
///
/// Mirrors `BundledSkillDefinition` / `Command` from the TypeScript source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Slash-command name (without leading `/`), e.g. `"debug"`.
    pub name: String,

    /// One-line description shown in `/help`.
    #[serde(default)]
    pub description: String,

    /// Alternate names that also invoke this skill.
    #[serde(default)]
    pub aliases: Vec<String>,

    /// Hint shown to the model about when to invoke this skill.
    #[serde(default)]
    pub when_to_use: String,

    /// The system prompt / instructions injected when the skill is invoked.
    #[serde(default)]
    pub content: String,

    /// Tool allow-list.  Empty means all tools are allowed.
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Optional model override.
    #[serde(default)]
    pub model: Option<String>,

    /// Whether the skill is available as a slash command to the user.
    #[serde(default = "default_true")]
    pub user_invocable: bool,

    /// Execution context: inline or fork.
    #[serde(default)]
    pub context: SkillContext,

    /// Source of this skill.
    #[serde(default)]
    pub source: SkillSource,

    /// Argument hint shown in completions (e.g. `"<filename>"`).
    #[serde(default)]
    pub argument_hint: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Skill {
    /// Returns true if the skill's tool list is unrestricted or empty.
    pub fn allows_all_tools(&self) -> bool {
        self.allowed_tools.is_empty() || self.allowed_tools.iter().any(|t| t == "*")
    }
}

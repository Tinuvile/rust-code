//! Skills system: bundled skills, custom loading, MCP skill builders.
//!
//! Ref: src/skills/bundledSkills.ts, src/skills/loadSkillsDir.ts,
//!      src/skills/mcpSkillBuilders.ts, src/skills/bundled/

pub mod skill;
pub mod registry;
pub mod loader;
pub mod mcp_builder;

// Bundled skills
pub mod bundled;

#[cfg(feature = "skill_search")]
pub mod search;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use skill::{Skill, SkillContext, SkillSource};
pub use registry::SkillRegistry;
pub use bundled::all_bundled_skills;
pub use mcp_builder::{McpToolDescriptor, skill_from_mcp_tool, skills_from_mcp_tools};

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

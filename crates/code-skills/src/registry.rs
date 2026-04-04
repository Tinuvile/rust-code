//! Skill registry: combines bundled and user-defined skills.
//!
//! Ref: src/skills/bundledSkills.ts, src/skills/loadSkillsDir.ts

use std::path::Path;
use std::sync::Arc;

use crate::bundled::all_bundled_skills;
use crate::loader::{load_global_skills, load_skills_dir};
use crate::skill::Skill;

// ── SkillRegistry ─────────────────────────────────────────────────────────────

/// Holds all available skill definitions (bundled + user-defined).
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: Vec<Arc<Skill>>,
}

impl SkillRegistry {
    /// Create a registry containing only the bundled skills.
    pub fn with_bundled() -> Self {
        let skills = all_bundled_skills().into_iter().map(Arc::new).collect();
        Self { skills }
    }

    /// Build a registry by loading bundled skills plus user-defined ones
    /// from `<cwd>/.claude/skills/` and `~/.claude/skills/`.
    pub async fn load(cwd: &Path) -> Self {
        let mut all = all_bundled_skills();

        // Global user skills (~/.claude/skills/).
        let global = load_global_skills().await;
        all.extend(global);

        // Project-local skills (.claude/skills/) — take precedence.
        let local = load_skills_dir(cwd).await;
        all.extend(local);

        let skills = all.into_iter().map(Arc::new).collect();
        Self { skills }
    }

    /// Look up by name or alias (case-insensitive).
    pub fn find(&self, query: &str) -> Option<Arc<Skill>> {
        let lower = query.to_lowercase();
        self.skills.iter().find(|s| {
            s.name.to_lowercase() == lower
                || s.aliases.iter().any(|a| a.to_lowercase() == lower)
        }).cloned()
    }

    /// All user-invocable skills (shown in `/help` and completions).
    pub fn user_invocable(&self) -> Vec<Arc<Skill>> {
        self.skills
            .iter()
            .filter(|s| s.user_invocable)
            .cloned()
            .collect()
    }

    /// All registered skills.
    pub fn all(&self) -> &[Arc<Skill>] {
        &self.skills
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

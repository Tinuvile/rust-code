//! Skill search: relevance-scored lookup for the `skill_search` feature.
//!
//! When the `skill_search` feature is enabled, the model can query this module
//! to discover skills relevant to the current task without listing all of them.
//!
//! Ref: src/services/skillSearch/

use crate::skill::Skill;

// ── ScoredSkill ───────────────────────────────────────────────────────────────

/// A skill with an associated relevance score.
#[derive(Debug)]
pub struct ScoredSkill<'a> {
    pub skill: &'a Skill,
    /// Relevance score in [0.0, 1.0].
    pub score: f32,
}

// ── search ────────────────────────────────────────────────────────────────────

/// Return skills from `candidates` most relevant to `query`, sorted by score.
///
/// Uses simple keyword overlap scoring.  A full implementation would use
/// embedding-based similarity.
pub fn search_skills<'a>(query: &str, candidates: &'a [Skill], top_k: usize) -> Vec<ScoredSkill<'a>> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<ScoredSkill<'a>> = candidates
        .iter()
        .filter_map(|skill| {
            let haystack = format!(
                "{} {} {}",
                skill.name, skill.description, skill.when_to_use
            )
            .to_lowercase();

            let matches = query_words
                .iter()
                .filter(|w| haystack.contains(*w))
                .count();

            if matches == 0 {
                None
            } else {
                let score = matches as f32 / query_words.len().max(1) as f32;
                Some(ScoredSkill { skill, score })
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

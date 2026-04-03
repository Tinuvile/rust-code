//! Keyword-based relevance scoring for memory entries.
//!
//! Scores each `ScannedMemoryEntry` against a query string and returns the
//! entries sorted by descending relevance.  Zero-score entries are retained
//! so the caller (prompt formatter) can apply hard limits rather than silently
//! dropping low-signal entries.
//!
//! Ref: src/memdir/findRelevantMemories.ts

use crate::memory_type::MemorySource;
use crate::scanner::ScannedMemoryEntry;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A memory entry with a computed relevance score.
#[derive(Debug, Clone)]
pub struct ScoredEntry {
    pub entry: ScannedMemoryEntry,
    /// Relevance score ≥ 0.  Higher means more relevant to the query.
    pub score: f32,
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Score a single `ScannedMemoryEntry` against a pre-tokenised query.
///
/// Scoring rules (cumulative):
/// - **+2.0** per frontmatter keyword that appears in `query_words`
/// - **+1.0** per query word that appears in the entry body (case-insensitive)
/// - **+0.5** per point of `frontmatter.priority`
/// - **+0.5** base boost for `MemorySource::Global` entries (always relevant)
pub fn score_entry(entry: &ScannedMemoryEntry, query_words: &[&str]) -> f32 {
    let mut score: f32 = 0.0;

    // Base boost for global entries.
    if matches!(entry.entry.source, MemorySource::Global) {
        score += 0.5;
    }

    // Priority bonus.
    if let Some(p) = entry.frontmatter.priority {
        score += 0.5 * p as f32;
    }

    // Keyword matches (+2.0 each).
    for keyword in &entry.frontmatter.keywords {
        let kw_lower = keyword.to_lowercase();
        if query_words
            .iter()
            .any(|w| *w == kw_lower || w.contains(&*kw_lower) || kw_lower.contains(*w))
        {
            score += 2.0;
        }
    }

    // Body word matches (+1.0 each unique query word found in body).
    let body_lower = entry.body.to_lowercase();
    for word in query_words {
        if !word.is_empty() && body_lower.contains(*word) {
            score += 1.0;
        }
    }

    score
}

/// Score all entries against `query` and return them sorted by descending score.
///
/// Zero-score entries are still included — the prompt formatter decides what to
/// include based on byte/line budgets, not relevance alone.
pub fn rank_entries(entries: Vec<ScannedMemoryEntry>, query: &str) -> Vec<ScoredEntry> {
    let words: Vec<&str> = tokenise(query);

    let mut scored: Vec<ScoredEntry> = entries
        .into_iter()
        .map(|e| {
            let score = score_entry(&e, &words);
            ScoredEntry { entry: e, score }
        })
        .collect();

    // Stable sort so equal-score entries keep their original (load) order.
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Tokenise a query string into lowercase words, stripping punctuation.
fn tokenise(query: &str) -> Vec<&str> {
    query
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_type::{MemoryEntry, MemorySource};
    use crate::scanner::{MemoryFrontmatter, ScannedMemoryEntry};
    use std::path::PathBuf;

    fn make_entry(source: MemorySource, body: &str, keywords: Vec<&str>, priority: Option<u32>) -> ScannedMemoryEntry {
        let content = body.to_owned();
        ScannedMemoryEntry {
            entry: MemoryEntry {
                content: content.clone(),
                source,
                path: PathBuf::from("test.md"),
                is_claude_md: true,
            },
            frontmatter: MemoryFrontmatter {
                keywords: keywords.into_iter().map(|s| s.to_owned()).collect(),
                description: None,
                priority,
            },
            body: body.to_owned(),
        }
    }

    #[test]
    fn global_entry_gets_base_boost() {
        let entry = make_entry(MemorySource::Global, "irrelevant content", vec![], None);
        let score = score_entry(&entry, &[]);
        assert!(score > 0.0, "global entry should always score > 0");
    }

    #[test]
    fn keyword_match_scores_higher() {
        let with_kw = make_entry(MemorySource::Memdir { name: "a".into() }, "body", vec!["rust"], None);
        let without_kw = make_entry(MemorySource::Memdir { name: "b".into() }, "body", vec![], None);
        let words = tokenise("I love rust programming");
        let s1 = score_entry(&with_kw, &words);
        let s2 = score_entry(&without_kw, &words);
        assert!(s1 > s2);
    }

    #[test]
    fn rank_returns_highest_first() {
        let low = make_entry(MemorySource::Memdir { name: "x".into() }, "nothing relevant", vec![], None);
        let high = make_entry(MemorySource::Global, "rust testing patterns", vec!["rust"], Some(2));
        let ranked = rank_entries(vec![low, high], "rust");
        assert_eq!(ranked[0].entry.frontmatter.keywords, vec!["rust"]);
    }
}

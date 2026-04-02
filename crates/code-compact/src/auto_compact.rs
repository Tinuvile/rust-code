//! Auto-compact trigger logic.
//!
//! Determines when the context window is full enough to warrant automatic
//! compaction.  The threshold is configurable per session.
//!
//! Ref: src/services/compact/autoCompact.ts

/// Configuration for automatic context compaction.
#[derive(Debug, Clone)]
pub struct AutoCompactConfig {
    /// Fraction of the context window at which compaction triggers (0.0–1.0).
    /// Default: 0.85 (mirrors `code_api::tokens::AUTO_COMPACT_THRESHOLD`).
    pub threshold_fraction: f32,
    /// Total context window size in tokens for the active model.
    pub context_window: u32,
    /// Whether auto-compact is enabled (from global config).
    pub enabled: bool,
}

impl Default for AutoCompactConfig {
    fn default() -> Self {
        Self {
            threshold_fraction: code_api::tokens::AUTO_COMPACT_THRESHOLD,
            context_window: 200_000, // Claude Sonnet 4 default
            enabled: true,
        }
    }
}

impl AutoCompactConfig {
    /// Create a config from global settings and the active model's context window.
    pub fn new(enabled: bool, context_window: u32) -> Self {
        Self {
            threshold_fraction: code_api::tokens::AUTO_COMPACT_THRESHOLD,
            context_window,
            enabled,
        }
    }
}

/// Returns `true` if auto-compact should trigger given the current token usage.
pub fn should_auto_compact(used_tokens: u32, config: &AutoCompactConfig) -> bool {
    if !config.enabled {
        return false;
    }
    code_api::tokens::should_auto_compact(used_tokens, config.context_window)
}

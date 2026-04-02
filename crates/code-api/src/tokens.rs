//! Token counting and estimation utilities.
//!
//! The Anthropic API does not expose a public tokenizer, so we use character-
//! level heuristics for budget estimation.  Actual token counts from API
//! responses always take precedence.
//!
//! Ref: src/utils/tokens.ts

use code_types::message::TokenUsage;

// ── Heuristics ────────────────────────────────────────────────────────────────

/// Estimated tokens per character for English prose.
/// ~4 chars/token is a common rule of thumb for Claude models.
const CHARS_PER_TOKEN: f64 = 4.0;

/// Estimate the token count for a string using character heuristics.
///
/// Ref: src/utils/tokens.ts (character-based estimation)
pub fn estimate_tokens(text: &str) -> u32 {
    ((text.len() as f64 / CHARS_PER_TOKEN).ceil() as u32).max(1)
}

/// Estimate tokens for a JSON value.
pub fn estimate_tokens_json(value: &serde_json::Value) -> u32 {
    estimate_tokens(&value.to_string())
}

// ── Usage accumulation ────────────────────────────────────────────────────────

/// Accumulated token usage across multiple API calls in a session.
///
/// Ref: src/utils/tokens.ts (session token accumulation)
#[derive(Debug, Clone, Default)]
pub struct SessionUsage {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_input_tokens: u64,
    pub total_cache_read_input_tokens: u64,
    pub total_web_search_requests: u64,
    pub api_call_count: u32,
}

impl SessionUsage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add usage from a single API response.
    pub fn add(&mut self, usage: &TokenUsage) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        self.total_cache_creation_input_tokens +=
            usage.cache_creation_input_tokens as u64;
        self.total_cache_read_input_tokens +=
            usage.cache_read_input_tokens as u64;
        self.api_call_count += 1;
    }

    /// Total tokens consumed (input + output, excluding cache tokens).
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }
}

// ── Context-window budget ─────────────────────────────────────────────────────

/// How close to the context window limit we are (0.0 = empty, 1.0 = full).
pub fn context_fill_ratio(used_tokens: u32, context_window: u32) -> f32 {
    if context_window == 0 {
        return 0.0;
    }
    (used_tokens as f32 / context_window as f32).clamp(0.0, 1.0)
}

/// Threshold at which auto-compact should trigger (85% of context window).
pub const AUTO_COMPACT_THRESHOLD: f32 = 0.85;

/// `true` if the current usage warrants triggering auto-compact.
pub fn should_auto_compact(used_tokens: u32, context_window: u32) -> bool {
    context_fill_ratio(used_tokens, context_window) >= AUTO_COMPACT_THRESHOLD
}

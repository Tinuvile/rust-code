//! Token budget tracking for the active context window.
//!
//! Estimates token usage using character heuristics (actual counts from the
//! API always take precedence).  Triggers warnings and auto-compact thresholds.
//!
//! Ref: src/utils/tokenBudget.ts

use code_types::message::{ContentBlock, Message};

// ── Context windows ───────────────────────────────────────────────────────────

/// Return the context window size (in tokens) for a given model name.
///
/// Supports Anthropic, OpenAI, Gemini, DeepSeek, Kimi, and Minimax models.
/// Falls back to 128 000 for unknown models.
pub fn context_window_for_model(model: &str) -> u32 {
    let lower = model.to_lowercase();
    // Anthropic
    if lower.contains("opus") || lower.contains("sonnet") || lower.contains("haiku") {
        return 200_000;
    }
    // OpenAI
    if lower.starts_with("o3") || lower.starts_with("o1") {
        return 200_000;
    }
    if lower.contains("gpt-4o") || lower.contains("gpt-4-turbo") {
        return 128_000;
    }
    // Gemini
    if lower.contains("gemini") {
        return 1_000_000;
    }
    // DeepSeek
    if lower.contains("deepseek") {
        return 64_000;
    }
    // Kimi / Moonshot
    if lower.contains("moonshot") {
        if lower.contains("128k") { return 128_000; }
        if lower.contains("32k") { return 32_000; }
        return 8_000;
    }
    // Minimax
    if lower.contains("abab") {
        return 245_760;
    }
    128_000 // safe default
}

/// Maximum output tokens for a model.
pub fn max_output_tokens_for_model(model: &str) -> u32 {
    let lower = model.to_lowercase();
    // Anthropic
    if lower.contains("claude-3-5") || lower.contains("claude-3-7") {
        return 8_192;
    }
    if lower.contains("claude") {
        return 64_000;
    }
    // OpenAI
    if lower.starts_with("o3") || lower.starts_with("o1") {
        return 100_000;
    }
    if lower.contains("gpt-4o-mini") {
        return 16_384;
    }
    if lower.contains("gpt-4o") || lower.contains("gpt-4-turbo") {
        return 16_384;
    }
    // Gemini
    if lower.contains("gemini-2.5") {
        return 65_536;
    }
    if lower.contains("gemini") {
        return 8_192;
    }
    // Others
    if lower.contains("deepseek") || lower.contains("moonshot") || lower.contains("abab") {
        return 8_192;
    }
    16_384 // safe default
}

// ── Token estimation ──────────────────────────────────────────────────────────

/// Estimate tokens for a single `ContentBlock` using character heuristics.
pub fn estimate_block_tokens(block: &ContentBlock) -> u32 {
    let text = match block {
        ContentBlock::Text(t) => t.text.len(),
        ContentBlock::Thinking(t) => t.thinking.len(),
        ContentBlock::RedactedThinking(t) => t.data.len(),
        ContentBlock::ToolUse(t) => t.name.len() + t.input.to_string().len(),
        ContentBlock::ToolResult(r) => match &r.content {
            code_types::message::ToolResultContent::Text(s) => s.len(),
            code_types::message::ToolResultContent::Blocks(bs) => {
                bs.iter().map(|b| estimate_block_tokens(b) as usize).sum()
            }
        },
        ContentBlock::Image(_) => 1_000, // rough image estimate
    };
    code_api::tokens::estimate_tokens(&"x".repeat(text))
}

/// Estimate total tokens for a slice of messages.
pub fn estimate_messages_tokens(messages: &[Message]) -> u32 {
    messages
        .iter()
        .filter(|m| !m.is_ui_only())
        .flat_map(|m| match m {
            Message::User(u) => u.content.iter().map(estimate_block_tokens).collect::<Vec<_>>(),
            Message::Assistant(a) => a.content.iter().map(estimate_block_tokens).collect(),
            _ => vec![],
        })
        .sum()
}

// ── TokenBudget ───────────────────────────────────────────────────────────────

/// Tracks current token usage against the context window.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub context_window: u32,
    pub max_output_tokens: u32,
    /// Estimated input tokens consumed so far (updated after each API response).
    pub used_input_tokens: u32,
}

impl TokenBudget {
    pub fn for_model(model: &str) -> Self {
        Self {
            context_window: context_window_for_model(model),
            max_output_tokens: max_output_tokens_for_model(model),
            used_input_tokens: 0,
        }
    }

    /// Update with actual token counts from an API response.
    pub fn update(&mut self, input_tokens: u32) {
        self.used_input_tokens = input_tokens;
    }

    /// Fraction of the context window in use (0.0–1.0).
    pub fn fill_ratio(&self) -> f32 {
        code_api::tokens::context_fill_ratio(self.used_input_tokens, self.context_window)
    }

    /// Returns `true` if auto-compact should trigger.
    pub fn should_auto_compact(&self) -> bool {
        code_api::tokens::should_auto_compact(self.used_input_tokens, self.context_window)
    }

    /// Remaining input tokens before the context window is full.
    pub fn remaining_input_tokens(&self) -> u32 {
        self.context_window
            .saturating_sub(self.used_input_tokens)
            .saturating_sub(self.max_output_tokens)
    }

    /// Optional warning message shown in the TUI footer when context is almost full.
    pub fn budget_warning(&self) -> Option<String> {
        let ratio = self.fill_ratio();
        if ratio >= 0.95 {
            Some(format!(
                "Context window {:.0}% full ({}/{} tokens)",
                ratio * 100.0,
                self.used_input_tokens,
                self.context_window,
            ))
        } else if ratio >= code_api::tokens::AUTO_COMPACT_THRESHOLD {
            Some(format!(
                "Context window {:.0}% full — auto-compact will trigger soon",
                ratio * 100.0,
            ))
        } else {
            None
        }
    }
}

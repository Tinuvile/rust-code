//! Per-turn token cost attribution.
//!
//! After each API response, records the token usage and cost so the TUI
//! footer can display per-turn and cumulative costs.
//!
//! Ref: src/utils/attribution.ts

use code_api::cost::{calculate_cost, CostTracker};
use code_types::message::TokenUsage;

/// Token and cost breakdown for a single assistant turn.
#[derive(Debug, Clone, Default)]
pub struct TurnCost {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cache_read_tokens: u32,
    pub cost_usd: f64,
    pub duration_ms: u64,
}

impl TurnCost {
    /// Compute attribution from an API usage block.
    pub fn compute(usage: &TokenUsage, model: &str, duration_ms: u64) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_creation_tokens: usage.cache_creation_input_tokens,
            cache_read_tokens: usage.cache_read_input_tokens,
            cost_usd: calculate_cost(usage, model),
            duration_ms,
        }
    }

    /// Human-readable cost string.
    pub fn format_cost(&self) -> String {
        if self.cost_usd < 0.01 {
            format!("${:.4}", self.cost_usd)
        } else {
            format!("${:.2}", self.cost_usd)
        }
    }
}

/// Accumulates turn-level attribution into a session-level summary.
#[derive(Debug, Default)]
pub struct SessionAttribution {
    pub tracker: CostTracker,
    pub turns: Vec<TurnCost>,
}

impl SessionAttribution {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed turn.
    pub fn record_turn(&mut self, cost: TurnCost, model: &str) {
        let usage = TokenUsage {
            input_tokens: cost.input_tokens,
            output_tokens: cost.output_tokens,
            cache_creation_input_tokens: cost.cache_creation_tokens,
            cache_read_input_tokens: cost.cache_read_tokens,
        };
        self.tracker.record(&usage, model);
        self.turns.push(cost);
    }

    /// Total cost across all turns.
    pub fn total_cost_usd(&self) -> f64 {
        self.tracker.total_cost_usd
    }

    /// Most recent turn attribution.
    pub fn last_turn(&self) -> Option<&TurnCost> {
        self.turns.last()
    }
}

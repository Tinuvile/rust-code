//! Cost tracking — accumulates token usage and converts to USD.
//!
//! Prices are per-million-tokens (MTok). The price table mirrors the
//! public Anthropic pricing page as of the knowledge cutoff.
//!
//! Ref: src/cost-tracker.ts

use std::collections::HashMap;

use code_types::message::TokenUsage;

// ── Price table ───────────────────────────────────────────────────────────────

/// Pricing for a single model, in USD per million tokens.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Input (prompt) tokens per MTok.
    pub input_per_mtok: f64,
    /// Output (completion) tokens per MTok.
    pub output_per_mtok: f64,
    /// Cache write tokens per MTok (prompt caching creation).
    pub cache_write_per_mtok: f64,
    /// Cache read tokens per MTok (prompt caching hits).
    pub cache_read_per_mtok: f64,
}

/// Pricing table keyed by canonical model name prefix.
///
/// Ref: src/cost-tracker.ts (model pricing map)
fn pricing_table() -> &'static [(&'static str, ModelPricing)] {
    &[
        ("claude-opus-4", ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_write_per_mtok: 18.75,
            cache_read_per_mtok: 1.5,
        }),
        ("claude-sonnet-4", ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        }),
        ("claude-haiku-4", ModelPricing {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
            cache_write_per_mtok: 1.0,
            cache_read_per_mtok: 0.08,
        }),
        // claude-3.7 / claude-3.5 fallback
        ("claude-3-7-sonnet", ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        }),
        ("claude-3-5-sonnet", ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        }),
        ("claude-3-5-haiku", ModelPricing {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
            cache_write_per_mtok: 1.0,
            cache_read_per_mtok: 0.08,
        }),
    ]
}

/// Look up pricing for a model.  Falls back to Sonnet 4 pricing if unknown.
pub fn get_pricing(model: &str) -> ModelPricing {
    let lower = model.to_lowercase();
    for (prefix, pricing) in pricing_table() {
        if lower.contains(prefix) {
            return *pricing;
        }
    }
    // Default: Sonnet 4 pricing
    ModelPricing {
        input_per_mtok: 3.0,
        output_per_mtok: 15.0,
        cache_write_per_mtok: 3.75,
        cache_read_per_mtok: 0.30,
    }
}

/// Calculate the cost in USD for a single API response.
pub fn calculate_cost(usage: &TokenUsage, model: &str) -> f64 {
    let p = get_pricing(model);
    let input = usage.input_tokens as f64 / 1_000_000.0 * p.input_per_mtok;
    let output = usage.output_tokens as f64 / 1_000_000.0 * p.output_per_mtok;
    let cache_write = usage.cache_creation_input_tokens as f64
        / 1_000_000.0 * p.cache_write_per_mtok;
    let cache_read = usage.cache_read_input_tokens as f64
        / 1_000_000.0 * p.cache_read_per_mtok;
    input + output + cache_write + cache_read
}

// ── CostTracker ───────────────────────────────────────────────────────────────

/// Per-model usage and cost breakdown.
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    pub request_count: u32,
}

/// Tracks cumulative cost and token usage across all API calls in a session.
///
/// Ref: src/cost-tracker.ts CostTracker
#[derive(Debug, Clone, Default)]
pub struct CostTracker {
    pub total_cost_usd: f64,
    pub per_model: HashMap<String, ModelUsage>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record usage and cost from a completed API call.
    pub fn record(&mut self, usage: &TokenUsage, model: &str) {
        let cost = calculate_cost(usage, model);
        self.total_cost_usd += cost;

        let entry = self.per_model.entry(model.to_owned()).or_default();
        entry.input_tokens += usage.input_tokens as u64;
        entry.output_tokens += usage.output_tokens as u64;
        entry.cache_creation_tokens +=
            usage.cache_creation_input_tokens as u64;
        entry.cache_read_tokens +=
            usage.cache_read_input_tokens as u64;
        entry.cost_usd += cost;
        entry.request_count += 1;
    }

    /// Human-readable cost string, e.g. "$0.0123".
    pub fn format_cost(&self) -> String {
        if self.total_cost_usd < 0.01 {
            format!("${:.4}", self.total_cost_usd)
        } else {
            format!("${:.2}", self.total_cost_usd)
        }
    }

    /// Reset all counters (called on session clear).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

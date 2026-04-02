//! Context compaction: replace old messages with a model-generated summary.
//!
//! Full implementation (Phase 8) calls the API to summarize the conversation.
//! Phase 6 defines the data structures and a stub that returns an error.
//!
//! Ref: src/services/compact/compact.ts

use code_types::message::Message;

// ── Request / Result ──────────────────────────────────────────────────────────

/// Input to a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactRequest {
    /// All messages in the current conversation.
    pub messages: Vec<Message>,
    /// Optional custom instruction appended to the summarization prompt.
    pub custom_prompt: Option<String>,
    /// Model to use for summarization.
    pub model: String,
    /// Max tokens for the summary output.
    pub max_summary_tokens: u32,
}

/// Result of a successful compaction.
#[derive(Debug, Clone)]
pub struct CompactResult {
    /// The model-generated summary text.
    pub summary: String,
    /// Replacement message list (summary + any pinned messages).
    pub replacement_messages: Vec<Message>,
    /// Token count before compaction (heuristic estimate).
    pub tokens_before: u32,
    /// Token count after compaction (heuristic estimate).
    pub tokens_after: u32,
}

// ── Stub implementation ───────────────────────────────────────────────────────

/// Compact the conversation by summarizing early messages.
///
/// **Phase 6 stub** — returns `Err` with a "not yet implemented" message.
/// Phase 8 will replace this with a real API call.
pub async fn compact_conversation(
    _request: CompactRequest,
    _client: &code_api::client::AnthropicClient,
) -> anyhow::Result<CompactResult> {
    Err(anyhow::anyhow!(
        "Context compaction is not yet implemented (planned for Phase 8)."
    ))
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Estimate whether a compaction is warranted based on token usage.
///
/// Returns `true` if `used_tokens` exceeds `threshold_fraction` of
/// `context_window`.  Delegates to `code_api::tokens::should_auto_compact`.
pub fn should_compact(used_tokens: u32, context_window: u32, threshold: f32) -> bool {
    if context_window == 0 {
        return false;
    }
    (used_tokens as f32 / context_window as f32) >= threshold
}

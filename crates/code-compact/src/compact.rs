//! Context compaction: replace old messages with a model-generated summary.
//!
//! Calls the LLM provider to summarize the conversation up to a given point,
//! then returns a minimal replacement message list so the context window can
//! be reclaimed.
//!
//! Ref: src/services/compact/compact.ts

use serde_json::json;

use code_api::tokens::estimate_tokens_json;
use code_types::message::{
    ApiMessage, ApiRole, ContentBlock, Message, TextBlock, UserMessage,
};
use code_types::provider::{LlmProvider, LlmRequest};

use crate::prompt::build_summarization_prompt;

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

// ── Implementation ────────────────────────────────────────────────────────────

/// Compact the conversation by having the model summarize it.
///
/// Strips UI-only messages, calls the model with a summarization prompt, and
/// returns a single replacement `Message::User` containing the summary text.
pub async fn compact_conversation(
    request: CompactRequest,
    provider: &dyn LlmProvider,
) -> anyhow::Result<CompactResult> {
    let system_prompt = build_summarization_prompt(request.custom_prompt.as_deref());

    // Filter to API-eligible messages only.
    let api_messages: Vec<ApiMessage> = request
        .messages
        .iter()
        .filter(|m| !m.is_ui_only())
        .map(|m| match m {
            Message::User(u) => ApiMessage {
                role: ApiRole::User,
                content: u.content.clone(),
            },
            Message::Assistant(a) => ApiMessage {
                role: ApiRole::Assistant,
                content: a.content.clone(),
            },
            // Remaining non-UI messages (Progress, Tombstone, etc.) are omitted.
            _ => ApiMessage {
                role: ApiRole::User,
                content: vec![ContentBlock::text("[omitted]")],
            },
        })
        .collect();

    // Estimate tokens before compaction.
    let before_value = serde_json::to_value(&api_messages).unwrap_or(json!([]));
    let tokens_before = estimate_tokens_json(&before_value);

    let llm_request = LlmRequest {
        model: request.model.clone(),
        messages: api_messages,
        max_tokens: request.max_summary_tokens,
        system: Some(json!([{"type": "text", "text": system_prompt}])),
        tools: vec![],
        temperature: None,
        thinking: None,
        top_p: None,
    };

    let assembled = provider
        .send(llm_request)
        .await
        .map_err(|e| anyhow::anyhow!("compact API error: {e}"))?;

    // Extract the first text block as the summary.
    let summary = assembled
        .content
        .iter()
        .find_map(|b| {
            if let ContentBlock::Text(t) = b {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("summarization model returned no text content"))?;

    // Build the replacement user message.
    let replacement_text = format!(
        "This is a summary of our previous conversation:\n\n{summary}"
    );
    let replacement_messages = vec![Message::User(UserMessage {
        uuid: uuid::Uuid::new_v4(),
        content: vec![ContentBlock::Text(TextBlock {
            text: replacement_text,
            cache_control: None,
        })],
        is_api_error_message: false,
        agent_id: None,
    })];

    let after_value = serde_json::to_value(&replacement_messages).unwrap_or(json!([]));
    let tokens_after = estimate_tokens_json(&after_value);

    Ok(CompactResult {
        summary,
        replacement_messages,
        tokens_before,
        tokens_after,
    })
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Estimate whether a compaction is warranted based on token usage.
///
/// Returns `true` if `used_tokens` exceeds `threshold_fraction` of
/// `context_window`.
pub fn should_compact(used_tokens: u32, context_window: u32, threshold: f32) -> bool {
    if context_window == 0 {
        return false;
    }
    (used_tokens as f32 / context_window as f32) >= threshold
}

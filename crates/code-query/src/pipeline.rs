//! Single API turn pipeline.
//!
//! Takes the current conversation, builds an API request, calls the model,
//! and returns the assembled `AssistantMessage`.
//!
//! Ref: src/utils/queryContext.ts, src/services/api/claude.ts (streamQuery)

use std::time::Instant;

use code_api::client::{AnthropicClient, ApiMessage, ApiTool, MessagesRequest};
use code_api::retry::RetryPolicy;
use code_types::message::{AssistantMessage, ContentBlock, Message, TokenUsage};
use uuid::Uuid;

use crate::attribution::TurnCost;
use crate::message_queue::MessageQueue;
use crate::messages::normalize_messages_for_api;
use crate::token_budget::max_output_tokens_for_model;

// ── Pipeline configuration ────────────────────────────────────────────────────

/// Configuration for a single API pipeline turn.
pub struct PipelineConfig {
    /// Model identifier.
    pub model: String,
    /// System prompt content blocks.
    pub system: Vec<ContentBlock>,
    /// Tool definitions sent to the model.
    pub tools: Vec<ApiTool>,
    /// Extended thinking configuration.
    pub thinking: Option<code_api::client::ThinkingConfig>,
    /// Retry policy for transient API errors.
    pub retry_policy: RetryPolicy,
}

impl PipelineConfig {
    /// Create a minimal config for the given model.
    pub fn new(model: impl Into<String>, system: Vec<ContentBlock>) -> Self {
        Self {
            model: model.into(),
            system,
            tools: Vec::new(),
            thinking: None,
            retry_policy: RetryPolicy::default(),
        }
    }
}

// ── Run a single pipeline turn ────────────────────────────────────────────────

/// Execute one API request turn.
///
/// 1. Normalize `conversation` to API format (strip UI-only messages).
/// 2. Build a `MessagesRequest`.
/// 3. Send via `client.stream()` with retry.
/// 4. Convert `AssembledResponse` to `AssistantMessage`.
/// 5. Publish the `AssistantMessage` to `queue`.
/// 6. Return `(AssistantMessage, TurnCost)`.
pub async fn run_pipeline_turn(
    conversation: &[Message],
    config: &PipelineConfig,
    client: &AnthropicClient,
    queue: &MessageQueue,
) -> anyhow::Result<(AssistantMessage, TurnCost)> {
    let start = Instant::now();

    // Normalize messages.
    let api_messages: Vec<ApiMessage> = normalize_messages_for_api(conversation)
        .into_iter()
        .map(|m| ApiMessage {
            role: match m.role {
                code_types::message::ApiRole::User => code_api::client::ApiRole::User,
                code_types::message::ApiRole::Assistant => code_api::client::ApiRole::Assistant,
            },
            content: m.content,
        })
        .collect();

    if api_messages.is_empty() {
        anyhow::bail!("cannot send an empty conversation to the API");
    }

    let max_tokens = max_output_tokens_for_model(&config.model);

    // Build system — convert to serde_json::Value if non-empty.
    let system_value = if config.system.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&config.system)?)
    };

    let request = MessagesRequest {
        model: config.model.clone(),
        messages: api_messages,
        max_tokens,
        system: system_value,
        tools: config.tools.clone(),
        stream: true,
        temperature: None,
        thinking: config.thinking.clone(),
        top_p: None,
    };

    // Call the API.
    let assembled = client
        .stream(request, config.retry_policy.clone())
        .await
        .map_err(|e| anyhow::anyhow!("API error: {e:?}"))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Build AssistantMessage.
    let usage = TokenUsage {
        input_tokens: assembled.usage.input_tokens,
        output_tokens: assembled.usage.output_tokens,
        cache_creation_input_tokens: assembled.usage.cache_creation_input_tokens,
        cache_read_input_tokens: assembled.usage.cache_read_input_tokens,
    };

    let assistant_msg = AssistantMessage {
        uuid: Uuid::new_v4(),
        content: assembled.content,
        model: assembled.model.clone(),
        stop_reason: assembled.stop_reason.clone(),
        usage: usage.clone(),
        agent_id: None,
    };

    // Compute cost attribution.
    let turn_cost = TurnCost::compute(&usage, &assembled.model, duration_ms);

    // Publish to message queue.
    queue.publish(Message::Assistant(assistant_msg.clone()));

    Ok((assistant_msg, turn_cost))
}

//! Streaming event types from the Anthropic Messages API (SSE).
//!
//! Ref: src/types/message.ts StreamEvent, RequestStartEvent
//! Ref: @anthropic-ai/sdk streaming event types

use serde::{Deserialize, Serialize};

use crate::message::{ContentBlock, ToolUseBlock, TokenUsage};

// ── SSE event types ───────────────────────────────────────────────────────────

/// The full set of SSE events emitted by the Anthropic Messages API.
///
/// Emitted by `code_api::stream` and consumed by the query pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Very first event — contains message ID, model, and initial usage.
    MessageStart { message: MessageStartPayload },
    /// A content block (text, tool_use, thinking) is beginning.
    ContentBlockStart { index: usize, content_block: ContentBlockStart },
    /// Incremental delta within an open content block.
    ContentBlockDelta { index: usize, delta: ContentBlockDelta },
    /// A content block has finished streaming.
    ContentBlockStop { index: usize },
    /// Final event — stop reason and final token counts.
    MessageDelta { delta: MessageDeltaPayload, usage: MessageDeltaUsage },
    /// The entire message is complete.
    MessageStop,
    /// Server-sent keep-alive.
    Ping,
    /// API-level error embedded in the stream.
    Error { error: StreamError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStartPayload {
    pub id: String,
    pub model: String,
    pub usage: TokenUsage,
}

/// What type of content block is opening.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockStart {
    Text { text: String },
    Thinking { thinking: String },
    RedactedThinking { data: String },
    ToolUse(ToolUseBlock),
}

/// Incremental data for an open content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    InputJsonDelta { partial_json: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaPayload {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaUsage {
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

// ── Assembled stream result ───────────────────────────────────────────────────

/// A fully assembled assistant message built by consuming a stream.
///
/// Produced by `code_api::stream::collect_stream()`.
#[derive(Debug, Clone)]
pub struct AssembledResponse {
    pub message_id: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: TokenUsage,
}

// ── Request start event ───────────────────────────────────────────────────────

/// Emitted by the query engine when a new API request starts.
///
/// Used by the TUI to show the "thinking…" spinner and by analytics.
///
/// Ref: src/types/message.ts RequestStartEvent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStartEvent {
    pub request_id: Option<String>,
    pub model: String,
    pub timestamp_ms: u64,
}

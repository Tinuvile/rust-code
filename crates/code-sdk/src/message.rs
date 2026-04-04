//! SDK message types: the public wire format used by SDK consumers.
//!
//! Mirrors `SDKMessage` from `src/entrypoints/sdk/coreTypes.ts`.

use serde::{Deserialize, Serialize};

// ── SDKMessage ────────────────────────────────────────────────────────────────

/// The discriminated union of all messages the SDK emits on its output stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SdkMessage {
    /// A user turn message.
    UserMessage(SdkUserMessage),
    /// An assistant turn message.
    AssistantMessage(SdkAssistantMessage),
    /// A tool use block within an assistant turn.
    ToolUse(SdkToolUseMessage),
    /// A tool result block.
    ToolResult(SdkToolResultMessage),
    /// A system-level informational event.
    System(SdkSystemMessage),
    /// The final result of a complete query.
    Result(SdkResultMessage),
}

// ── Individual message types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkUserMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkAssistantMessage {
    pub role: String,
    /// Full text of the assistant's response (concatenated text blocks).
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkToolUseMessage {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkToolResultMessage {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkSystemMessage {
    pub subtype: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Emitted as the final line of an SDK stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkResultMessage {
    /// `"success"` or `"error"`.
    pub subtype: String,
    /// Total cost in USD for this turn.
    pub cost_usd: f64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Total input tokens.
    pub input_tokens: u64,
    /// Total output tokens.
    pub output_tokens: u64,
    /// Session id.
    pub session_id: String,
    /// Optional error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Conversion from internal messages ────────────────────────────────────────

impl SdkMessage {
    /// Serialize this message as a single JSON line (for stream-json output).
    pub fn to_json_line(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

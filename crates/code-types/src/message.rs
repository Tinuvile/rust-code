//! Message type system — the "currency" flowing through the entire application.
//!
//! Every part of the system (query engine, TUI, history, compact, hooks)
//! produces or consumes these types.
//!
//! Ref: src/types/message.ts, src/utils/messages.ts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::ids::AgentId;
use crate::permissions::PermissionDecisionReason;

// ── Content blocks (mirror Anthropic SDK types) ───────────────────────────────

/// Cache control hint for prompt caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String, // "ephemeral"
}

/// Plain text content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Extended thinking block (model's internal reasoning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    pub thinking: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Redacted thinking block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedThinkingBlock {
    pub data: String,
}

/// A tool call issued by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseBlock {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// The result of executing a tool, returned to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: ToolResultContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// An image content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBlock {
    pub source: ImageSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// Any content block that can appear in a message.
///
/// Ref: @anthropic-ai/sdk ContentBlock union
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text(TextBlock),
    Thinking(ThinkingBlock),
    RedactedThinking(RedactedThinkingBlock),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    Image(ImageBlock),
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        ContentBlock::Text(TextBlock { text: s.into(), cache_control: None })
    }
}

// ── Token usage ──────────────────────────────────────────────────────────────

/// Token usage for a single API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

// ── Message variants ──────────────────────────────────────────────────────────

/// A message from the human turn.
///
/// Ref: src/types/message.ts UserMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub uuid: Uuid,
    pub content: Vec<ContentBlock>,
    pub is_api_error_message: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

/// A message from the assistant turn (raw API response).
///
/// Ref: src/types/message.ts AssistantMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub uuid: Uuid,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: TokenUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

/// Realtime progress update from a running tool (shown in TUI, not sent to API).
///
/// Ref: src/types/message.ts ProgressMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    pub uuid: Uuid,
    pub tool_use_id: String,
    pub tool_name: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

/// Marks a message slot as deleted without shifting indices.
///
/// Ref: src/types/message.ts TombstoneMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneMessage {
    pub uuid: Uuid,
}

/// Memory / code.md file injected as a system attachment (not sent to API verbatim).
///
/// Ref: src/types/message.ts AttachmentMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub uuid: Uuid,
    pub content: String,
    pub attachment_type: AttachmentType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentType {
    Memory,
    /// Renamed from claude_md to code_md to avoid trademark concerns.
    CodeMd,
    NestedMemory,
    Skill,
}

/// Summarised representation of tool calls used after compact.
///
/// Ref: src/types/message.ts ToolUseSummaryMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseSummaryMessage {
    pub uuid: Uuid,
    pub summary: String,
    pub tool_names: Vec<String>,
}

// ── System messages (UI-only, stripped before API call) ───────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemMessageLevel {
    Info,
    Warning,
    Error,
}

/// Generic informational system message shown in the TUI.
///
/// Ref: src/types/message.ts SystemInformationalMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInformationalMessage {
    pub uuid: Uuid,
    pub content: String,
    pub level: SystemMessageLevel,
}

/// Marks the boundary where a context compact happened.
///
/// Ref: src/types/message.ts SystemCompactBoundaryMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCompactBoundaryMessage {
    pub uuid: Uuid,
    pub summary: String,
    pub tokens_before: u32,
    pub tokens_after: u32,
    pub direction: CompactDirection,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactDirection {
    Forward,
    Backward,
}

/// Lightweight in-place compact of a large tool result.
///
/// Ref: src/types/message.ts SystemMicrocompactBoundaryMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMicrocompactBoundaryMessage {
    pub uuid: Uuid,
    pub tool_use_id: String,
}

/// An API error shown inline in the conversation.
///
/// Ref: src/types/message.ts SystemAPIErrorMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemApiErrorMessage {
    pub uuid: Uuid,
    pub error: String,
    pub is_retryable: bool,
}

/// Permission retry hint shown after a tool denial.
///
/// Ref: src/types/message.ts SystemPermissionRetryMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPermissionRetryMessage {
    pub uuid: Uuid,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
}

/// "Memory saved" confirmation shown in the TUI.
///
/// Ref: src/types/message.ts SystemMemorySavedMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMemorySavedMessage {
    pub uuid: Uuid,
    pub memory_type: String,
    pub path: String,
}

/// How long a turn took (shown in the footer after each response).
///
/// Ref: src/types/message.ts SystemTurnDurationMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTurnDurationMessage {
    pub uuid: Uuid,
    pub duration_ms: u64,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub cost_usd: f64,
}

// ── Top-level Message enum ────────────────────────────────────────────────────

/// All message variants that can appear in a conversation.
///
/// `System*` variants are UI-only and stripped by `normalize_messages_for_api()`
/// before being sent to the Anthropic API.
///
/// Ref: src/types/message.ts Message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    Progress(ProgressMessage),
    Tombstone(TombstoneMessage),
    Attachment(AttachmentMessage),
    ToolUseSummary(ToolUseSummaryMessage),
    // System (UI-only) variants:
    SystemInformational(SystemInformationalMessage),
    SystemCompactBoundary(SystemCompactBoundaryMessage),
    SystemMicrocompactBoundary(SystemMicrocompactBoundaryMessage),
    SystemApiError(SystemApiErrorMessage),
    SystemPermissionRetry(SystemPermissionRetryMessage),
    SystemMemorySaved(SystemMemorySavedMessage),
    SystemTurnDuration(SystemTurnDurationMessage),
}

impl Message {
    pub fn uuid(&self) -> Uuid {
        match self {
            Message::User(m) => m.uuid,
            Message::Assistant(m) => m.uuid,
            Message::Progress(m) => m.uuid,
            Message::Tombstone(m) => m.uuid,
            Message::Attachment(m) => m.uuid,
            Message::ToolUseSummary(m) => m.uuid,
            Message::SystemInformational(m) => m.uuid,
            Message::SystemCompactBoundary(m) => m.uuid,
            Message::SystemMicrocompactBoundary(m) => m.uuid,
            Message::SystemApiError(m) => m.uuid,
            Message::SystemPermissionRetry(m) => m.uuid,
            Message::SystemMemorySaved(m) => m.uuid,
            Message::SystemTurnDuration(m) => m.uuid,
        }
    }

    /// True if this message variant is UI-only and must be stripped before API calls.
    ///
    /// Ref: src/utils/messages.ts normalizeMessagesForAPI — strips these variants.
    pub fn is_ui_only(&self) -> bool {
        matches!(
            self,
            Message::SystemInformational(_)
                | Message::SystemCompactBoundary(_)
                | Message::SystemMicrocompactBoundary(_)
                | Message::SystemApiError(_)
                | Message::SystemPermissionRetry(_)
                | Message::SystemMemorySaved(_)
                | Message::SystemTurnDuration(_)
                | Message::Progress(_)
                | Message::Tombstone(_)
                | Message::Attachment(_)
                | Message::ToolUseSummary(_)
        )
    }

    pub fn agent_id(&self) -> Option<&AgentId> {
        match self {
            Message::User(m) => m.agent_id.as_ref(),
            Message::Assistant(m) => m.agent_id.as_ref(),
            Message::Progress(m) => m.agent_id.as_ref(),
            _ => None,
        }
    }
}

// ── Convenience constructors ──────────────────────────────────────────────────

impl UserMessage {
    pub fn new(content: Vec<ContentBlock>) -> Self {
        Self { uuid: Uuid::new_v4(), content, is_api_error_message: false, agent_id: None }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::new(vec![ContentBlock::text(text)])
    }
}

impl AssistantMessage {
    pub fn new(content: Vec<ContentBlock>, model: impl Into<String>, usage: TokenUsage) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            content,
            model: model.into(),
            stop_reason: None,
            usage,
            agent_id: None,
        }
    }
}

// ── API-normalised message pair ───────────────────────────────────────────────

/// Role-tagged message suitable for direct API serialization.
///
/// Produced by `code_query::messages::normalize_messages_for_api()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: ApiRole,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiRole {
    User,
    Assistant,
}

// ── Session metadata ─────────────────────────────────────────────────────────

/// Persisted metadata for a session (written to JSONL header).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: crate::ids::SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub created_at: u64,
    pub last_active_at: u64,
    pub model: String,
    pub total_cost_usd: f64,
    pub message_count: u32,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

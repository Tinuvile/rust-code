//! Lightweight tool-related types shared across the workspace.
//!
//! The full `Tool` trait lives in `code-tools` to avoid pulling heavy
//! dependencies into `code-types`.  This module holds only the types
//! that multiple crates need to reference without depending on code-tools.
//!
//! Ref: src/Tool.ts

use serde::{Deserialize, Serialize};

// ── Validation ────────────────────────────────────────────────────────────────

/// Result of validating a tool's input before execution.
///
/// Ref: src/Tool.ts ValidationResult
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Ok,
    Err { message: String, error_code: i32 },
}

impl ValidationResult {
    pub fn ok() -> Self { Self::Ok }
    pub fn err(message: impl Into<String>, error_code: i32) -> Self {
        Self::Err { message: message.into(), error_code }
    }
    pub fn is_ok(&self) -> bool { matches!(self, Self::Ok) }
}

// ── Tool input JSON Schema ────────────────────────────────────────────────────

/// Opaque JSON Schema object describing a tool's accepted input.
///
/// Must have `"type": "object"` at the root.
///
/// Ref: src/Tool.ts ToolInputJSONSchema
pub type ToolInputSchema = serde_json::Value;

// ── Tool result ───────────────────────────────────────────────────────────────

/// The output of a tool execution, returned to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: ToolResultPayload,
    pub is_error: bool,
    /// True if the result was truncated and full content stored on disk.
    pub was_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultPayload {
    Text(String),
    Json(serde_json::Value),
}

impl ToolResultPayload {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Json(_) => None,
        }
    }
}

// ── Progress reporting ────────────────────────────────────────────────────────

/// Progress data emitted by a tool during execution (streamed to TUI).
///
/// Ref: src/types/tools.ts (BashProgress, AgentToolProgress, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress {
    pub tool_use_id: String,
    pub tool_name: String,
    /// Tool-specific JSON payload (bash line, agent status, etc.).
    pub data: serde_json::Value,
}

// ── Minimal tool descriptor ───────────────────────────────────────────────────

/// Metadata about a tool, used in permission checks and UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub is_read_only: bool,
}

// ── File / glob limits ────────────────────────────────────────────────────────

/// Per-call limits for file reading operations.
///
/// Ref: src/Tool.ts ToolUseContext.fileReadingLimits
#[derive(Debug, Clone, Default)]
pub struct FileReadingLimits {
    pub max_tokens: Option<usize>,
    pub max_size_bytes: Option<usize>,
}

/// Per-call limits for glob operations.
///
/// Ref: src/Tool.ts ToolUseContext.globLimits
#[derive(Debug, Clone, Default)]
pub struct GlobLimits {
    pub max_results: Option<usize>,
}

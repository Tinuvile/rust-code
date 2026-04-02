//! Tool trait definition and all tool implementations.
//!
//! Ref: src/Tool.ts, src/tools.ts, src/services/tools/toolOrchestration.ts

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{
    FileReadingLimits, GlobLimits, ToolInputSchema, ToolResult, ValidationResult,
};
use code_types::permissions::ToolPermissionContext;

pub mod hooks_stub;
pub mod registry;
pub mod orchestration;
pub mod execution;
pub mod result_storage;
pub mod progress;

// Tool implementations (Tier 1 — core)
pub mod bash;
pub mod file_read;
pub mod file_write;
pub mod file_edit;
pub mod grep;
pub mod glob;

// Tool implementations (Tier 2 — important)
pub mod web_fetch;
pub mod web_search;
pub mod ask_user;
pub mod todo_write;
pub mod notebook_edit;

// Tool implementations (Tier 3 — specialized)
pub mod mcp_tool;
pub mod task_tools;
pub mod plan_mode;
pub mod worktree;
pub mod config_tool;
pub mod tool_search;
pub mod lsp;
pub mod powershell;
pub mod synthetic_output;
pub mod brief;

// Feature-gated tools
#[cfg(any(feature = "proactive", feature = "kairos"))]
pub mod sleep;

#[cfg(feature = "agent_triggers")]
pub mod cron;

#[cfg(feature = "agent_triggers_remote")]
pub mod remote_trigger;

#[cfg(feature = "coordinator_mode")]
pub mod coordinator_tools;

#[cfg(feature = "monitor_tool")]
pub mod monitor;

// Re-export core types for ergonomic use by downstream crates.
pub use hooks_stub::{NoopHookRunner, PostToolHookResult, PreToolHookResult, ToolHookRunner};
pub use progress::ProgressSender;
pub use registry::ToolRegistry;

// ── ToolContext ───────────────────────────────────────────────────────────────

/// Runtime context provided to every tool invocation.
///
/// Mirrors the subset of `ToolUseContext` that tools actually need.
/// Ref: src/Tool.ts ToolUseContext
#[derive(Clone)]
pub struct ToolContext {
    /// Current working directory for the session.
    pub cwd: PathBuf,
    /// Unique session identifier (used for result storage paths).
    pub session_id: String,
    /// Directory for persisting large tool results.
    /// Typically `~/.claude/sessions/{session_id}/`.
    pub session_dir: PathBuf,
    /// Snapshot of permission configuration for this invocation.
    pub permission_ctx: ToolPermissionContext,
    /// Optional limits for file-reading operations.
    pub file_reading_limits: FileReadingLimits,
    /// Optional limits for glob operations.
    pub glob_limits: GlobLimits,
}

// ── ProgressSender type alias lives in progress.rs ───────────────────────────
// pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<ToolProgress>;

// ── Tool trait ────────────────────────────────────────────────────────────────

/// The core trait implemented by every Claude Code tool.
///
/// Object-safe so tools can be stored as `Box<dyn Tool>` in a registry.
/// All I/O uses `serde_json::Value` at the trait boundary; individual tools
/// parse their typed input structs internally with `serde_json::from_value`.
///
/// Ref: src/Tool.ts Tool<Input, Output, P>
#[async_trait]
pub trait Tool: Send + Sync {
    // ── Identity ──────────────────────────────────────────────────────────────

    /// Canonical tool name used in API calls, permission rules, and the TUI.
    fn name(&self) -> &str;

    /// Human-readable description sent to the model (appears in the system prompt).
    fn description(&self) -> &str;

    /// JSON Schema for the tool's input (must have `"type": "object"` at root).
    fn input_schema(&self) -> ToolInputSchema;

    // ── Capability flags ──────────────────────────────────────────────────────

    /// Whether this tool is classified as read-only for a given input.
    ///
    /// Read-only tools skip the user-permission prompt in `Default` mode and
    /// are allowed to run concurrently.
    fn is_read_only(&self, input: &serde_json::Value) -> bool;

    /// Whether multiple concurrent calls to this tool with distinct inputs
    /// are safe.  Defaults to `is_read_only(input)`.
    fn is_concurrency_safe(&self, input: &serde_json::Value) -> bool {
        self.is_read_only(input)
    }

    /// Whether this tool is available in the current environment.
    /// Can be overridden to disable tools when prerequisites are missing
    /// (e.g. `rg` not in PATH for GrepTool).
    fn is_enabled(&self) -> bool {
        true
    }

    // ── Validation & permission ───────────────────────────────────────────────

    /// Perform tool-specific semantic validation *before* the permission check.
    ///
    /// The input has already passed basic schema validation at this point.
    /// Return `ValidationResult::Err` to short-circuit with an error `ToolResult`
    /// without prompting the user.
    async fn validate_input(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> ValidationResult {
        let _ = (input, ctx);
        ValidationResult::ok()
    }

    /// Build the `ToolCallContext` used by `PermissionEvaluator`.
    ///
    /// Override to provide a meaningful `content` string (file path, bash
    /// command, …) so that rule matching works correctly.
    fn permission_context<'a>(
        &'a self,
        input: &'a serde_json::Value,
        cwd: &'a Path,
    ) -> ToolCallContext<'a>;

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Execute the tool.  Called only after permission has been granted.
    ///
    /// * `tool_use_id` — correlates the result with the model's `tool_use` block.
    /// * `input`       — validated JSON input (may have been updated by hooks).
    /// * `ctx`         — runtime context (cwd, limits, …).
    /// * `progress`    — optional channel to stream intermediate results to the TUI.
    async fn call(
        &self,
        tool_use_id: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        progress: Option<&ProgressSender>,
    ) -> ToolResult;
}

// ── Convenience helpers ───────────────────────────────────────────────────────

/// Build an error `ToolResult` for the given tool use ID and message.
pub fn error_result(tool_use_id: impl Into<String>, message: impl Into<String>) -> ToolResult {
    ToolResult {
        tool_use_id: tool_use_id.into(),
        content: code_types::tool::ToolResultPayload::Text(message.into()),
        is_error: true,
        was_truncated: false,
    }
}

/// Build a successful text `ToolResult`.
pub fn ok_result(tool_use_id: impl Into<String>, text: impl Into<String>) -> ToolResult {
    ToolResult {
        tool_use_id: tool_use_id.into(),
        content: code_types::tool::ToolResultPayload::Text(text.into()),
        is_error: false,
        was_truncated: false,
    }
}
